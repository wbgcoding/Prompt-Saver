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
