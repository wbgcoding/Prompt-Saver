use super::*;

// ---- the backfill never writes text for a picture that is gone ----

#[test]
fn backfill_text_is_dropped_when_the_picture_changed_meanwhile() {
    let mut p = test_prompt("a");
    p.image = "data:image/png;base64,NEW".to_string();
    p.image_text = String::new();
    assert!(!apply_backfill_text(&mut p, "data:image/png;base64,OLD", "old words".to_string()));
    assert!(p.image_text.is_empty());
    p.image = String::new();
    assert!(!apply_backfill_text(&mut p, "data:image/png;base64,OLD", "old words".to_string()));
    assert!(p.image_text.is_empty());
    p.image = "data:image/png;base64,SAME".to_string();
    assert!(apply_backfill_text(&mut p, "data:image/png;base64,SAME", "words".to_string()));
    assert_eq!(p.image_text, "words");
    assert!(!apply_backfill_text(&mut p, "data:image/png;base64,SAME", "other".to_string()));
    assert_eq!(p.image_text, "words");
}

// ---- text recognition sees a photo upright ----

// A 2x1 JPEG carrying an EXIF Orientation tag of 6 (rotate 90 degrees clockwise).
pub(super) fn jpeg_with_orientation(orientation: u8) -> Vec<u8> {
    let mut plain = Vec::new();
    image::DynamicImage::new_rgb8(2, 1)
        .write_to(&mut std::io::Cursor::new(&mut plain), image::ImageFormat::Jpeg)
        .unwrap();
    let mut exif: Vec<u8> = b"Exif\0\0MM\0*\0\0\0\x08".to_vec();
    exif.extend_from_slice(&[0, 1, 0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, orientation, 0, 0, 0, 0, 0, 0]);
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
    out.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&exif);
    out.extend_from_slice(&plain[2..]);
    out
}

#[test]
fn decode_image_oriented_applies_the_exif_rotation() {
    let bytes = jpeg_with_orientation(6);
    let raw = decode_image_mem(&bytes).expect("decodes");
    assert_eq!((raw.width(), raw.height()), (2, 1));
    let up = decode_image_oriented(image::ImageReader::new(std::io::Cursor::new(&bytes))).expect("decodes");
    assert_eq!((up.width(), up.height()), (1, 2));
    let flat = jpeg_with_orientation(1);
    let same = decode_image_oriented(image::ImageReader::new(std::io::Cursor::new(&flat))).expect("decodes");
    assert_eq!((same.width(), same.height()), (2, 1));
}

#[test]
fn decode_image_oriented_handles_every_orientation_tag() {
    // (tag, expected size of the 2x1 source after the transform)
    for (tag, size) in [(1u8, (2, 1)), (2, (2, 1)), (3, (2, 1)), (4, (2, 1)), (5, (1, 2)), (6, (1, 2)), (7, (1, 2)), (8, (1, 2)), (0, (2, 1)), (9, (2, 1))] {
        let bytes = jpeg_with_orientation(tag);
        let img = decode_image_oriented(image::ImageReader::new(std::io::Cursor::new(&bytes))).expect("decodes");
        assert_eq!((img.width(), img.height()), size, "orientation tag {tag}");
    }
}

#[test]
fn save_screenshot_skips_a_name_taken_by_a_directory() {
    let dir = std::env::temp_dir().join(format!("{}-shotdir-{}", brand::TEMP_PREFIX, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("Shot.png")).unwrap();
    let p = save_screenshot(b"x", &dir, "Shot", "png");
    assert!(p.ends_with("Shot (2).png"), "{p}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn decode_image_oriented_enforces_the_alloc_cap() {
    let mut png = Vec::new();
    image::DynamicImage::new_rgba8(100, 100)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let mut tight = image::Limits::default();
    tight.max_alloc = Some(1000);
    let r = decode_image_with_orientation_limits(image::ImageReader::new(std::io::Cursor::new(&png)), tight);
    assert!(r.is_none(), "40000 decoded bytes must exceed a 1000 byte cap");
    assert!(decode_image_mem_oriented(&png).is_some());
}

#[test]
fn a_cancelled_capture_leaves_no_folder_file() {
    let dir = std::env::temp_dir().join(format!("{}-cancel-{}", brand::TEMP_PREFIX, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let path = save_screenshot(b"x", &dir, "Screenshot-cancel", "png");
    assert!(Path::new(&path).exists());
    discard_cancelled_file(&path);
    assert!(!Path::new(&path).exists());
    discard_cancelled_file("");
    let _ = fs::remove_dir_all(&dir);
}

// ---- two shots with the same stem never overwrite each other ----

#[test]
fn save_screenshot_keeps_both_files_for_the_same_stem() {
    let dir = std::env::temp_dir().join(format!("{}-shot-{}", brand::TEMP_PREFIX, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let first = save_screenshot(b"one", &dir, "Screenshot-2026-01-01_00-00-00", "png");
    let second = save_screenshot(b"two", &dir, "Screenshot-2026-01-01_00-00-00", "png");
    assert!(!first.is_empty() && !second.is_empty());
    assert_ne!(first, second);
    assert_eq!(fs::read(&first).unwrap(), b"one");
    assert_eq!(fs::read(&second).unwrap(), b"two");
    let _ = fs::remove_dir_all(&dir);
}

// ---- a stale preview encode never lands in the slot ----

#[test]
fn a_stale_preview_encode_leaves_the_slot_alone() {
    let mut slot: Option<(u64, i32)> = None;
    assert!(!preview_slot_store(&mut slot, 2, 1, 7));
    assert!(slot.is_none());
    assert!(preview_slot_store(&mut slot, 2, 2, 9));
    assert_eq!(slot, Some((2, 9)));
}

// ---- a capture of a cancelled session finishes without side effects ----

#[test]
fn finalize_is_refused_for_a_session_that_ended_or_was_replaced() {
    assert!(finalize_allowed(2, 2, true));
    assert!(!finalize_allowed(1, 2, true));
    assert!(!finalize_allowed(1, 1, false));
}

// ---- the overlay hit-tests a window by its frame, not its client area ----

#[test]
fn snip_window_rect_prefers_the_frame() {
    let client = (0, 30, 100, 70);
    assert_eq!(snip_window_rect(Some((0, 0, 100, 100)), client), (0, 0, 100, 100));
    assert_eq!(snip_window_rect(None, client), client);
}

// ---- the install result file is read with its byte-order mark ----

#[test]
fn install_result_parses_past_a_byte_order_mark() {
    let ok = ocr::parse_install_result("\u{feff}{\r\n  \"ok\": true\r\n}\r\n").expect("parsed");
    assert!(ok.ok && ok.error.is_none());
    let bad = ocr::parse_install_result("\u{feff}{\"ok\":false,\"error\":\"x\"}").expect("parsed");
    assert!(!bad.ok);
    assert_eq!(bad.error.as_deref(), Some("x"));
    assert!(ocr::parse_install_result("{\"ok\":").is_none());
}

// ---- a start always brings up a window ----

#[test]
fn show_main_rebuilds_the_main_window_when_it_is_gone() {
    let mut builds = 0;
    let win = show_main_seam(None::<()>, || {
        builds += 1;
        Ok(())
    });
    assert_eq!(builds, 1, "a missing window must trigger exactly one rebuild");
    assert!(win.is_some());
}

#[test]
fn show_main_never_rebuilds_a_window_that_is_already_there() {
    let mut builds = 0;
    let win = show_main_seam(Some(()), || {
        builds += 1;
        Ok(())
    });
    assert_eq!(builds, 0, "an existing window must never be rebuilt");
    assert!(win.is_some());
}

#[test]
fn show_main_seam_gives_up_quietly_when_the_rebuild_itself_fails() {
    let win = show_main_seam(None::<()>, || {
        Err(tauri::Error::AssetNotFound("test".into()))
    });
    assert!(win.is_none());
}

#[test]
fn a_destroyed_main_window_ends_the_process() {
    let deadline = UNIX_EPOCH + Duration::from_secs(100);
    assert!(!watchdog_deadline_reached(UNIX_EPOCH + Duration::from_secs(99), deadline), "must not fire before the grace period is up");
    assert!(watchdog_deadline_reached(UNIX_EPOCH + Duration::from_secs(100), deadline), "must fire right at the deadline");
    assert!(watchdog_deadline_reached(UNIX_EPOCH + Duration::from_secs(200), deadline), "must fire well past the deadline too");
}

#[test]
fn the_screenshot_delete_recognises_current_and_legacy_names() {
    use crate::snip::is_own_screenshot_name as own;
    for ext in ["jpg", "png", "webp", "bmp", "tiff"] {
        assert!(own(&format!("Screenshot-2026-01-01.{ext}"), ext));
    }
    assert!(!own("Screenshot-2026-01-01.gif", "gif"));
    assert!(!own("holiday.png", "png"));
    // A shot an older version saved keeps being recognised, but only as a png.
    let legacy = format!("{}2024-05-05.png", crate::brand::LEGACY_SCREENSHOT_PREFIX);
    assert!(own(&legacy, "png"));
    assert!(!own(&format!("{}2024-05-05.jpg", crate::brand::LEGACY_SCREENSHOT_PREFIX), "jpg"));
}

#[test]
fn the_editor_save_accepts_only_a_plain_capture_stem() {
    use crate::snip::is_shot_stem as ok;
    assert!(ok("Screenshot-Explorer-2026-10-24_17-23-41"));
    assert!(ok("Screenshot-2026-10-24_17-23-41"));
    for bad in ["holiday", r"Screenshot-..\..\evil", "Screenshot-a/b", "Screenshot-C:x", "Screenshot-a.", "Screenshot-a ", "Screenshot-a\nb"] {
        assert!(!ok(bad), "{bad:?}");
    }
    assert!(!ok(&format!("Screenshot-{}", "a".repeat(200))));
}

// The checks below compare multi-line source text; a checkout with Windows line endings
// must read the same as one with Unix endings.
macro_rules! source {
    ($path:literal) => {
        include_str!($path).replace("\r\n", "\n")
    };
}

#[test]
fn window_capture_picks_the_monitor_under_its_centre() {
    let mons = [(0, 0, 1920u32, 1080u32), (1920, 0, 1920, 1080), (-1280, 100, 1280, 1024)];
    assert_eq!(monitor_index_for_rect(&mons, 100, 100, 800, 600), 0);
    assert_eq!(monitor_index_for_rect(&mons, 2500, 200, 800, 600), 1);
    assert_eq!(monitor_index_for_rect(&mons, -1000, 300, 600, 400), 2);
    // A window nowhere near any screen keeps the primary capture.
    assert_eq!(monitor_index_for_rect(&mons, 9000, 9000, 100, 100), 0);
}

// Structural fix (the overlay window cannot be built without a running app), so the check
// reads the source: a failed overlay build must end the whole session, not just clear a flag.
#[test]
fn a_failed_overlay_build_ends_the_whole_snip_session() {
    let src = source!("../snip.rs");
    let arm = src.split("match build_snip_window(&app) {").nth(1).unwrap().split("Ok(w) => w,").nth(1).unwrap();
    let arm = arm.split("return Err(e);").next().unwrap();
    assert!(arm.contains("close_all_snip(&app)"));
    assert!(!arm.contains("SNIP_ACTIVE.store(false"), "clearing one flag leaves the rest of the session behind");
}

// Structural fix, no red unit test: the rect comes from a live window enumeration, which a unit
// test cannot fake. The pure choice (frame over client) is covered by snip_window_rect_prefers_the_frame.
#[test]
fn the_window_fallback_crops_the_frame_the_overlay_showed() {
    let src = source!("../capture.rs");
    // The rect comes from the one window probe, which crops the visible frame the overlay showed.
    let at = src.find("pub(crate) fn probe_window(").expect("probe");
    assert!(src[at..at + 600].contains("snip_window_rect(window_frame_rect(id)"));
    assert!(src.contains("match probe.rect"));
}

#[test]
fn an_attached_photo_previews_upright() {
    let dir = std::env::temp_dir().join(format!("ps-photo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("sideways.jpg");
    // 2x1 pixels with an orientation flag of 6: shown upright it is 1x2.
    std::fs::write(&file, shots::jpeg_with_orientation(6)).unwrap();
    let url = image_file_preview(&file.to_string_lossy()).expect("previews");
    let img = decode_image_mem(&data_url_bytes(&url)).expect("the preview decodes");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!((img.width(), img.height()), (1, 2));
}

#[test]
fn the_camera_page_asks_for_the_permission_state_and_has_no_timer_fallback() {
    let js = source!("../../../ui/camera.js");
    assert!(js.contains("invoke(\"camera_permission_ready\")"), "the page must query the flag after listening");
    assert!(!js.contains("}, 1500)"), "no timer may count the permission as given");
    let lib = source!("../lib.rs");
    assert!(lib.contains("camera::camera_permission_ready"));
    camera::camera_permission_ready();
}

// Structural fix, no red unit test: the cost is a 7 MB read per call, not a visible result.
#[test]
fn the_verified_pdfium_copy_is_remembered_after_the_first_check() {
    let src = source!("../images.rs");
    let body = &src[src.find("pub(crate) fn pdfium_lib_path").unwrap()..];
    let lookup = body.find("PDFIUM_VERIFIED.get()").expect("lookup");
    let read = body.find("open_verified_pdfium(&cached").expect("content check");
    assert!(lookup < read, "the remembered path is consulted before the content check");
    assert_eq!(body.matches("PDFIUM_VERIFIED.set(").count(), 2);
}

// Structural fix, no red unit test: install_permission_handler cannot be made to fail in a test.
#[test]
fn a_camera_window_without_its_permission_handler_is_destroyed() {
    let src = source!("../camera.rs");
    let at = src.find("if let Err(e) = install_permission_handler(&app, &win) {").expect("error arm");
    let arm = &src[at..at + 120];
    assert!(arm.contains("win.destroy()"), "{arm}");
    assert!(!src.contains("install_permission_handler(&app, &win)?;"));
}

#[test]
fn a_camera_chunk_decodes_and_garbage_is_an_error() {
    assert_eq!(crate::camera::decode_camera_chunk("AQID").unwrap(), vec![1u8, 2, 3]);
    assert_eq!(crate::camera::decode_camera_chunk("").unwrap(), Vec::<u8>::new(), "an empty chunk is a no-op");
    assert!(crate::camera::decode_camera_chunk("not base64!").is_err());
    assert!(crate::camera::decode_camera_chunk("AQ#D").is_err());
}

#[test]
fn a_failed_camera_sidecar_write_is_reported() {
    let root = std::env::temp_dir().join(format!("ps-camside-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let session = |dir: &std::path::Path| crate::camera::CamSession {
        output_path: dir.join("cam-1.webm"),
        companion_path: None,
        finishing: None,
    };
    // The folder does not exist, so the sidecar cannot be written.
    assert!(crate::camera::finalize_session(&session(&root.join("missing")), 5, None).is_err());
    std::fs::create_dir_all(&root).unwrap();
    assert!(crate::camera::finalize_session(&session(&root), 5, Some(7)).is_ok());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_failed_camera_finish_leaves_the_take_protected_from_the_sweep() {
    let root = std::env::temp_dir().join(format!("ps-camside-keep-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let video = root.join("cam-1.webm");
    std::fs::write(&video, b"take").unwrap();
    // A folder where the sidecar belongs makes the sidecar write fail with the folder writable.
    std::fs::create_dir_all(crate::media_files::sidecar_path(&video, crate::media_files::CAM_SIDECAR_EXT)).unwrap();
    let session = crate::camera::CamSession { output_path: video.clone(), companion_path: None, finishing: None };
    assert!(crate::camera::finalize_session(&session, 5, None).is_err());
    assert!(crate::camera::is_kept_abandoned(&video), "the unreferenced take must not age out");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_verified_pdfium_copy_is_held_open_so_it_cannot_be_swapped() {
    let dir = std::env::temp_dir().join(format!("ps-pdfhold-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pdfium-test.dll");
    fs::write(&path, b"expected bytes").unwrap();
    // Content that differs is refused, and the refused open keeps nothing locked.
    assert!(open_verified_pdfium(&path, b"other bytes").is_none());
    assert!(fs::write(&path, b"expected bytes").is_ok());
    let held = open_verified_pdfium(&path, b"expected bytes").expect("matches");
    assert!(fs::write(&path, b"swapped").is_err(), "no write while the handle is held");
    assert!(fs::remove_file(&path).is_err(), "no delete while the handle is held");
    assert!(fs::rename(&path, dir.join("moved.dll")).is_err(), "no rename while the handle is held");
    assert_eq!(fs::read(&path).unwrap(), b"expected bytes", "readers still get the file");
    drop(held);
    assert!(fs::remove_file(&path).is_ok());
    let _ = fs::remove_dir_all(&dir);
    // The lookup keeps the handle for the whole run, and never re-reads the path to verify.
    let src = source!("../images.rs");
    let body = &src[src.find("pub(crate) fn pdfium_lib_path").unwrap()..];
    assert!(body.contains("PDFIUM_HOLD.set("));
    assert!(!body.contains("fs::read(&cached)"));
}

#[test]
fn old_pdfium_copies_in_the_shared_temp_folder_are_removed_once_at_start() {
    let root = std::env::temp_dir().join(format!("ps-oldpdf-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let dir = root.join(brand::TEMP_DIR);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("pdfium-1.2.3.dll"), b"old").unwrap();
    fs::write(dir.join("pdfium-1.2.4.dll"), b"old").unwrap();
    fs::write(dir.join("pdfium-notes.txt"), b"keep").unwrap();
    fs::write(dir.join(format!("{}-ocr-1.png", brand::TEMP_PREFIX)), b"keep").unwrap();
    remove_old_temp_pdfium_in(&root);
    assert!(!dir.join("pdfium-1.2.3.dll").exists());
    assert!(!dir.join("pdfium-1.2.4.dll").exists());
    assert!(dir.join("pdfium-notes.txt").exists(), "only the dll copies go");
    assert!(dir.join(format!("{}-ocr-1.png", brand::TEMP_PREFIX)).exists(), "other scratch files are not this pass's business");
    // A missing folder is not an error.
    remove_old_temp_pdfium_in(&root.join("absent"));
    let _ = fs::remove_dir_all(&root);
    // The start-up setup runs the pass, and the snip sweep comment names where the copy lives now.
    let lib = source!("../lib.rs");
    assert!(lib.contains("remove_old_temp_pdfium()"));
    let snip = source!("../snip.rs");
    assert!(!snip.contains("next to the versioned pdfium cache"), "the comment no longer places the cache in TEMP");
}

#[test]
fn the_desktop_freeze_runs_on_a_blocking_worker() {
    let src = source!("../snip.rs");
    let body = &src[src.find("let frozen = ").unwrap()..];
    let line = &body[..body.find(";\n").unwrap()];
    assert!(line.contains("spawn_blocking(freeze_desktop)"), "{line}");
    assert!(!src.contains("match freeze_desktop()"));
}
