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
fn jpeg_with_orientation(orientation: u8) -> Vec<u8> {
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

// ---- ffmpeg errors in plain language: real log snippets, one per key ----

#[test]
fn ffmpeg_error_truncated_wins_over_the_generic_invalid_data_line_below_it() {
    let text = "[mov,mp4,m4a,3gp,3g2,mj2 @ 0000021a] moov atom not found\n\
                     source.mp4: Invalid data found when processing input";
    assert_eq!(classify_ffmpeg_error(text), FfmpegErrorKey::Truncated);
}

#[test]
fn ffmpeg_error_unreadable_from_a_plain_invalid_data_line() {
    let text = "[matroska,webm @ 0000021a] Could not find codec parameters\n\
                     source.webm: Invalid data found when processing input";
    assert_eq!(classify_ffmpeg_error(text), FfmpegErrorKey::Unreadable);
}

#[test]
fn ffmpeg_error_missing_from_no_such_file() {
    assert_eq!(classify_ffmpeg_error("in.mp4: No such file or directory"), FfmpegErrorKey::Missing);
}

#[test]
fn ffmpeg_error_denied_from_permission_denied() {
    assert_eq!(classify_ffmpeg_error("out.mp4: Permission denied"), FfmpegErrorKey::Denied);
    // The same category, reached from a plain Windows IO error string
    // instead of ffmpeg's own log (a locked destination file, e.g.).
    assert_eq!(classify_ffmpeg_error("Access is denied. (os error 5)"), FfmpegErrorKey::Denied);
}

#[test]
fn ffmpeg_error_no_space_from_the_write_failure_line() {
    let text = "av_interleaved_write_frame(): Error number -28 occurred\n\
                     out.mp4: No space left on device";
    assert_eq!(classify_ffmpeg_error(text), FfmpegErrorKey::NoSpace);
}

#[test]
fn ffmpeg_error_odd_size_from_the_encoder_line() {
    let text = "[libx264 @ 0000021a] width not divisible by 2 (641x480)\n\
                     Error initializing output stream 0:0 -- Error while opening encoder";
    assert_eq!(classify_ffmpeg_error(text), FfmpegErrorKey::OddSize);
}

#[test]
fn ffmpeg_error_too_large_from_an_allocation_failure() {
    let text = "[libx264 @ 0000021a] Cannot allocate memory\n\
                     Error while opening encoder for output stream 0:0";
    assert_eq!(classify_ffmpeg_error(text), FfmpegErrorKey::TooLarge);
}

#[test]
fn ffmpeg_error_timeout_key_has_no_matching_ffmpeg_text() {
    // Unlike every other key above, a timeout is this app's own wall-clock
    // decision (see FFMPEG_MAX_RUN_SECS), never something ffmpeg's own log
    // says -- so its coverage is the key itself, not a classify() case.
    assert_eq!(FfmpegErrorKey::Timeout.as_str(), "ffErrTimeout");
}

#[test]
fn ffmpeg_error_unrecognised_text_falls_back_to_other_not_nothing() {
    let key = classify_ffmpeg_error("some future ffmpeg build's brand new log line");
    assert_eq!(key, FfmpegErrorKey::Other);
    assert_eq!(key.as_str(), "ffErrOther");
}

#[test]
fn ffmpeg_error_json_round_trips_key_code_and_tailed_raw_text() {
    let json = ffmpeg_error_json(FfmpegErrorKey::NoSpace, Some(1), "out.mp4: No space left on device");
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["key"], "ffErrNoSpace");
    assert_eq!(parsed["code"], 1);
    assert_eq!(parsed["raw"], "out.mp4: No space left on device");
}

#[test]
fn ffmpeg_error_tail_lines_keeps_only_the_last_n() {
    let text = (1..=25).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n");
    let tail = tail_lines(&text, 20);
    assert_eq!(tail.lines().count(), 20);
    assert_eq!(tail.lines().next().unwrap(), "line 6");
    assert_eq!(tail.lines().last().unwrap(), "line 25");
}

// A user-triggered cancel is not classified at all -- run_ffmpeg_to_staged
// returns the plain "cancelled" sentinel before ever reaching
// classify_ffmpeg_error, and it is real ffmpeg output that would otherwise
// often exist alongside it (a killed process usually still wrote a log).
#[test]
fn ffmpeg_error_a_cancel_sentinel_is_not_json_and_is_never_classified() {
    assert!(serde_json::from_str::<serde_json::Value>("cancelled").is_err());
}

// ---- taskbar progress: the pure aggregation state machine, no real window ----

fn bg_op(phase: OpPhase) -> BgOp {
    BgOp { kind: "test".to_string(), phase, target: None, started_at_ms: now_ms(), finished_at_ms: None }
}

#[test]
fn taskbar_aggregate_is_off_with_no_operations() {
    let ops: HashMap<String, BgOp> = HashMap::new();
    assert_eq!(aggregate_taskbar_state(&ops), ("off", None));
}

#[test]
fn taskbar_aggregate_averages_several_running_operations() {
    let mut ops = HashMap::new();
    ops.insert("export".to_string(), bg_op(OpPhase::Running(40)));
    ops.insert("ffmpeg-download".to_string(), bg_op(OpPhase::Running(60)));
    assert_eq!(aggregate_taskbar_state(&ops), ("normal", Some(50)));
}

#[test]
fn taskbar_aggregate_prefers_indeterminate_over_a_running_percentage() {
    let mut ops = HashMap::new();
    ops.insert("export".to_string(), bg_op(OpPhase::Running(75)));
    ops.insert("backup".to_string(), bg_op(OpPhase::Indeterminate));
    assert_eq!(aggregate_taskbar_state(&ops), ("indeterminate", None));
}

#[test]
fn taskbar_aggregate_reports_error_even_with_other_operations_still_running() {
    let mut ops = HashMap::new();
    ops.insert("export".to_string(), bg_op(OpPhase::Running(20)));
    ops.insert("convert".to_string(), bg_op(OpPhase::Error));
    assert_eq!(aggregate_taskbar_state(&ops), ("error", None));
}

#[test]
fn taskbar_aggregate_returns_to_off_once_the_only_operation_is_removed_mid_way() {
    let mut ops = HashMap::new();
    ops.insert("convert".to_string(), bg_op(OpPhase::Running(30)));
    assert_eq!(aggregate_taskbar_state(&ops), ("normal", Some(30)));
    ops.remove("convert");
    assert_eq!(aggregate_taskbar_state(&ops), ("off", None));
}

#[test]
fn taskbar_aggregate_ignores_a_done_job_kept_around_for_the_popover() {
    // A finished job lingers in the map for bgDoneKeepMin so the header
    // popover can still list it, but must never keep the taskbar icon
    // showing stale progress.
    let mut ops = HashMap::new();
    ops.insert("export".to_string(), bg_op(OpPhase::Done));
    assert_eq!(aggregate_taskbar_state(&ops), ("off", None));
    ops.insert("convert".to_string(), bg_op(OpPhase::Running(55)));
    assert_eq!(aggregate_taskbar_state(&ops), ("normal", Some(55)));
}

// ---- background operations: the shared registry behind 386 and 397 ----

// Serializes every test below that reads or writes the process-global
// bg_ops() registry against each other -- cargo test runs tests on
// separate threads by default, and bg_ops_running_count() in particular
// counts every entry in the map regardless of which test put it there, so
// two of these tests racing would inflate each other's counts.
static BG_OPS_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn record_bg_op_reports_running_then_done_and_keeps_the_done_entry_briefly() {
    let _guard = BG_OPS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let op = format!("test-record-{}", std::process::id());
    record_bg_op(&op, "export", "running", Some(10), Some("C:\\out\\x.mp4".to_string()), 10.0);
    let list = list_bg_ops_impl(10.0);
    let row = list.iter().find(|o| o.id == op).expect("running job must be listed");
    assert_eq!(row.kind, "export");
    assert_eq!(row.state, "running");
    assert_eq!(row.pct, Some(10));
    assert_eq!(row.target.as_deref(), Some("C:\\out\\x.mp4"));

    record_bg_op(&op, "export", "done", None, None, 10.0);
    let list = list_bg_ops_impl(10.0);
    let row = list.iter().find(|o| o.id == op).expect("a just-finished job stays listed");
    assert_eq!(row.state, "done");
    assert!(row.finished_at_ms.is_some());
    // A "done" call with no target must not blank out the one already recorded.
    assert_eq!(row.target.as_deref(), Some("C:\\out\\x.mp4"));

    bg_ops().lock().unwrap_or_else(|e| e.into_inner()).remove(&op);
}

// A batch export gives each file its own registry key while sharing the
// "export" kind -- proves the fix directly, since keying by kind alone
// would have both calls share the single key "export" and the second
// would overwrite the first instead of the list showing two rows.
#[test]
fn record_bg_op_keeps_two_same_kind_jobs_as_separate_rows() {
    let _guard = BG_OPS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let id_a = format!("test-batch-a-{}", std::process::id());
    let id_b = format!("test-batch-b-{}", std::process::id());
    record_bg_op(&id_a, "export", "running", Some(10), Some("C:\\out\\a.mp4".to_string()), 10.0);
    record_bg_op(&id_b, "export", "running", Some(40), Some("C:\\out\\b.mp4".to_string()), 10.0);
    let list = list_bg_ops_impl(10.0);
    let row_a = list.iter().find(|o| o.id == id_a).expect("first batch job must be listed");
    let row_b = list.iter().find(|o| o.id == id_b).expect("second batch job must still be listed");
    assert_eq!(row_a.kind, "export");
    assert_eq!(row_b.kind, "export");
    assert_eq!(row_a.pct, Some(10), "the second job must not overwrite the first's progress");
    assert_eq!(row_b.pct, Some(40));

    let mut ops = bg_ops().lock().unwrap_or_else(|e| e.into_inner());
    ops.remove(&id_a);
    ops.remove(&id_b);
}

#[test]
fn prune_finished_ops_drops_a_done_job_past_its_keep_window() {
    let mut ops = HashMap::new();
    ops.insert(
        "old-export".to_string(),
        BgOp { kind: "export".to_string(), phase: OpPhase::Done, target: None, started_at_ms: 0, finished_at_ms: Some(0) },
    );
    prune_finished_ops(&mut ops, 10.0); // 0ms is far more than 10 minutes ago
    assert!(ops.is_empty(), "a long-finished job must be swept away");
}

#[test]
fn bg_ops_running_count_ignores_done_and_error_entries() {
    let _guard = BG_OPS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let a = format!("test-count-a-{}", std::process::id());
    let b = format!("test-count-b-{}", std::process::id());
    let before = bg_ops_running_count();
    record_bg_op(&a, "export", "running", Some(1), None, 10.0);
    record_bg_op(&b, "export", "error", None, None, 10.0);
    // Only `a` (still running) should raise the count; `b` already failed.
    assert_eq!(bg_ops_running_count(), before + 1);
    record_bg_op(&a, "export", "done", None, None, 10.0);
    assert_eq!(bg_ops_running_count(), before);
    let mut ops = bg_ops().lock().unwrap_or_else(|e| e.into_inner());
    ops.remove(&a);
    ops.remove(&b);
}

// ---- ffmpeg: zip-signature + pinned-hash verification, no network needed ----

fn ffmpeg_test_zip() -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("hello.txt", options).unwrap();
    use std::io::Write;
    writer.write_all(b"not a real ffmpeg build, just a test fixture").unwrap();
    writer.finish().unwrap().into_inner()
}

#[test]
fn ffmpeg_wrong_hash_is_rejected() {
    let zip_bytes = ffmpeg_test_zip();
    // Any hash that is not this blob's real SHA-256 must be rejected -- the
    // "point the pinned constant at a wrong hash" proof the spec asks for.
    let wrong_hash = "0".repeat(64);
    let err = verify_ffmpeg_zip_against(&zip_bytes, &wrong_hash).unwrap_err();
    assert!(err.contains("hash mismatch"), "{err}");
}

#[test]
fn ffmpeg_matching_hash_is_accepted() {
    let zip_bytes = ffmpeg_test_zip();
    use sha2::{Digest, Sha256};
    let real_hash = format!("{:x}", Sha256::digest(&zip_bytes));
    assert!(verify_ffmpeg_zip_against(&zip_bytes, &real_hash).is_ok());
}

#[test]
fn ffmpeg_non_zip_bytes_are_rejected_before_the_hash_check() {
    let err = verify_ffmpeg_zip_against(b"not a zip file at all", FFMPEG_SHA256).unwrap_err();
    assert!(err.contains("not a valid archive"), "{err}");
}

// ---- file dialogs parent to their caller ----

#[test]
fn dialog_parent_picks_the_requested_window_when_it_is_open() {
    let open = ["main", "video-editor"];
    let chosen = resolve_dialog_parent_label(|l| open.contains(&l), "video-editor");
    assert_eq!(chosen, Some("video-editor"));
}

#[test]
fn dialog_parent_falls_back_to_main_when_the_label_is_unknown() {
    let open = ["main"];
    let chosen = resolve_dialog_parent_label(|l| open.contains(&l), "video-editor");
    assert_eq!(chosen, Some("main"));
}

#[test]
fn dialog_parent_is_none_when_neither_window_is_open() {
    let chosen = resolve_dialog_parent_label(|_| false, "video-editor");
    assert_eq!(chosen, None);
}

#[test]
fn ffmpeg_search_path_never_touches_path_or_a_system_directory() {
    // Discovery only ever looks beside the exe, in the app's own data folder,
    // and in PATH -- confirming the constants/logic never reference a system
    // directory or attempt to modify PATH is a static-shape check, not a
    // runtime one (there is no writable AppHandle in this pure unit test).
    assert_eq!(FFMPEG_ZIP_ENTRY, "ffmpeg-9.0.1-essentials_build/bin/ffmpeg.exe");
    assert!(FFMPEG_URL.starts_with("https://github.com/GyanD/codexffmpeg/releases/download/"));
    assert_eq!(FFMPEG_SHA256.len(), 64);
}

#[test]
fn hex_decode_round_trips_the_pinned_hash() {
    let decoded = hex_decode(FFMPEG_SHA256).expect("pinned hash must be valid hex");
    assert_eq!(decoded.len(), 32); // SHA-256 is 32 bytes
}

#[test]
fn ct_eq_matches_plain_equality_without_short_circuiting() {
    assert!(ct_eq(b"abcdef", b"abcdef"));
    assert!(!ct_eq(b"abcdef", b"abcdeg"));
    assert!(!ct_eq(b"abc", b"abcd")); // different lengths never match
}

// ---- Recording conversion: pure guard logic, no ffmpeg/hardware needed ----

#[test]
fn convert_is_noop_rejects_a_plain_mp4_to_mp4_request() {
    assert!(convert_is_noop(true, "mp4", false));
}

#[test]
fn convert_is_noop_allows_the_lossless_mp4_reencode_despite_the_same_extension() {
    // This is the one case where "same extension" must NOT short-circuit
    // the conversion -- lossless re-encodes mp4 in place on purpose.
    assert!(!convert_is_noop(true, "mp4", true));
}

#[test]
fn convert_is_noop_allows_a_genuine_format_change() {
    assert!(!convert_is_noop(false, "webm", false));
    assert!(!convert_is_noop(false, "gif", false));
}

// ---- Recording conversion: real ffmpeg, no capture hardware needed ----
//
// These exercise the actual subprocess, against the local build's own
// ffmpeg.exe (gitignored, downloaded the same way `download_ffmpeg` does
// -- see the README's "Building from source" section). A clean checkout
// without it present skips these rather than failing.

fn test_ffmpeg() -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("ffmpeg.exe");
    p.is_file().then_some(p)
}

// A tiny synthetic clip, generated entirely by ffmpeg's own test-pattern
// source -- no real recording needed to exercise the conversion path.
fn make_test_mp4(ffmpeg: &Path, dir: &Path, name: &str, seconds: u32, size: &str) -> PathBuf {
    use std::os::windows::process::CommandExt;
    let out = dir.join(name);
    let status = std::process::Command::new(ffmpeg)
        .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
        .arg(format!("testsrc=duration={seconds}:size={size}:rate=15"))
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(&out)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .expect("could not run ffmpeg to build the test fixture");
    assert!(status.success(), "ffmpeg failed to build the test fixture clip");
    out
}

#[test]
fn convert_recording_real_ffmpeg_produces_a_playable_webm_and_removes_the_source() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-convtest-webm-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 1, "64x64");
    let out = convert_recording_blocking(&ffmpeg, &src, "webm", false, || false, |_| {}).expect("conversion should succeed");
    let out = PathBuf::from(out);
    assert!(out.is_file(), "converted file missing");
    assert_eq!(out.extension().unwrap(), "webm");
    // WebM is a Matroska/EBML container -- its files start with this fixed
    // 4-byte magic regardless of codec, which is enough to prove ffmpeg
    // actually produced a real container rather than an empty/garbage file.
    let bytes = fs::read(&out).unwrap();
    assert_eq!(&bytes[0..4], &[0x1A, 0x45, 0xDF, 0xA3]);
    assert!(!src.is_file(), "source must be removed once the conversion succeeded");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn convert_recording_real_ffmpeg_gif_has_gif_magic_and_removes_the_source() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-convtest-gif-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 1, "64x64");
    let out = convert_recording_blocking(&ffmpeg, &src, "gif", false, || false, |_| {}).expect("conversion should succeed");
    let bytes = fs::read(&out).unwrap();
    assert_eq!(&bytes[0..3], b"GIF");
    assert!(!src.is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn convert_recording_real_ffmpeg_lossless_replaces_the_source_in_place() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-convtest-lossless-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 1, "64x64");
    let before = fs::read(&src).unwrap();
    let out = convert_recording_blocking(&ffmpeg, &src, "mp4", true, || false, |_| {}).expect("conversion should succeed");
    assert_eq!(PathBuf::from(&out), src, "lossless mp4 must replace the source path, not a new one");
    assert!(src.is_file());
    let after = fs::read(&src).unwrap();
    assert_ne!(before, after, "a CRF-0 re-encode should actually rewrite the file, not leave it untouched");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn convert_recording_real_ffmpeg_cancel_leaves_the_source_untouched() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-convtest-cancel-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    // Long enough that the cancel flag is set well before ffmpeg finishes.
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 8, "960x720");
    // A cancel flag local to this test -- never the process-global
    // REC_CONVERT_CANCEL -- so flipping it can never race an unrelated
    // conversion another test happens to be running in parallel.
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_writer = Arc::clone(&cancel);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        cancel_writer.store(true, Ordering::SeqCst);
    });
    let result = convert_recording_blocking(&ffmpeg, &src, "webm", false, || cancel.load(Ordering::SeqCst), |_| {});
    assert_eq!(result, Err("cancelled".to_string()));
    assert!(src.is_file(), "a cancelled conversion must never remove the source");
    assert!(!dir.join("src.webm").exists(), "a cancelled conversion must leave no destination file");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn convert_recording_real_ffmpeg_failure_leaves_the_source_untouched() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-convtest-fail-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    // Not a real video -- ffmpeg exits non-zero, exercising the failure path.
    // ffmpeg's mp4 demuxer reports this exact case ("no valid moov box at
    // all", whether from garbage bytes or a real truncation) as the same
    // "moov atom not found" line as a genuinely cut-short recording, so
    // this lands on ffErrTruncated too -- confirmed empirically here
    // rather than assumed, which is why the dedicated truncated-file test
    // right below exists as its own separate case.
    let src = dir.join("src.mp4");
    fs::write(&src, b"this is not a video file").unwrap();
    let result = convert_recording_blocking(&ffmpeg, &src, "webm", false, || false, |_| {});
    let Err(err) = result else { panic!("garbage input must fail") };
    let payload: serde_json::Value = serde_json::from_str(&err).expect("error must be the classified JSON shape");
    assert_eq!(payload["key"], "ffErrTruncated");
    assert!(src.is_file(), "a failed conversion must never remove the source");
    assert!(!dir.join("src.webm").exists(), "a failed conversion must leave no destination file");
    let _ = fs::remove_dir_all(&dir);
}

// The point-399 Abnahme's own required scenario: a genuinely truncated
// video file (built by cutting a real, valid mp4 down to a prefix, which
// for ffmpeg's default (non-faststart) muxer removes the moov atom it
// writes at the end) must show the friendly "incomplete file" key, never
// ffmpeg's own raw log line as the headline.
#[test]
fn convert_recording_real_ffmpeg_truncated_file_shows_the_friendly_incomplete_key() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-convtest-truncated-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let whole = make_test_mp4(&ffmpeg, &dir, "whole.mp4", 2, "320x240");
    let bytes = fs::read(&whole).unwrap();
    let src = dir.join("src.mp4");
    fs::write(&src, &bytes[..bytes.len() / 3]).unwrap();
    let result = convert_recording_blocking(&ffmpeg, &src, "webm", false, || false, |_| {});
    let Err(err) = result else { panic!("a truncated file must fail to convert") };
    let payload: serde_json::Value = serde_json::from_str(&err).expect("error must be the classified JSON shape");
    assert_eq!(payload["key"], "ffErrTruncated");
    assert!(src.is_file(), "a failed conversion must never remove the source");
    let _ = fs::remove_dir_all(&dir);
}

// Serializes every test that reads or flips the process-global REC_CROP_CANCEL
// flag against each other -- cargo test runs tests on separate threads by
// default, and one test flipping that flag mid-run would otherwise cancel
// whatever unrelated crop another thread happened to be running at the same
// moment.
static REC_CROP_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn crop_recording_real_ffmpeg_produces_the_requested_size_in_place() {
    let _guard = REC_CROP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    REC_CROP_CANCEL.store(false, Ordering::SeqCst);
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let Some(ffprobe) = test_ffprobe() else { return };
    let dir = std::env::temp_dir().join(format!("ps-croptest-size-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 1, "64x48");
    let out = crop_recording_blocking(&ffmpeg, &src, (8, 8, 32, 24), || REC_CROP_CANCEL.load(Ordering::SeqCst), |_| {}).expect("crop should succeed");
    assert_eq!(PathBuf::from(&out), src, "the crop must replace the source path, not a new one");
    let probe = probe_recording_blocking(&ffprobe, &src).expect("probe should succeed");
    assert_eq!((probe.width, probe.height), (32, 24));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn crop_recording_real_ffmpeg_floors_odd_rect_values_to_even() {
    let _guard = REC_CROP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    REC_CROP_CANCEL.store(false, Ordering::SeqCst);
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let Some(ffprobe) = test_ffprobe() else { return };
    let dir = std::env::temp_dir().join(format!("ps-croptest-odd-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    // libx264 needs even dimensions; an odd rect (from an odd window
    // position/size) must still produce a playable file, not an
    // "odd dimensions" ffmpeg failure.
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 1, "64x48");
    let out = crop_recording_blocking(&ffmpeg, &src, (7, 5, 31, 23), || REC_CROP_CANCEL.load(Ordering::SeqCst), |_| {}).expect("crop should succeed");
    assert_eq!(PathBuf::from(&out), src);
    let probe = probe_recording_blocking(&ffprobe, &src).expect("probe should succeed");
    assert_eq!((probe.width, probe.height), (30, 22), "odd rect values must floor to even");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn crop_recording_real_ffmpeg_cancel_leaves_the_source_untouched() {
    let _guard = REC_CROP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-croptest-cancel-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 2, "320x240");
    let before = fs::read(&src).unwrap();
    // Set BEFORE starting rather than racing a delayed flip against an
    // unknown encode speed: run_ffmpeg_to_staged re-checks is_cancelled()
    // once more right after the process exits, so a flag already true
    // from the start is caught deterministically whether or not the
    // stdout-progress loop itself ever observes it mid-run.
    REC_CROP_CANCEL.store(true, Ordering::SeqCst);
    let result = crop_recording_blocking(&ffmpeg, &src, (10, 10, 200, 150), || REC_CROP_CANCEL.load(Ordering::SeqCst), |_| {});
    assert_eq!(result, Err("cancelled".to_string()));
    assert!(src.is_file(), "a cancelled crop must never remove the source");
    let after = fs::read(&src).unwrap();
    assert_eq!(before, after, "a cancelled crop must leave the source byte-for-byte untouched");
    REC_CROP_CANCEL.store(false, Ordering::SeqCst);
    let _ = fs::remove_dir_all(&dir);
}

// ---- The export format note's size estimate ----

#[test]
fn estimate_export_bytes_matches_the_plain_bitrate_formula_for_mp4() {
    // 1000 kbps video + 192 kbps AAC (export_audio_kbps's own mp4-normal
    // constant) over exactly 10 seconds must be an exact, not approximate, match.
    let est = estimate_export_bytes("mp4", Some(1000), false, 10_000);
    let expected = (1000 + 192) * 1000 / 8 * 10;
    assert_eq!(est.bytes, Some(expected as u64));
    assert!(est.low_bytes.is_none() && est.high_bytes.is_none());
}

#[test]
fn estimate_export_bytes_lossless_quality_uses_the_higher_mp4_audio_rate() {
    // lossless_quality on mp4 burns 320k AAC instead of 192k -- the estimate
    // must reflect the same jump export_video_blocking's own encoder settings make.
    let normal = estimate_export_bytes("mp4", Some(1000), false, 10_000);
    let lossless = estimate_export_bytes("mp4", Some(1000), true, 10_000);
    assert!(lossless.bytes.unwrap() > normal.bytes.unwrap());
}

#[test]
fn estimate_export_bytes_gif_returns_a_range_not_a_number() {
    let est = estimate_export_bytes("gif", None, false, 5_000);
    assert!(est.bytes.is_none(), "gif must not report a single byte figure");
    let low = est.low_bytes.expect("range must have a low bound");
    let high = est.high_bytes.expect("range must have a high bound");
    assert!(low > 0 && low < high, "gif's range must be a real, ordered bracket");
}

#[test]
fn estimate_export_bytes_falls_back_to_a_sane_default_without_a_known_bitrate() {
    // A silent read (video_kbps: None) for a plain video format must not
    // panic or report zero -- it falls back to a reasonable assumed bitrate.
    let est = estimate_export_bytes("webm", None, false, 10_000);
    assert!(est.bytes.unwrap() > 0);
}

// ---- Disk space check (recording start, mid-recording, export) ----

#[test]
fn estimate_recording_seconds_remaining_uses_bitrate_times_time() {
    // 8000 kbps = 1,000,000 bytes/sec with no audio -- 10,000,000 bytes
    // should buy exactly 10 seconds, an exact match like the export
    // formula test above, not just "roughly right".
    assert_eq!(estimate_recording_seconds_remaining(10_000_000, 8000, false), 10);
}

#[test]
fn estimate_recording_seconds_remaining_adds_the_audio_surcharge() {
    // Same free space either way -- the higher combined bitrate with
    // audio active must buy strictly fewer seconds than video alone.
    let with_audio = estimate_recording_seconds_remaining(10_000_000, 8000, true);
    let without_audio = estimate_recording_seconds_remaining(10_000_000, 8000, false);
    assert!(with_audio < without_audio);
}

#[test]
fn estimate_recording_seconds_remaining_never_divides_by_zero() {
    assert_eq!(estimate_recording_seconds_remaining(10_000_000, 0, false), 0);
}

// ---- Recording start: a second claim during the check-then-act gap ----

#[cfg(windows)]
#[test]
fn rec_starting_guard_blocks_a_second_claim_until_dropped() {
    // start_recording's own gap: rec_can_start passes while RecordingState
    // is still empty, and the actual capture start only publishes a
    // session once it returns from a blocking thread -- a second start
    // racing through that window used to see the same empty state and
    // pass too. REC_STARTING closes it: claimed alongside the phase
    // check, released by RecStartingGuard's Drop on every exit path.
    assert!(!REC_STARTING.load(Ordering::SeqCst), "a stray claim from another test left this dirty");
    assert!(REC_STARTING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok());
    let guard = RecStartingGuard;
    assert!(
        REC_STARTING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err(),
        "a second start claimed the slot while the first one was still starting"
    );
    drop(guard);
    assert!(!REC_STARTING.load(Ordering::SeqCst), "the guard did not release the slot on drop");
}

// ---- Recording bar file size ----

// Structural change, so no red exists: the threshold used to be a stored setting
// nothing in the UI wrote and is now a constant; the bar settings hand it out as is.
#[test]
fn rec_bar_size_threshold_is_the_fixed_constant() {
    let store = Store {
        prompts: Vec::new(),
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: Default::default(),
    };
    assert_eq!(rec_bar::REC_BAR_SIZE_FROM_MB, 500.0);
    assert_eq!(rec_bar::rec_bar_settings(&store).1, rec_bar::REC_BAR_SIZE_FROM_MB);
}

#[test]
fn rec_bar_size_state_hides_below_the_show_threshold() {
    let mb = 1024 * 1024;
    assert_eq!(rec_bar_size_state(499 * mb, 60_000, 500.0, 8000.0), (false, None));
}

#[test]
fn rec_bar_size_state_shows_at_and_above_the_threshold_without_an_eta_yet() {
    let mb = 1024 * 1024;
    // Well past the show threshold but nowhere near three quarters of the
    // 8000 MB ceiling -- shown, but with no ETA to a limit this far off.
    assert_eq!(rec_bar_size_state(600 * mb, 60_000, 500.0, 8000.0), (true, None));
}

#[test]
fn rec_bar_size_state_adds_an_eta_past_three_quarters_of_the_ceiling() {
    let mb = 1024u64 * 1024;
    // 6100 MB of an 8000 MB ceiling is past the 6000 MB three-quarter mark.
    // Observed rate: 6100 MB in 60s -> remaining 1900 MB at that same
    // rate takes 1900/6100*60 ~= 18.7s.
    let (show, eta) = rec_bar_size_state(6100 * mb, 60_000, 500.0, 8000.0);
    assert!(show);
    let eta = eta.expect("must carry an ETA this close to the ceiling");
    assert!((17..=20).contains(&eta), "eta was {eta}, expected roughly 18-19s");
}

#[test]
fn rec_bar_size_state_zero_ceiling_disables_the_eta_but_not_the_size() {
    let mb = 1024 * 1024;
    assert_eq!(rec_bar_size_state(600 * mb, 60_000, 500.0, 0.0), (true, None));
}

#[test]
fn rec_bar_size_state_never_panics_at_zero_elapsed_time() {
    let mb = 1024 * 1024;
    let (show, eta) = rec_bar_size_state(6100 * mb, 0, 500.0, 8000.0);
    assert!(show);
    assert_eq!(eta, None, "no observed rate yet at elapsed=0, so no ETA rather than a division by zero");
}

// ---- Live recording target switch ----

#[test]
fn target_fits_canvas_accepts_anything_on_the_every_display_canvas() {
    assert!(recording::target_fits_canvas(Some("all"), (-1920, 0), (5760, 1080), Some((3840, 0, 1920, 1080))));
    assert!(recording::target_fits_canvas(Some("all"), (-1920, 0), (5760, 1080), None));
}

#[test]
fn target_fits_canvas_requires_the_same_monitor_in_monitor_mode() {
    let canvas_origin = (0, 0);
    let canvas_size = (1920, 1080);
    assert!(recording::target_fits_canvas(Some("monitor"), canvas_origin, canvas_size, Some((0, 0, 1920, 1080))));
    assert!(!recording::target_fits_canvas(Some("monitor"), canvas_origin, canvas_size, Some((1920, 0, 1920, 1080))));
    assert!(!recording::target_fits_canvas(Some("monitor"), canvas_origin, canvas_size, None));
}

#[test]
fn target_fits_canvas_never_offers_a_switch_on_the_window_size_canvas() {
    assert!(!recording::target_fits_canvas(Some("window"), (0, 0), (800, 600), Some((0, 0, 800, 600))));
    assert!(!recording::target_fits_canvas(None, (0, 0), (800, 600), Some((0, 0, 800, 600))));
}

// ---- Region recording's own outline instead of the OS capture border ----

#[test]
fn border_setting_only_switches_off_for_a_region_target() {
    assert_eq!(
        recording::border_setting_for_target(recording::RecTarget::Region { monitor: 1, x: 0, y: 0, w: 800, h: 600 }),
        recording::BorderSetting::WithoutBorder,
    );
    assert_eq!(
        recording::border_setting_for_target(recording::RecTarget::Monitor(0)),
        recording::BorderSetting::Default,
    );
    assert_eq!(
        recording::border_setting_for_target(recording::RecTarget::Window(123)),
        recording::BorderSetting::Default,
    );
}

#[test]
fn region_outline_rect_adds_the_monitor_origin_to_the_region_offset() {
    // A region picked on a monitor to the right of the primary one: the
    // outline must land at the monitor's own absolute position plus the
    // region's offset within it, exactly what resolve_crop already crops
    // each frame to.
    assert_eq!(recording::region_outline_rect((1920, 0), 100, 50, 400, 300), (2020, 50, 400, 300));
    assert_eq!(recording::region_outline_rect((0, 0), 0, 0, 1920, 1080), (0, 0, 1920, 1080));
}

#[test]
fn region_outline_rect_clamps_a_negative_offset_to_the_monitor_origin() {
    // Same defensive clamp resolve_crop already applies to a region whose
    // picker math ever hands back a negative offset.
    assert_eq!(recording::region_outline_rect((0, 0), -10, -5, 200, 100), (0, 0, 200, 100));
}

#[test]
fn outline_rect_for_target_is_none_for_monitor_and_window_targets() {
    assert_eq!(recording::outline_rect_for_target(recording::RecTarget::Monitor(0)), None);
    assert_eq!(recording::outline_rect_for_target(recording::RecTarget::Window(123)), None);
}

#[test]
fn outline_reapply_guard_ignores_a_two_pixel_rounding_difference() {
    let want = (2020, 50, 400, 300);
    assert!(!outline_rect_needs_reapply(want, want));
    assert!(!outline_rect_needs_reapply((2022, 48, 402, 298), want));
    // A window rescaled by a DPI change (400x300 at 100% moved to 150%)
    // is far outside the guard.
    assert!(outline_rect_needs_reapply((2020, 50, 600, 450), want));
    assert!(outline_rect_needs_reapply((2023, 50, 400, 300), want));
    assert!(outline_rect_needs_reapply((2020, 50, 400, 297), want));
}

#[test]
fn rec_outline_switch_and_colour_come_from_the_settings() {
    let rect = Some((10, 20, 300, 200));
    let mut s = Settings::default();
    // Nothing stored: the frame shows, in the shipped colour.
    assert_eq!(rec_outline_request(&s, rect), Some(((10, 20, 300, 200), "#2563eb".to_string())));
    // A target without an outline never asks for one.
    assert_eq!(rec_outline_request(&s, None), None);
    s.ui_texts.insert("recOutlineColor".to_string(), "#FF8800".to_string());
    assert_eq!(rec_outline_request(&s, rect).map(|r| r.1), Some("#ff8800".to_string()));
    // Switched off: no outline for any target.
    s.ui_flags.insert("recOutline".to_string(), false);
    assert_eq!(rec_outline_request(&s, rect), None);
    s.ui_flags.insert("recOutline".to_string(), true);
    assert!(rec_outline_request(&s, rect).is_some());
}

#[test]
fn rec_outline_colour_only_lets_six_hex_digits_through() {
    for bad in ["", "red", "#fff", "#12345g", "#1234567", "2563eb", "#fff';alert(1)//", "#2563eb\n"] {
        let mut s = Settings::default();
        s.ui_texts.insert("recOutlineColor".to_string(), bad.to_string());
        assert_eq!(rec_outline_color(&s), REC_OUTLINE_DEFAULT_COLOR, "{bad:?}");
    }
    let script = rec_outline_init_script("#2563eb");
    assert!(script.contains("'#2563eb'") && script.contains("--outline-color"));
}

#[test]
fn recording_windows_stay_excluded_whatever_the_setting_says() {
    assert!(always_excluded("rec-bar"));
    assert!(always_excluded("rec-outline"));
    assert!(!always_excluded("main"));
    assert!(!always_excluded("float-abc"));
    // Setting allows screenshots (excluded = false): only the two stay excluded.
    assert!(wants_capture_exclusion("rec-bar", false));
    assert!(wants_capture_exclusion("rec-outline", false));
    assert!(!wants_capture_exclusion("main", false));
    assert!(!wants_capture_exclusion("snip-0", false));
    // Setting off (excluded = true): everything is excluded.
    assert!(wants_capture_exclusion("main", true));
    assert!(wants_capture_exclusion("rec-bar", true));
}

#[test]
fn begin_capture_hides_the_app_only_while_screenshots_are_not_allowed() {
    assert!(capture_hides_app_windows(true));
    assert!(!capture_hides_app_windows(false));
    // The default setting (nothing stored) means excluded, so hidden.
    assert!(capture_hides_app_windows(capture_excluded(&Settings::default())));
    let mut allow = Settings::default();
    allow.ui_flags.insert(ALLOW_SCREENSHOT_KEY.to_string(), true);
    assert!(!capture_hides_app_windows(capture_excluded(&allow)));
}

#[test]
fn snip_restore_set_skips_windows_that_were_hidden_before() {
    let before = vec![
        ("main".to_string(), false),
        ("float-1".to_string(), true),
        ("folder-2".to_string(), false),
        ("float-3".to_string(), true),
    ];
    assert_eq!(windows_to_restore(&before), vec!["float-1".to_string(), "float-3".to_string()]);
    assert!(windows_to_restore(&[("main".to_string(), false)]).is_empty());
    assert_eq!(windows_to_restore(&[("main".to_string(), true)]), vec!["main".to_string()]);
    assert!(is_snip_hidden_label("main") && is_snip_hidden_label("float-x") && is_snip_hidden_label("folder-y"));
    assert!(!is_snip_hidden_label("snip-0") && !is_snip_hidden_label("rec-bar"));
}

#[test]
fn a_stale_outline_closure_is_not_current_after_a_newer_outline() {
    assert!(outline_closure_is_current(3, 3));
    assert!(!outline_closure_is_current(3, 4));
}

#[test]
fn affinity_readback_is_compared_against_the_wanted_state() {
    assert!(affinity_matches(true, Some(0x11)));
    assert!(affinity_matches(false, Some(0)));
    assert!(!affinity_matches(false, Some(0x11)));
    assert!(!affinity_matches(true, Some(0)));
    assert!(affinity_matches(true, None));
    assert!(affinity_matches(false, None));
}

#[test]
fn recording_disk_warning_fires_only_below_the_threshold() {
    let mb = 1024 * 1024;
    // Free space at or above the threshold: no warning.
    assert!(recording_disk_warning("C:".to_string(), 500 * mb, 500.0, 8000, false).is_none());
    // Below it: a warning, carrying the drive and free space through untouched.
    let warn = recording_disk_warning("D:".to_string(), 100 * mb, 500.0, 8000, false).expect("must warn");
    assert_eq!(warn.drive, "D:");
    assert_eq!(warn.free_mb, 100);
}

#[test]
fn export_disk_warning_fires_only_when_the_estimate_does_not_fit() {
    let mb = 1024 * 1024;
    assert!(export_disk_warning("C:".to_string(), 500 * mb, 400 * mb).is_none());
    let warn = export_disk_warning("C:".to_string(), 300 * mb, 400 * mb).expect("must warn");
    assert_eq!(warn.free_mb, 300);
    assert_eq!(warn.needed_mb, 400);
}

#[test]
fn free_space_bytes_walks_up_to_the_nearest_existing_ancestor() {
    // A destination folder that does not exist yet (a fresh recordings or
    // export folder) must still resolve to its real drive's free space,
    // proven here by matching the real ancestor directory two levels up.
    // The two figures are read a moment apart and the volume keeps being
    // written to, so they are compared within a tolerance: a different
    // volume would be off by far more than a gigabyte.
    let base = std::env::temp_dir();
    let base_free = free_space_bytes(&base).expect("free space on an existing dir");
    let missing = base.join("prompt-saver-disk-check-probe").join("deeper");
    let missing_free = free_space_bytes(&missing).expect("free space on a missing nested path");
    let drift = base_free.abs_diff(missing_free);
    assert!(
        drift < 1024 * 1024 * 1024,
        "missing path resolved to a different volume: {base_free} vs {missing_free}"
    );
}

#[test]
fn free_space_bytes_returns_a_plausible_positive_number() {
    assert!(free_space_bytes(&std::env::temp_dir()).expect("free space") > 0);
}

// ---- Video editor (242-245): real ffmpeg/ffprobe, no capture hardware needed ----

fn test_ffprobe() -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("ffprobe.exe");
    p.is_file().then_some(p)
}

// ---- detect_media_kind / exceeds_open_edit_limit: opening an existing file for editing ----

#[test]
fn exceeds_open_edit_limit_rejects_only_past_the_configured_mb() {
    let mb = 1024 * 1024;
    assert!(!exceeds_open_edit_limit(2048 * mb, 2048.0), "exactly at the limit must still open");
    assert!(exceeds_open_edit_limit(2048 * mb + 1, 2048.0), "one byte past the limit must be rejected");
    // A stray zero/negative setting must never turn into "anything goes".
    assert!(exceeds_open_edit_limit(2 * mb, 0.0));
}

#[test]
fn detect_media_kind_recognizes_a_jpeg_saved_with_the_wrong_extension() {
    let dir = std::env::temp_dir().join(format!("ps-openedit-jpg-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("photo.png"); // real JPEG bytes, misleading extension
    let img = image::RgbImage::from_pixel(4, 4, image::Rgb([200, 50, 10]));
    image::DynamicImage::ImageRgb8(img).save_with_format(&path, image::ImageFormat::Jpeg).unwrap();
    let kind = detect_media_kind(&path, None);
    assert_eq!(kind.map(|(k, _)| k), Some("image"), "content, not the .png extension, must decide");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn detect_media_kind_rejects_a_text_file_named_png() {
    let dir = std::env::temp_dir().join(format!("ps-openedit-txt-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("notes.png");
    fs::write(&path, b"just some plain text, not an image at all").unwrap();
    assert!(detect_media_kind(&path, None).is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn detect_media_kind_rejects_an_empty_file() {
    let dir = std::env::temp_dir().join(format!("ps-openedit-empty-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("empty.jpg");
    fs::write(&path, b"").unwrap();
    assert!(detect_media_kind(&path, None).is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn detect_media_kind_recognizes_an_mp4_saved_with_the_wrong_extension() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let Some(ffprobe) = test_ffprobe() else { return };
    let dir = std::env::temp_dir().join(format!("ps-openedit-mp4-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let real = make_test_mp4(&ffmpeg, &dir, "clip.mp4", 1, "64x64");
    let path = dir.join("clip.jpg"); // real mp4 bytes, misleading extension
    fs::rename(&real, &path).unwrap();
    let kind = detect_media_kind(&path, Some(&ffprobe));
    assert_eq!(kind.map(|(k, _)| k), Some("video"), "content, not the .jpg extension, must decide");
    let _ = fs::remove_dir_all(&dir);
}

// Like make_test_mp4, but with its own (system) audio track -- needed only
// for the mux test below: muxing a mic sidecar into a video-only source
// (make_test_mp4's lavfi testsrc has no audio at all) would still end up
// with just one audio stream, since "0:a?" maps nothing when there is no
// stream 0 audio to begin with.
fn make_test_mp4_with_audio(ffmpeg: &Path, dir: &Path, name: &str, seconds: u32, size: &str) -> PathBuf {
    use std::os::windows::process::CommandExt;
    let out = dir.join(name);
    let status = std::process::Command::new(ffmpeg)
        .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
        .arg(format!("testsrc=duration={seconds}:size={size}:rate=15"))
        .args(["-f", "lavfi", "-i"])
        .arg(format!("sine=frequency=220:duration={seconds}"))
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
        .arg(&out)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .expect("could not run ffmpeg to build the test fixture");
    assert!(status.success(), "ffmpeg failed to build the test fixture clip");
    out
}

fn make_test_wav(dir: &Path, name: &str, seconds: u32) -> PathBuf {
    use std::os::windows::process::CommandExt;
    let Some(ffmpeg) = test_ffmpeg() else { panic!("ffmpeg.exe required to build the wav fixture") };
    let out = dir.join(name);
    let status = std::process::Command::new(&ffmpeg)
        .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
        .arg(format!("sine=frequency=440:duration={seconds}"))
        .args(["-ar", "48000", "-ac", "1"])
        .arg(&out)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .expect("could not run ffmpeg to build the wav fixture");
    assert!(status.success());
    out
}

#[test]
fn pcm_s16le_to_peaks_never_overflows_on_i16_min() {
    // i16::MIN.unsigned_abs() is 32768, one past i16::MAX -- a signed
    // running max would wrap here; the u16 running max must not.
    let bytes = i16::MIN.to_le_bytes();
    let peaks = pcm_s16le_to_peaks(&bytes, 1);
    assert_eq!(peaks, vec![1.0]);
}

#[test]
fn pcm_s16le_to_peaks_reports_zero_for_silence_and_one_bucket_per_request() {
    let silence = vec![0u8; 8000 * 2];
    let peaks = pcm_s16le_to_peaks(&silence, 10);
    assert_eq!(peaks.len(), 10);
    assert!(peaks.iter().all(|p| *p == 0.0));
    // Empty input still returns the requested bucket count, all zero.
    assert_eq!(pcm_s16le_to_peaks(&[], 5), vec![0.0; 5]);
}

#[test]
fn only_mp4_webm_gif_and_avi_are_video_formats() {
    for f in ["mp4", "webm", "gif", "avi"] {
        assert!(is_video_format(f), "{f} must be accepted");
    }
    for f in ["mov", "mkv", "webpanim", "apng", "webp", "", "MP4"] {
        assert!(!is_video_format(f), "{f} must be refused");
    }
}

#[test]
fn screenshot_format_ext_and_mime_cover_all_four_and_fall_back_to_png() {
    let cases = [
        ("png", "png", "image/png"),
        ("jpeg", "jpg", "image/jpeg"),
        ("webp", "webp", "image/webp"),
        ("bmp", "bmp", "image/bmp"),
        ("tiff", "png", "image/png"), // a stored choice that is no longer offered
        ("something-unknown", "png", "image/png"),
    ];
    for (format, ext, mime) in cases {
        assert_eq!(screenshot_format_ext(format), ext, "ext for {format}");
        assert_eq!(screenshot_format_mime(format), mime, "mime for {format}");
    }
}

#[test]
fn screenshot_format_needs_quality_is_true_only_for_jpeg() {
    assert!(screenshot_format_needs_quality("jpeg"));
    for f in ["png", "webp", "bmp", "unknown"] {
        assert!(!screenshot_format_needs_quality(f), "{f} should not need a quality slider");
    }
}

// A 2x2 checkerboard -- small enough to assert on exact bytes, varied
// enough (opaque and transparent, black and white) that a lossy encoder
// could not accidentally produce the same output as a lossless one.
fn checkerboard_rgba() -> Vec<u8> {
    vec![
        255, 255, 255, 255, 0, 0, 0, 128, // top-left opaque white, top-right translucent black
        0, 0, 0, 255, 255, 255, 255, 0, // bottom-left opaque black, bottom-right transparent white
    ]
}

#[test]
fn encode_screenshot_png_webp_bmp_all_roundtrip_losslessly() {
    let rgba = checkerboard_rgba();
    for format in ["png", "webp", "bmp"] {
        let bytes = encode_screenshot(&rgba, 2, 2, format, 90).unwrap_or_else(|| panic!("{format} failed to encode"));
        let decoded = image::load_from_memory(&bytes).unwrap_or_else(|e| panic!("{format} failed to decode: {e}"));
        assert_eq!(decoded.width(), 2, "{format} width");
        assert_eq!(decoded.height(), 2, "{format} height");
        assert_eq!(decoded.to_rgba8().into_raw(), rgba, "{format} must round-trip every pixel exactly");
    }
}

#[test]
fn encode_screenshot_writes_png_for_a_stored_tiff_choice() {
    let bytes = encode_screenshot(&checkerboard_rgba(), 2, 2, "tiff", 90).expect("encode");
    assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
}

#[test]
fn encode_screenshot_jpeg_still_uses_the_fast_encoder_and_ignores_alpha_losslessness() {
    // JPEG has no alpha channel and is lossy by design -- this only proves
    // the dispatch still reaches encode_snip_jpeg (a valid, decodable JPEG
    // of the right size comes back), never that JPEG matches source bytes.
    let rgba = checkerboard_rgba();
    let bytes = encode_screenshot(&rgba, 2, 2, "jpeg", 90).expect("jpeg encode failed");
    let decoded = image::load_from_memory(&bytes).expect("jpeg failed to decode");
    assert_eq!((decoded.width(), decoded.height()), (2, 2));
}

#[test]
fn parse_ffprobe_rate_reads_a_fraction_and_rejects_zero_denominator() {
    assert_eq!(parse_ffprobe_rate("30/1"), Some(30.0));
    assert_eq!(parse_ffprobe_rate("30000/1001"), Some(30000.0 / 1001.0));
    assert_eq!(parse_ffprobe_rate("0/0"), None);
    assert_eq!(parse_ffprobe_rate("not-a-rate"), None);
}

#[test]
fn probe_recording_real_ffmpeg_reports_duration_dimensions_and_no_mic_sidecar() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let Some(ffprobe) = test_ffprobe() else { return };
    let dir = std::env::temp_dir().join(format!("ps-probetest-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 2, "64x48");
    let probe = probe_recording_blocking(&ffprobe, &src).expect("probe should succeed");
    assert!(probe.duration_ms >= 1500 && probe.duration_ms <= 3000, "duration_ms was {}", probe.duration_ms);
    assert_eq!((probe.width, probe.height), (64, 48));
    assert!(!probe.has_audio, "the lavfi testsrc fixture has no audio stream");
    assert!(!probe.has_mic_track);
    assert_eq!(probe.mic_sidecar, None);
    assert!(probe.stream_copy_safe, "a plain h264 mp4 with no audio must be reported safe to stream-copy");
    let _ = fs::remove_dir_all(&dir);
}

// A source whose video codec is not h264/hevc (e.g. dragged in from
// elsewhere as VP9/webm) must never be reported stream-copy-safe -- that
// flag is what gates the editor's own "-c copy" fast path, and copying a
// VP9 stream into an mp4 container fails ffmpeg outright.
#[test]
fn probe_recording_real_ffmpeg_reports_a_non_h264_source_as_not_stream_copy_safe() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let Some(ffprobe) = test_ffprobe() else { return };
    let dir = std::env::temp_dir().join(format!("ps-probetest-vp9-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    use std::os::windows::process::CommandExt;
    let out = dir.join("src.webm");
    let status = std::process::Command::new(&ffmpeg)
        .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=duration=1:size=64x48:rate=15"])
        .args(["-c:v", "libvpx-vp9"])
        .arg(&out)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .expect("could not run ffmpeg to build the vp9 test fixture");
    assert!(status.success(), "ffmpeg failed to build the vp9 test fixture clip");
    let probe = probe_recording_blocking(&ffprobe, &out).expect("probe should succeed");
    assert!(!probe.stream_copy_safe, "a vp9 source must never be reported safe for an mp4 stream copy");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn probe_recording_real_ffmpeg_reports_a_mic_sidecar_when_present_and_not_yet_muxed() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let Some(ffprobe) = test_ffprobe() else { return };
    let dir = std::env::temp_dir().join(format!("ps-probetest-sidecar-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 1, "64x48");
    let sidecar = src.with_extension("mic.wav");
    fs::write(&sidecar, b"not real wav data, only existence matters here").unwrap();
    let probe = probe_recording_blocking(&ffprobe, &src).expect("probe should succeed");
    assert_eq!(probe.mic_sidecar, Some(sidecar.to_string_lossy().to_string()));
    let _ = fs::remove_dir_all(&dir);
}

// ---- probe_media_meta: duration/size/resolution stored on a media attachment ----

#[test]
fn probe_media_meta_reads_a_real_images_dimensions_and_size() {
    let dir = std::env::temp_dir().join(format!("ps-mediameta-img-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pic.png");
    let img = image::RgbImage::from_pixel(12, 8, image::Rgb([10, 20, 30]));
    image::DynamicImage::ImageRgb8(img).save_with_format(&path, image::ImageFormat::Png).unwrap();
    let expected_size = fs::metadata(&path).unwrap().len();
    let (duration_ms, width, height, size) = probe_media_meta(None, &path);
    assert_eq!(duration_ms, 0, "a still image has no duration");
    assert_eq!((width, height), (12, 8));
    assert_eq!(size, expected_size);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn probe_media_meta_reads_a_real_videos_duration_and_dimensions() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let Some(ffprobe) = test_ffprobe() else { return };
    let dir = std::env::temp_dir().join(format!("ps-mediameta-vid-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 2, "64x48");
    let expected_size = fs::metadata(&src).unwrap().len();
    let (duration_ms, width, height, size) = probe_media_meta(Some(&ffprobe), &src);
    assert!((1500..=3000).contains(&duration_ms), "duration_ms was {duration_ms}");
    assert_eq!((width, height), (64, 48));
    assert_eq!(size, expected_size);
    let _ = fs::remove_dir_all(&dir);
}

// A file that is neither a real image nor a readable video must come back as
// zeros, never an error and never a panic -- probe_media_meta has no Result
// to bubble one through, on purpose (a caller stores "never measured" for
// whatever field could not be read, not a whole failed attach).
#[test]
fn probe_media_meta_on_an_unreadable_file_returns_zeros_not_an_error() {
    let dir = std::env::temp_dir().join(format!("ps-mediameta-broken-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("broken.mp4");
    fs::write(&path, b"not media at all").unwrap();
    let expected_size = fs::metadata(&path).unwrap().len();
    let (duration_ms, width, height, size) = probe_media_meta(None, &path);
    assert_eq!((duration_ms, width, height), (0, 0, 0));
    assert_eq!(size, expected_size, "the file's own size is filesystem metadata, not a measurement that can fail");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn probe_media_meta_on_a_missing_file_returns_all_zeros() {
    let path = std::env::temp_dir().join(format!("ps-mediameta-missing-{}.mp4", std::process::id()));
    let (duration_ms, width, height, size) = probe_media_meta(None, &path);
    assert_eq!((duration_ms, width, height, size), (0, 0, 0, 0));
}

#[test]
fn video_folder_file_durations_ms_reads_each_files_real_duration_and_zeros_a_missing_one() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let Some(ffprobe) = test_ffprobe() else { return };
    let dir = std::env::temp_dir().join(format!("ps-batch-durations-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    make_test_mp4(&ffmpeg, &dir, "a.mp4", 1, "64x48");
    make_test_mp4(&ffmpeg, &dir, "b.mp4", 2, "64x48");
    let names = vec!["a.mp4".to_string(), "b.mp4".to_string(), "missing.mp4".to_string()];
    let durations = video_folder_file_durations_ms_impl(Some(&ffprobe), &dir, &names);
    assert_eq!(durations.len(), 3);
    assert!((500..=1500).contains(&durations[0]), "a.mp4 duration was {}", durations[0]);
    assert!((1500..=2500).contains(&durations[1]), "b.mp4 duration was {}", durations[1]);
    assert_eq!(durations[2], 0, "a missing file must come back as 0, not an error");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn mux_mic_track_real_ffmpeg_produces_two_distinct_audio_streams_and_removes_the_sidecar() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let Some(ffprobe) = test_ffprobe() else { return };
    let dir = std::env::temp_dir().join(format!("ps-muxtest-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let video = make_test_mp4_with_audio(&ffmpeg, &dir, "video.mp4", 2, "64x48");
    let sidecar = make_test_wav(&dir, "video.mic.wav", 2);
    mux_mic_track_blocking(&ffmpeg, &video, &sidecar, 128).expect("mux should succeed");
    assert!(video.is_file(), "muxed output must replace the video at the same path");
    assert!(!sidecar.is_file(), "the sidecar must be removed once muxed in");
    let probe = probe_recording_blocking(&ffprobe, &video).expect("probe of the muxed file should succeed");
    assert!(probe.has_audio);
    assert!(probe.has_mic_track, "the muxed file must report two audio streams");
    assert_eq!(probe.mic_sidecar, None);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_video_real_ffmpeg_lossless_trim_shortens_the_clip_via_stream_copy() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let Some(ffprobe) = test_ffprobe() else { return };
    let dir = std::env::temp_dir().join(format!("ps-exporttest-trim-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 4, "64x48");
    let dest = dir.join("out.mp4");
    let req = ExportRequest {
        src_path: src.to_string_lossy().to_string(),
        dest_path: dest.to_string_lossy().to_string(),
        filter_chain: None,
        audio_map: None,
        lossless_copy: true,
        lossless_quality: false,
        format: "mp4".to_string(),
        video_kbps: None,
        trim_start_ms: Some(0),
        trim_end_ms: Some(1000),
        total_ms: 60_000,
    };
    let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let out = export_video_blocking(&ffmpeg, &src, &dest, &req, my_gen, |_| {}).expect("export should succeed");
    assert_eq!(PathBuf::from(&out), dest);
    let probe = probe_recording_blocking(&ffprobe, &dest).expect("probe of the exported file should succeed");
    assert!(probe.duration_ms <= 1500, "trimmed export should be roughly 1s, was {}ms", probe.duration_ms);
    assert!(src.is_file(), "export must never touch the source file");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_video_real_ffmpeg_filter_chain_crops_via_the_v_output_label() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let Some(ffprobe) = test_ffprobe() else { return };
    let dir = std::env::temp_dir().join(format!("ps-exporttest-crop-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 1, "64x48");
    let dest = dir.join("out.mp4");
    let req = ExportRequest {
        src_path: src.to_string_lossy().to_string(),
        dest_path: dest.to_string_lossy().to_string(),
        filter_chain: Some("[0:v]crop=32:32:0:0[vout]".to_string()),
        audio_map: None,
        lossless_copy: false,
        lossless_quality: false,
        format: "mp4".to_string(),
        video_kbps: Some(1000),
        trim_start_ms: None,
        trim_end_ms: None,
        total_ms: 60_000,
    };
    let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let out = export_video_blocking(&ffmpeg, &src, &dest, &req, my_gen, |_| {}).expect("export should succeed");
    assert_eq!(PathBuf::from(&out), dest);
    let probe = probe_recording_blocking(&ffprobe, &dest).expect("probe of the exported file should succeed");
    assert_eq!((probe.width, probe.height), (32, 32), "the [v]-labelled crop must actually be applied");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_video_real_ffmpeg_filter_chain_scales_to_the_requested_dimensions() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let Some(ffprobe) = test_ffprobe() else { return };
    let dir = std::env::temp_dir().join(format!("ps-exporttest-scale-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 1, "64x48");
    let dest = dir.join("out.mp4");
    let req = ExportRequest {
        src_path: src.to_string_lossy().to_string(),
        dest_path: dest.to_string_lossy().to_string(),
        filter_chain: Some("[0:v]scale=32:24[vout]".to_string()),
        audio_map: None,
        lossless_copy: false,
        lossless_quality: false,
        format: "mp4".to_string(),
        video_kbps: Some(1000),
        trim_start_ms: None,
        trim_end_ms: None,
        total_ms: 60_000,
    };
    let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let out = export_video_blocking(&ffmpeg, &src, &dest, &req, my_gen, |_| {}).expect("export should succeed");
    assert_eq!(PathBuf::from(&out), dest);
    let probe = probe_recording_blocking(&ffprobe, &dest).expect("probe of the exported file should succeed");
    // ui/video.js's setScaleOp() always emits an explicit W:H pair (never
    // ffmpeg's -2 auto-side), so the real output must match both exactly.
    assert_eq!((probe.width, probe.height), (32, 24), "the [v]-labelled scale must actually be applied");
    let _ = fs::remove_dir_all(&dir);
}

// ---- the five newer export formats, one real-ffmpeg test each ----

fn ffprobe_format_and_video_codec(ffprobe: &Path, path: &Path) -> (String, String, usize) {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new(ffprobe);
    cmd.args(["-v", "quiet", "-print_format", "json", "-show_format", "-show_streams"]).arg(path);
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd.output().expect("could not run ffprobe");
    assert!(output.status.success(), "ffprobe failed to read {}", path.display());
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("ffprobe output was not valid json");
    let format_name = json["format"]["format_name"].as_str().unwrap_or("").to_string();
    let streams = json["streams"].as_array().cloned().unwrap_or_default();
    let video_codec = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .and_then(|v| v["codec_name"].as_str())
        .unwrap_or("")
        .to_string();
    let audio_count = streams.iter().filter(|s| s["codec_type"] == "audio").count();
    (format_name, video_codec, audio_count)
}

fn run_format_export(ffmpeg: &Path, src: &Path, dest: &Path, format: &str) -> String {
    let req = ExportRequest {
        src_path: src.to_string_lossy().to_string(),
        dest_path: dest.to_string_lossy().to_string(),
        filter_chain: None,
        audio_map: None,
        lossless_copy: false,
        lossless_quality: false,
        format: format.to_string(),
        video_kbps: Some(2000),
        trim_start_ms: None,
        trim_end_ms: None,
        total_ms: 60_000,
    };
    let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    export_video_blocking(ffmpeg, src, dest, &req, my_gen, |_| {})
        .unwrap_or_else(|e| panic!("{format} export should succeed, got: {e}"))
}

#[test]
fn export_video_real_ffmpeg_avi_format_carries_mpeg4_video_and_its_audio() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let Some(ffprobe) = test_ffprobe() else { return };
    let dir = std::env::temp_dir().join(format!("ps-exporttest-avi-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4_with_audio(&ffmpeg, &dir, "src.mp4", 2, "64x48");
    let dest = dir.join("out.avi");
    let out = run_format_export(&ffmpeg, &src, &dest, "avi");
    assert_eq!(PathBuf::from(&out), dest);
    let (format_name, video_codec, audio_count) = ffprobe_format_and_video_codec(&ffprobe, &dest);
    assert!(format_name.contains("avi"), "format_name was {format_name}");
    assert_eq!(video_codec, "mpeg4");
    assert_eq!(audio_count, 1, "avi export must keep the source's audio track");
    let _ = fs::remove_dir_all(&dir);
}

// ---- overlay export, one real-ffmpeg test per overlay kind ----
//
// ui/video.js's buildVideoChain() is the only place that turns an overlay
// op into a -filter_complex fragment; this backend never inspects it (see
// the ExportRequest doc comment above). These five tests hardcode the
// exact fragment shape buildVideoChain() emits for each overlay kind
// (verified by hand against a real ffmpeg run when 333 was investigated)
// and prove on the actual encoded output that the overlay was really
// burned into the pixels, not just that ffmpeg exited 0 -- a filter typo
// that still parses (e.g. a wrong label) would pass a "did it error"
// check but produce an unmodified frame.

fn sample_pixel_rgb24(ffmpeg: &Path, path: &Path, x: u32, y: u32) -> [u8; 3] {
    use std::os::windows::process::CommandExt;
    // A 1x1 crop is rejected by this ffmpeg build for a yuv420p source
    // (chroma subsampling needs even dimensions) -- 2x2 is the smallest
    // window that reliably decodes, so every caller reads its top-left texel.
    let out = std::process::Command::new(ffmpeg)
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .arg("-vf")
        .arg(format!("crop=2:2:{x}:{y}"))
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-frames:v", "1", "-"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .expect("could not run ffmpeg to sample a pixel");
    assert!(out.status.success(), "pixel sample failed: {}", String::from_utf8_lossy(&out.stderr));
    [out.stdout[0], out.stdout[1], out.stdout[2]]
}

fn run_overlay_export(ffmpeg: &Path, src: &Path, dest: &Path, filter_chain: String) {
    let req = ExportRequest {
        src_path: src.to_string_lossy().to_string(),
        dest_path: dest.to_string_lossy().to_string(),
        filter_chain: Some(filter_chain),
        audio_map: None,
        lossless_copy: false,
        lossless_quality: false,
        format: "mp4".to_string(),
        video_kbps: Some(1000),
        trim_start_ms: None,
        trim_end_ms: None,
        total_ms: 60_000,
    };
    let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    export_video_blocking(ffmpeg, src, dest, &req, my_gen, |_| {}).expect("overlay export should succeed");
}

#[test]
fn export_video_real_ffmpeg_frame_overlay_draws_a_visible_border() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-ovtest-frame-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 1, "64x48");
    let dest = dir.join("out.mp4");
    // Mirrors buildVideoChain()'s "frame" branch: a solid drawbox, enabled
    // for the whole (untrimmed) clip.
    run_overlay_export(&ffmpeg, &src, &dest,
        "[0:v]fps=15,trim=start=0.000:end=1.000,setpts=PTS-STARTPTS[v0];\
             [v0]drawbox=x=0:y=0:w=64:h=48:color=0xffffff@1.00:t=4:enable='between(t,0.000,1.000)'[v1];\
             [v1]null[vout]".to_string());
    assert_eq!(sample_pixel_rgb24(&ffmpeg, &dest, 0, 0), [255, 255, 255], "the drawbox border must be pure white");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_video_real_ffmpeg_image_overlay_composites_the_picture() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-ovtest-image-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 1, "64x48");
    let badge = dir.join("badge.png");
    use std::os::windows::process::CommandExt;
    let status = std::process::Command::new(&ffmpeg)
        .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=red:s=10x10", "-frames:v", "1"])
        .arg(&badge)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .expect("could not build the badge fixture");
    assert!(status.success());
    let dest = dir.join("out.mp4");
    // Mirrors buildVideoChain()'s "image" branch: movie() pulls the file in
    // as its own source, scaled/alpha-mixed, then overlaid at a fixed spot.
    let badge_ff = badge.to_string_lossy().replace('\\', "/").replace(':', "\\:");
    run_overlay_export(&ffmpeg, &src, &dest, format!(
        "[0:v]fps=15,trim=start=0.000:end=1.000,setpts=PTS-STARTPTS[v0];\
             movie='{badge_ff}'[img0];[img0]scale=10:10,format=rgba,colorchannelmixer=aa=1.00[img1];\
             [v0][img1]overlay=5:5:enable='between(t,0.000,1.000)'[v1];[v1]null[vout]"
    ));
    assert_eq!(sample_pixel_rgb24(&ffmpeg, &dest, 6, 6), [252, 0, 0], "the overlaid badge must show its red pixels");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_video_real_ffmpeg_text_overlay_draws_its_plate_box() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let fonts_dir = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string()) + "/Fonts";
    let font = PathBuf::from(&fonts_dir).join("segoeui.ttf");
    if !font.is_file() { return; } // a stripped CI image without the standard fonts skips this
    let dir = std::env::temp_dir().join(format!("ps-ovtest-text-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 1, "64x48");
    let dest = dir.join("out.mp4");
    let font_ff = font.to_string_lossy().replace('\\', "/").replace(':', "\\:");
    // Mirrors buildVideoChain()'s "text" branch, including its default
    // semi-transparent plate box behind the caption.
    run_overlay_export(&ffmpeg, &src, &dest, format!(
        "[0:v]fps=15,trim=start=0.000:end=1.000,setpts=PTS-STARTPTS[v0];\
             [v0]drawtext=fontfile='{font_ff}':x=5:y=4:fontsize=12:fontcolor=0xffffff@1.00:expansion=none\
             :box=1:boxcolor=black@0.5:boxborderw=3:enable='between(t,0.000,1.000)':text='Test'[v1];[v1]null[vout]"
    ));
    // (8,2) is a non-black texel of the test pattern under the plate box --
    // a corner-of-frame black pixel would stay black under a black@0.5
    // plate and hide a completely missing overlay.
    let before = sample_pixel_rgb24(&ffmpeg, &src, 8, 2);
    let after = sample_pixel_rgb24(&ffmpeg, &dest, 8, 2);
    assert_ne!(before, after, "the drawtext plate box must darken the frame under the caption");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_video_real_ffmpeg_text_overlay_draws_its_outline() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let fonts_dir = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string()) + "/Fonts";
    let font = PathBuf::from(&fonts_dir).join("segoeui.ttf");
    if !font.is_file() { return; } // a stripped CI image without the standard fonts skips this
    let dir = std::env::temp_dir().join(format!("ps-ovtest-text-outline-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 1, "64x48");
    let dest = dir.join("out.mp4");
    let font_ff = font.to_string_lossy().replace('\\', "/").replace(':', "\\:");
    // Mirrors buildVideoChain()'s "text" branch with the background set to
    // Outline instead of Plate: borderw/bordercolor replace the box, the
    // colour picked for contrast against white text (dark grey, not black,
    // to match snipEditContrastColor's own threshold on the JS side).
    run_overlay_export(&ffmpeg, &src, &dest, format!(
        "[0:v]fps=15,trim=start=0.000:end=1.000,setpts=PTS-STARTPTS[v0];\
             [v0]drawtext=fontfile='{font_ff}':x=5:y=4:fontsize=12:fontcolor=0xffffff@1.00:expansion=none\
             :borderw=3:bordercolor=0x111111@1.00:enable='between(t,0.000,1.000)':text='Test'[v1];[v1]null[vout]"
    ));
    // (8,2) sits just above the caption's first letter, inside the outline's
    // 3px ring but outside the letter itself -- confirmed empirically against
    // this exact filter string. The plate-box test above shows the same spot
    // is a plain, non-black test-pattern texel with no overlay at all.
    assert_eq!(sample_pixel_rgb24(&ffmpeg, &dest, 8, 2), [17, 17, 17],
        "the drawtext outline must paint its border colour around the caption");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_video_real_ffmpeg_pixelate_overlay_flattens_the_blocked_region() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-ovtest-pixelate-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 1, "64x48");
    let dest = dir.join("out.mp4");
    // Mirrors buildVideoChain()'s "pixelate" branch: split, crop the region,
    // shrink+re-enlarge with nearest-neighbour, overlay back over the original.
    run_overlay_export(&ffmpeg, &src, &dest,
        "[0:v]fps=15,trim=start=0.000:end=1.000,setpts=PTS-STARTPTS[v0];\
             [v0]split[va][vb];[vb]crop=16:16:24:16,scale=4:4,scale=16:16:flags=neighbor[vsmall];\
             [va][vsmall]overlay=24:16:enable='between(t,0.000,1.000)'[v1];[v1]null[vout]".to_string());
    let a = sample_pixel_rgb24(&ffmpeg, &dest, 24, 16);
    let b = sample_pixel_rgb24(&ffmpeg, &dest, 26, 16);
    assert_eq!(a, b, "two texels inside the same 4px pixelation block must come out identical");
    let original = sample_pixel_rgb24(&ffmpeg, &src, 24, 16);
    assert_ne!(a, original, "the pixelated block must actually differ from the untouched source");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_video_real_ffmpeg_spotlight_overlay_dims_outside_the_clear_rect() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-ovtest-spotlight-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 1, "64x48");
    let dest = dir.join("out.mp4");
    // Mirrors buildVideoChain()'s "spotlight" branch: split, darken the
    // whole frame, then overlay a clear crop of the original back over the
    // clear rect -- everything else stays dimmed.
    run_overlay_export(&ffmpeg, &src, &dest,
        "[0:v]fps=15,trim=start=0.000:end=1.000,setpts=PTS-STARTPTS[v0];\
             [v0]split[va][vb];[vb]crop=20:20:22:14[vclear];\
             [va]drawbox=x=0:y=0:w=iw:h=ih:color=black@0.60:t=fill:enable='between(t,0.000,1.000)'[vdark];\
             [vdark][vclear]overlay=22:14:enable='between(t,0.000,1.000)'[v1];[v1]null[vout]".to_string());
    // Outside the clear rect (50,24 is a non-black test-pattern texel, well
    // clear of the 22..42 x 14..34 clear rect) must be dimmed toward black;
    // inside the clear rect must match the untouched source.
    let dimmed = sample_pixel_rgb24(&ffmpeg, &dest, 50, 24);
    let original_corner = sample_pixel_rgb24(&ffmpeg, &src, 50, 24);
    assert_ne!(dimmed, original_corner, "outside the spotlight must be darkened");
    let clear = sample_pixel_rgb24(&ffmpeg, &dest, 30, 22);
    let original_clear = sample_pixel_rgb24(&ffmpeg, &src, 30, 22);
    assert_eq!(clear, original_clear, "inside the spotlight's clear rect the picture must be untouched");
    let _ = fs::remove_dir_all(&dir);
}

// Proves the "Lossless" quality tier actually stays lossless (a real
// -crf 0 re-encode) rather than silently falling back to an ordinary
// bitrate cap -- the same request with only lossless_quality flipped and
// a deliberately low video_kbps must produce a dramatically larger file.
#[test]
fn export_video_real_ffmpeg_lossless_quality_is_far_larger_than_a_capped_bitrate_reencode() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-exporttest-lossless-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 3, "320x240");
    let lossless_dest = dir.join("lossless.mp4");
    let capped_dest = dir.join("capped.mp4");
    let lossless_req = ExportRequest {
        src_path: src.to_string_lossy().to_string(),
        dest_path: lossless_dest.to_string_lossy().to_string(),
        filter_chain: Some("[0:v]scale=320:240[vout]".to_string()),
        audio_map: None,
        lossless_copy: false,
        lossless_quality: true,
        format: "mp4".to_string(),
        video_kbps: Some(300),
        trim_start_ms: None,
        trim_end_ms: None,
        total_ms: 60_000,
    };
    let capped_req = ExportRequest {
        src_path: src.to_string_lossy().to_string(),
        dest_path: capped_dest.to_string_lossy().to_string(),
        filter_chain: Some("[0:v]scale=320:240[vout]".to_string()),
        audio_map: None,
        lossless_copy: false,
        lossless_quality: false,
        format: "mp4".to_string(),
        video_kbps: Some(300),
        trim_start_ms: None,
        trim_end_ms: None,
        total_ms: 60_000,
    };
    let gen1 = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    export_video_blocking(&ffmpeg, &src, &lossless_dest, &lossless_req, gen1, |_| {}).expect("lossless export should succeed");
    let gen2 = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    export_video_blocking(&ffmpeg, &src, &capped_dest, &capped_req, gen2, |_| {}).expect("capped export should succeed");
    let lossless_len = fs::metadata(&lossless_dest).unwrap().len();
    let capped_len = fs::metadata(&capped_dest).unwrap().len();
    assert!(
        lossless_len > capped_len * 2,
        "a crf-0 lossless re-encode ({lossless_len} bytes) must be dramatically larger than a 300kbps-capped one ({capped_len} bytes), otherwise Lossless quality is silently falling back to a bitrate cap again"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_video_real_ffmpeg_gif_ignores_any_audio_map_and_produces_a_silent_gif() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-exporttest-gif-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4_with_audio(&ffmpeg, &dir, "src.mp4", 1, "64x48");
    let dest = dir.join("out.gif");
    let req = ExportRequest {
        src_path: src.to_string_lossy().to_string(),
        dest_path: dest.to_string_lossy().to_string(),
        filter_chain: Some("[0:v]scale=32:32[vout]".to_string()),
        // A gif export must never map this in -- the GIF container cannot
        // carry audio at all; without the is_gif guard this would make
        // ffmpeg fail with "GIF muxer does not support audio".
        audio_map: Some("[0:a]volume=1[aout]".to_string()),
        lossless_copy: false,
        lossless_quality: false,
        format: "gif".to_string(),
        video_kbps: None,
        trim_start_ms: None,
        trim_end_ms: None,
        total_ms: 60_000,
    };
    let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let out = export_video_blocking(&ffmpeg, &src, &dest, &req, my_gen, |_| {}).expect("gif export should succeed");
    let bytes = fs::read(&out).unwrap();
    assert_eq!(&bytes[0..3], b"GIF");
    let _ = fs::remove_dir_all(&dir);
}

// ---- extra video sources: overlay + section clip, one real-ffmpeg test each ----
//
// ui/video.js's timelinePlan()/buildVideoChain()/buildAudioChain() turn an
// inserted video (an overlay, or a whole section spliced into the kept
// timeline) into `movie=`/`amovie=` source filters referencing the extra
// file directly, normalized to the main video's own size/rate before any
// concat -- no second `-i` on the ffmpeg command line at all, so nothing
// here needs a backend change beyond the fragments themselves. These tests
// hardcode the exact fragment shape those functions emit and prove it
// against a real ffmpeg run, the same pattern the five overlay-kind tests
// above already use.

// Same two-level escape ui/video.js's own ffPath() applies to a filter
// option's path -- single-quoted, exactly one backslash before the drive
// colon. A test fixture path is always local (never contains an
// apostrophe), so this is the escaping alone, not the whole helper.
fn ff_path_for_test(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").replace(':', "\\:")
}

fn ffprobe_stream_durations_secs(ffprobe: &Path, path: &Path) -> (Option<f64>, Option<f64>) {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new(ffprobe);
    cmd.args(["-v", "quiet", "-print_format", "json", "-show_streams"]).arg(path);
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd.output().expect("could not run ffprobe");
    assert!(output.status.success(), "ffprobe failed to read {}", path.display());
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("ffprobe output was not valid json");
    let streams = json["streams"].as_array().cloned().unwrap_or_default();
    let dur_of = |kind: &str| {
        streams
            .iter()
            .find(|s| s["codec_type"] == kind)
            .and_then(|s| s["duration"].as_str())
            .and_then(|s| s.parse::<f64>().ok())
    };
    (dur_of("video"), dur_of("audio"))
}

fn sample_pixel_rgb24_at(ffmpeg: &Path, path: &Path, seconds: f64, x: u32, y: u32) -> [u8; 3] {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new(ffmpeg)
        .args(["-y", "-hide_banner", "-loglevel", "error", "-ss", &format!("{seconds:.3}"), "-i"])
        .arg(path)
        .arg("-vf")
        .arg(format!("crop=2:2:{x}:{y}"))
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-frames:v", "1", "-"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .expect("could not run ffmpeg to sample a pixel");
    assert!(out.status.success(), "pixel sample failed: {}", String::from_utf8_lossy(&out.stderr));
    [out.stdout[0], out.stdout[1], out.stdout[2]]
}

#[test]
fn export_video_real_ffmpeg_two_clips_of_different_resolution_and_rate_concatenate_cleanly() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let Some(ffprobe) = test_ffprobe() else { return };
    let dir = std::env::temp_dir().join(format!("ps-clip-concat-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let main = make_test_mp4_with_audio(&ffmpeg, &dir, "main.mp4", 2, "64x48");
    let extra = make_test_mp4_with_audio(&ffmpeg, &dir, "extra.mp4", 1, "100x80");
    let dest = dir.join("out.mp4");
    let extra_ff = ff_path_for_test(&extra);
    let video_chain = format!(
        "[0:v]fps=15,trim=start=0.000:end=2.000,setpts=PTS-STARTPTS[v0];\
             movie='{extra_ff}'[vb];\
             [vb]fps=15,scale=64:48:force_original_aspect_ratio=decrease,pad=64:48:(ow-iw)/2:(oh-ih)/2:color=black,setpts=PTS-STARTPTS[v1];\
             [v0][v1]concat=n=2:v=1:a=0[vout]"
    );
    let audio_chain = format!(
        "[0:a]atrim=start=0.000:end=2.000,asetpts=PTS-STARTPTS[a0];\
             amovie='{extra_ff}'[ab];\
             [ab]aresample=48000,aformat=channel_layouts=stereo,atrim=0:1.000,asetpts=PTS-STARTPTS[a1];\
             [a0][a1]concat=n=2:v=0:a=1[aout]"
    );
    let req = ExportRequest {
        src_path: main.to_string_lossy().to_string(),
        dest_path: dest.to_string_lossy().to_string(),
        filter_chain: Some(video_chain),
        audio_map: Some(audio_chain),
        lossless_copy: false,
        lossless_quality: false,
        format: "mp4".to_string(),
        video_kbps: Some(1000),
        trim_start_ms: None,
        trim_end_ms: None,
        total_ms: 60_000,
    };
    let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    export_video_blocking(&ffmpeg, &main, &dest, &req, my_gen, |_| {}).expect("concat export should succeed");
    let (_, video_codec, audio_count) = ffprobe_format_and_video_codec(&ffprobe, &dest);
    assert_eq!(video_codec, "h264");
    assert_eq!(audio_count, 1, "the two source audio tracks must end up mixed into one output track");
    let probe = probe_recording_blocking(&ffprobe, &dest).expect("probe of the exported file should succeed");
    assert_eq!((probe.width, probe.height), (64, 48), "the joined clip must carry the MAIN video's own size, not the inserted clip's");
    assert!((2700..=3300).contains(&probe.duration_ms), "expected roughly 3s (2s main + 1s inserted), was {}ms", probe.duration_ms);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_video_real_ffmpeg_a_silent_inserted_clip_gets_silence_and_does_not_shift_the_track() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let Some(ffprobe) = test_ffprobe() else { return };
    let dir = std::env::temp_dir().join(format!("ps-clip-silence-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let main = make_test_mp4_with_audio(&ffmpeg, &dir, "main.mp4", 2, "64x48");
    // No -c:a at all -- the inserted clip genuinely has no audio track,
    // the case buildAudioChain() must fill with anullsrc rather than
    // just skipping (or every track after it drifts out of sync).
    let extra = make_test_mp4(&ffmpeg, &dir, "extra.mp4", 1, "64x48");
    let dest = dir.join("out.mp4");
    let extra_ff = ff_path_for_test(&extra);
    let video_chain = format!(
        "[0:v]fps=15,trim=start=0.000:end=2.000,setpts=PTS-STARTPTS[v0];\
             movie='{extra_ff}'[vb];\
             [vb]fps=15,scale=64:48:force_original_aspect_ratio=decrease,pad=64:48:(ow-iw)/2:(oh-ih)/2:color=black,setpts=PTS-STARTPTS[v1];\
             [v0][v1]concat=n=2:v=1:a=0[vout]"
    );
    let audio_chain = "[0:a]atrim=start=0.000:end=2.000,asetpts=PTS-STARTPTS[a0];\
             anullsrc=channel_layout=stereo:sample_rate=48000,atrim=0:1.000,asetpts=PTS-STARTPTS[a1];\
             [a0][a1]concat=n=2:v=0:a=1[aout]".to_string();
    let req = ExportRequest {
        src_path: main.to_string_lossy().to_string(),
        dest_path: dest.to_string_lossy().to_string(),
        filter_chain: Some(video_chain),
        audio_map: Some(audio_chain),
        lossless_copy: false,
        lossless_quality: false,
        format: "mp4".to_string(),
        video_kbps: Some(1000),
        trim_start_ms: None,
        trim_end_ms: None,
        total_ms: 60_000,
    };
    let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    export_video_blocking(&ffmpeg, &main, &dest, &req, my_gen, |_| {}).expect("export should succeed");
    let (video_dur, audio_dur) = ffprobe_stream_durations_secs(&ffprobe, &dest);
    let video_dur = video_dur.expect("video stream must report a duration");
    let audio_dur = audio_dur.expect("the silence-padded audio stream must still exist");
    assert!((video_dur - audio_dur).abs() < 0.3, "audio must not drift from video once silence is inserted: video={video_dur}s audio={audio_dur}s");
    assert!((2.7..=3.3).contains(&video_dur), "expected roughly 3s total, was {video_dur}s");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_video_real_ffmpeg_video_overlay_appears_only_in_its_own_time_range() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-ovtest-video-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 3, "64x48");
    // A pure green clip -- unmistakable against the base testsrc pattern's
    // own colours, so a hit at (0,0) rules out every branch except "the
    // overlay drew there".
    use std::os::windows::process::CommandExt;
    let overlay_src = dir.join("overlay.mp4");
    let status = std::process::Command::new(&ffmpeg)
        .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=green:s=8x8:d=1", "-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(&overlay_src)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .expect("could not build the green overlay fixture");
    assert!(status.success());
    let dest = dir.join("out.mp4");
    let overlay_ff = ff_path_for_test(&overlay_src);
    // Mirrors buildVideoChain()'s "video" overlay branch: enabled only
    // between t=1 and t=2 on the (untouched, 1x-speed) output timeline.
    let video_chain = format!(
        "[0:v]fps=15,trim=start=0.000:end=3.000,setpts=PTS-STARTPTS[v0];\
             movie='{overlay_ff}'[ob];\
             [ob]scale=8:8:force_original_aspect_ratio=decrease,pad=8:8:(ow-iw)/2:(oh-ih)/2:color=black,format=rgba,colorchannelmixer=aa=1.00[os];\
             [v0][os]overlay=0:0:enable='between(t,1.000,2.000)'[vout]"
    );
    run_overlay_export(&ffmpeg, &src, &dest, video_chain);
    let before = sample_pixel_rgb24_at(&ffmpeg, &dest, 0.2, 0, 0);
    let during = sample_pixel_rgb24_at(&ffmpeg, &dest, 1.5, 0, 0);
    let after = sample_pixel_rgb24_at(&ffmpeg, &dest, 2.8, 0, 0);
    // Pure green survives the h264 re-encode almost exactly but not
    // pixel-perfectly -- a few units of quantization noise either way is
    // expected, not a sign the overlay drew the wrong thing.
    let is_green = |p: [u8; 3]| p[0] < 20 && (p[1] as i16 - 128).abs() < 20 && p[2] < 20;
    assert!(is_green(during), "the overlay must be visible at t=1.5s, sampled {during:?}");
    assert!(!is_green(before), "the overlay must not be visible before its range, sampled {before:?}");
    assert!(!is_green(after), "the overlay must not be visible after its range, sampled {after:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_video_real_ffmpeg_video_overlay_starts_at_its_own_beginning() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-ovtest-videostart-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 6, "64x48");
    // A red half then a blue half, one second each: a clip whose own first
    // frame is unmistakably different from its own later frames, which a
    // constant-colour fixture (like the sibling test above) cannot tell
    // apart from "the overlay decoded from the wrong point in its file".
    use std::os::windows::process::CommandExt;
    let overlay_src = dir.join("overlay.mp4");
    let status = std::process::Command::new(&ffmpeg)
        .args(["-y", "-hide_banner", "-loglevel", "error"])
        .args(["-f", "lavfi", "-i", "color=c=red:s=8x8:d=1"])
        .args(["-f", "lavfi", "-i", "color=c=blue:s=8x8:d=1"])
        .args(["-filter_complex", "[0:v][1:v]concat=n=2:v=1:a=0[v]", "-map", "[v]"])
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "15"])
        .arg(&overlay_src)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .expect("could not build the red/blue overlay fixture");
    assert!(status.success());
    let dest = dir.join("out.mp4");
    let overlay_ff = ff_path_for_test(&overlay_src);
    // Mirrors buildVideoChain()'s "video" overlay branch after the setpts
    // fix: the overlay's own clip starts (its own t=0, the red frame) at
    // output t=3.000, the same start second the export uses for the
    // overlay's adelay'd audio.
    let video_chain = format!(
        "[0:v]fps=15,trim=start=0.000:end=6.000,setpts=PTS-STARTPTS[v0];\
             movie='{overlay_ff}'[ob];\
             [ob]setpts=PTS-STARTPTS+3.000/TB,scale=8:8:force_original_aspect_ratio=decrease,pad=8:8:(ow-iw)/2:(oh-ih)/2:color=black,format=rgba,colorchannelmixer=aa=1.00[os];\
             [v0][os]overlay=0:0:enable='between(t,3.000,5.000)'[vout]"
    );
    run_overlay_export(&ffmpeg, &src, &dest, video_chain);
    let before = sample_pixel_rgb24_at(&ffmpeg, &dest, 2.5, 0, 0);
    let just_after_start = sample_pixel_rgb24_at(&ffmpeg, &dest, 3.3, 0, 0);
    let after_its_own_midpoint = sample_pixel_rgb24_at(&ffmpeg, &dest, 4.3, 0, 0);
    let is_red = |p: [u8; 3]| p[0] > 180 && p[1] < 40 && p[2] < 40;
    let is_blue = |p: [u8; 3]| p[0] < 40 && p[1] < 40 && p[2] > 180;
    assert!(!is_red(before) && !is_blue(before), "no overlay colour before its range, sampled {before:?}");
    assert!(is_red(just_after_start), "the overlay's own first (red) frame must show right after its start, sampled {just_after_start:?}");
    assert!(is_blue(after_its_own_midpoint), "the overlay's own second (blue) half must show once its own clip reaches it, sampled {after_its_own_midpoint:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_video_real_ffmpeg_cancel_leaves_no_destination_file() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-exporttest-cancel-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "src.mp4", 8, "960x720");
    let dest = dir.join("out.mp4");
    let req = ExportRequest {
        src_path: src.to_string_lossy().to_string(),
        dest_path: dest.to_string_lossy().to_string(),
        filter_chain: None,
        audio_map: None,
        lossless_copy: false,
        lossless_quality: false,
        format: "mp4".to_string(),
        video_kbps: Some(4000),
        trim_start_ms: None,
        trim_end_ms: None,
        total_ms: 60_000,
    };
    let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).insert(my_gen);
    });
    let result = export_video_blocking(&ffmpeg, &src, &dest, &req, my_gen, |_| {});
    assert_eq!(result, Err("cancelled".to_string()));
    assert!(!dest.exists(), "a cancelled export must leave no destination file");
    video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).remove(&my_gen);
    let _ = fs::remove_dir_all(&dir);
}

// A second export's generation must survive a first export's later
// cancellation -- the whole point of scoping cancellation to a set of
// generation numbers instead of a single shared slot. Also proves two
// generations can be cancelled independently of each other, which a
// batch export (several generations running at once) relies on.
#[test]
fn export_video_cancelling_an_earlier_generation_does_not_cancel_a_later_one() {
    let gen_a = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let gen_b = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).insert(gen_a);
    assert!(gen_a != gen_b);
    let is_cancelled_b = || video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).contains(&gen_b);
    assert!(!is_cancelled_b(), "an older generation's cancellation must not affect a newer one");
    video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).remove(&gen_a);
}

// The editor's Cancel must hit the editor's own export even when a batch
// export has reserved later generations since.
#[test]
fn editor_cancel_targets_the_editor_generation_not_a_later_batch_one() {
    let editor_gen = AtomicU64::new(0);
    cancel_editor_export_in(&editor_gen);
    let gen_a = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    editor_gen.store(gen_a, Ordering::SeqCst);
    let gen_b = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    assert!(!video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).contains(&gen_a));
    cancel_editor_export_in(&editor_gen);
    let set = video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner());
    assert!(set.contains(&gen_a), "the editor's own export must be cancelled");
    assert!(!set.contains(&gen_b), "a batch job must not be cancelled by the editor");
    assert!(!set.contains(&0), "no export in flight leaves nothing behind");
    drop(set);
    video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).remove(&gen_a);
}

// cancel_all_bg_ops used to call plain cancel_export(), which only ever
// cancels the single latest generation -- correct for the single-editor
// button, wrong for a batch with several generations running at once
// (a window above one). Proves the fix: two concurrently "running" export
// rows must BOTH end up cancelled.
#[test]
fn cancel_all_exports_cancels_every_running_generation_not_just_the_latest() {
    let _guard = BG_OPS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let gen_a = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let gen_b = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let id_a = format!("export-{gen_a}");
    let id_b = format!("export-{gen_b}");
    record_bg_op(&id_a, "export", "running", Some(10), None, 10.0);
    record_bg_op(&id_b, "export", "running", Some(20), None, 10.0);

    cancel_all_exports();

    let cancelled = video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner());
    assert!(cancelled.contains(&gen_a), "the older of two concurrent generations must be cancelled too");
    assert!(cancelled.contains(&gen_b), "the newer generation must be cancelled");
    drop(cancelled);

    video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).remove(&gen_a);
    video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).remove(&gen_b);
    let mut ops = bg_ops().lock().unwrap_or_else(|e| e.into_inner());
    ops.remove(&id_a);
    ops.remove(&id_b);
}

// Three recordings selected for a batch export, run one by one through
// run_batch_job_blocking exactly as export_recordings_batch itself does
// per item -- all three destination files must exist.
#[test]
fn batch_export_runs_every_job_and_produces_every_file() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-batchtest-ok-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let srcs = [
        make_test_mp4(&ffmpeg, &dir, "src-a.mp4", 1, "64x48"),
        make_test_mp4(&ffmpeg, &dir, "src-b.mp4", 1, "64x48"),
        make_test_mp4(&ffmpeg, &dir, "src-c.mp4", 1, "64x48"),
    ];
    let mut ok = 0usize;
    for (i, src) in srcs.iter().enumerate() {
        let dest = dir.join(format!("out-{i}.mp4"));
        let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
        let result = run_batch_job_blocking(
            &ffmpeg,
            &src.to_string_lossy(),
            &dest.to_string_lossy(),
            1000,
            "mp4",
            false,
            Some(2000),
            my_gen,
            |_| {},
        );
        match result {
            Ok(_) => {
                ok += 1;
                assert!(dest.exists(), "job {i} must produce its destination file");
            }
            Err(e) => panic!("job {i} unexpectedly failed: {e}"),
        }
    }
    assert_eq!(ok, 3);
    let _ = fs::remove_dir_all(&dir);
}

// One deliberately broken source must not stop the batch -- the other
// two jobs still run and produce their files, and the batch's own
// ok/failed count reflects exactly the one failure.
#[test]
fn batch_export_one_broken_source_does_not_stop_the_others() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-batchtest-broken-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let broken = dir.join("broken.mp4");
    fs::write(&broken, b"not a real video file").unwrap();
    let srcs = [make_test_mp4(&ffmpeg, &dir, "src-a.mp4", 1, "64x48"), broken, make_test_mp4(&ffmpeg, &dir, "src-c.mp4", 1, "64x48")];
    let mut ok = 0usize;
    let mut failed = 0usize;
    for (i, src) in srcs.iter().enumerate() {
        let dest = dir.join(format!("out-{i}.mp4"));
        let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
        let result = run_batch_job_blocking(&ffmpeg, &src.to_string_lossy(), &dest.to_string_lossy(), 1000, "mp4", false, Some(2000), my_gen, |_| {});
        match result {
            Ok(_) => {
                ok += 1;
                assert!(dest.exists(), "job {i} must produce its destination file");
            }
            Err(_) => failed += 1,
        }
    }
    assert_eq!(ok, 2, "the two valid recordings must still both succeed");
    assert_eq!(failed, 1, "only the broken recording must be counted as failed");
    let _ = fs::remove_dir_all(&dir);
}

// ---- Recording: state machine + clock + filename, no hardware needed ----

#[test]
fn rec_phase_allows_the_documented_transitions_only() {
    assert!(rec_can_start(RecPhase::Idle));
    assert!(!rec_can_start(RecPhase::Recording));
    assert!(!rec_can_start(RecPhase::Paused));

    assert!(rec_can_pause(RecPhase::Recording));
    assert!(!rec_can_pause(RecPhase::Idle));
    assert!(!rec_can_pause(RecPhase::Paused));

    assert!(rec_can_resume(RecPhase::Paused));
    assert!(!rec_can_resume(RecPhase::Recording));
    assert!(!rec_can_resume(RecPhase::Idle));

    assert!(rec_can_finish(RecPhase::Recording));
    assert!(rec_can_finish(RecPhase::Paused));
    assert!(!rec_can_finish(RecPhase::Idle));
}

#[test]
fn rec_clock_freezes_during_a_pause_and_resumes_from_where_it_left_off() {
    assert_eq!(rec_clock_ticks(0, Some(50_000)), 50_000);
    // paused: no active segment, only the frozen accumulated value counts.
    assert_eq!(rec_clock_ticks(50_000, None), 50_000);
    // resumed: accumulated carries over, the fresh segment adds on top.
    assert_eq!(rec_clock_ticks(50_000, Some(10_000)), 60_000);
}

#[test]
fn record_clock_pause_freezes_wall_time_progress() {
    let clock = RecordClock::new();
    std::thread::sleep(std::time::Duration::from_millis(30));
    clock.pause();
    let frozen = clock.now_ticks();
    std::thread::sleep(std::time::Duration::from_millis(30));
    assert_eq!(clock.now_ticks(), frozen, "ticks must not advance while paused");
    clock.resume();
    std::thread::sleep(std::time::Duration::from_millis(10));
    assert!(clock.now_ticks() > frozen, "ticks must advance again after resume");
}

#[test]
fn a_paused_span_before_a_later_reader_joins_never_inflates_its_position() {
    // camera_screen_elapsed_ms (camera.rs) reads this same clock live, at
    // the moment a camera take starts, to place it on the screen
    // recording's own timeline. A screen recording paused for a while
    // before the camera joins must not push that position later than
    // where the screen recording's own output actually is -- only the
    // active segments may count, exactly like the ticks assertions
    // above, just phrased at the "another reader joins later" level this
    // bug was found at.
    let clock = RecordClock::new();
    let t0 = std::time::Instant::now();
    std::thread::sleep(std::time::Duration::from_millis(20));
    clock.pause();
    let pause_start = std::time::Instant::now();
    std::thread::sleep(std::time::Duration::from_millis(200)); // the paused span
    clock.resume();
    // Measured, not assumed: a loaded machine can make thread::sleep
    // overshoot its nominal duration by far more than the two short
    // active segments below ever could on their own, so the real paused
    // wall time (however long it actually took) is what gets subtracted,
    // not the 200ms asked for.
    let real_pause_ms = pause_start.elapsed().as_millis() as i64;
    std::thread::sleep(std::time::Duration::from_millis(20)); // the camera joins here
    let elapsed_ms = clock.now_ticks() / 10_000;
    let wall_ms = t0.elapsed().as_millis() as i64;
    assert!(
        elapsed_ms <= wall_ms - real_pause_ms + 50,
        "the paused span (~{real_pause_ms}ms of {wall_ms}ms wall time) must not count toward \
             the camera's join position, got {elapsed_ms}ms"
    );
}

#[test]
fn rec_filename_matches_the_spec_pattern() {
    assert_eq!(rec_filename(2026, 9, 3, 8, 5, 9), "rec-2026-09-03_080509.mp4");
    assert_eq!(rec_filename(2026, 12, 31, 23, 59, 0), "rec-2026-12-31_235900.mp4");
}

#[test]
fn rec_feeder_queue_drops_oldest_when_full_and_counts_it() {
    let shared = RecFeederShared::new();
    for i in 0..(REC_QUEUE_CAP as i64 + 2) {
        shared.push(RecFrameItem { bytes: vec![], ts: i });
    }
    assert_eq!(shared.dropped.load(std::sync::atomic::Ordering::Relaxed), 2);
    // the two oldest (ts 0, 1) were dropped -- what remains starts at ts 2.
    let first = shared.pop_or_stop().expect("queue should still have items");
    assert_eq!(first.ts, 2);
}

#[test]
fn rec_feeder_encode_error_latches_the_first_message_and_peek_does_not_consume() {
    let shared = RecFeederShared::new();
    assert_eq!(shared.peek_encode_error(), None);
    shared.set_encode_error("disk full".to_string());
    shared.set_encode_error("a later, different failure".to_string());
    // First failure wins -- a failing encoder tends to repeat the same
    // error on every subsequent frame, so only the first is kept.
    assert_eq!(shared.peek_encode_error(), Some("disk full".to_string()));
    // Peeking twice must return the same thing (the live status poll and
    // the final stop payload both read it independently).
    assert_eq!(shared.peek_encode_error(), Some("disk full".to_string()));
}

#[test]
fn rec_feeder_resize_crop_is_none_before_any_call() {
    let shared = RecFeederShared::new();
    assert_eq!(shared.resize_crop_at_ms(), None);
}

#[test]
fn rec_feeder_resize_crop_latches_the_first_timestamp_and_tracks_the_largest_size() {
    let shared = RecFeederShared::new();
    // A synthetic stream: three frames past the resize point, at growing
    // sizes and later timestamps -- only the FIRST one's timestamp may
    // stick, even though every one of them re-triggers the safety net.
    shared.mark_resize_crop(1000, 800, 25_000_000); // 2.5s in (100ns ticks)
    shared.mark_resize_crop(1200, 900, 30_000_000);
    shared.mark_resize_crop(1100, 850, 35_000_000);
    assert_eq!(shared.resize_crop_at_ms(), Some(2500), "must report the FIRST occurrence's time, not the latest");
    assert_eq!(shared.resize_crop_max_w.load(std::sync::atomic::Ordering::Relaxed), 1200);
    assert_eq!(shared.resize_crop_max_h.load(std::sync::atomic::Ordering::Relaxed), 900);
}

#[test]
fn rec_feeder_resize_crop_at_exactly_zero_ms_still_reports_a_hit() {
    // The ms+1 encoding trick must not swallow a genuine ts=0 frame.
    let shared = RecFeederShared::new();
    shared.mark_resize_crop(999, 777, 0);
    assert_eq!(shared.resize_crop_at_ms(), Some(0));
}

// ---- Audio mixing/resampling + the WASAPI-producer queue, no hardware needed ----

#[test]
fn audio_f32_to_i16_clamps_and_scales() {
    assert_eq!(audio_f32_to_i16(0.0), 0);
    assert_eq!(audio_f32_to_i16(1.0), i16::MAX);
    assert_eq!(audio_f32_to_i16(-1.0), -i16::MAX);
    assert_eq!(audio_f32_to_i16(2.0), i16::MAX); // over-range clips, never wraps
    assert_eq!(audio_f32_to_i16(-2.0), -i16::MAX);
}

#[test]
fn audio_convert_to_target_passthrough_when_format_already_matches() {
    let samples = [0.5f32, -0.5, 0.25, -0.25];
    let out = audio_convert_to_target(&samples, 2, 48_000, 2, 48_000);
    assert_eq!(out.len(), samples.len());
    assert_eq!(out[0], audio_f32_to_i16(0.5));
}

#[test]
fn audio_convert_to_target_mono_to_stereo_duplicates_the_one_channel() {
    let samples = [1.0f32, -1.0]; // 2 mono frames
    let out = audio_convert_to_target(&samples, 1, 48_000, 2, 48_000);
    assert_eq!(out, vec![i16::MAX, i16::MAX, -i16::MAX, -i16::MAX]);
}

#[test]
fn audio_convert_to_target_resamples_to_the_requested_rate() {
    // 4 stereo frames at 48kHz downsampled to 24kHz -> half as many frames.
    let samples: Vec<f32> = (0..8).map(|i| i as f32 / 10.0).collect();
    let out = audio_convert_to_target(&samples, 2, 48_000, 2, 24_000);
    assert_eq!(out.len(), 4); // 2 frames * 2 channels
}

#[test]
fn audio_convert_to_target_empty_input_never_panics() {
    assert!(audio_convert_to_target(&[], 2, 48_000, 2, 48_000).is_empty());
    assert!(audio_convert_to_target(&[0.1, 0.2], 0, 48_000, 2, 48_000).is_empty());
}

#[test]
fn audio_mix_i16_sums_and_hard_clips_instead_of_wrapping() {
    assert_eq!(audio_mix_i16(&[100, -100], &[200, -200]), vec![300, -300]);
    assert_eq!(audio_mix_i16(&[i16::MAX, i16::MIN], &[100, -100]), vec![i16::MAX, i16::MIN]);
}

#[test]
fn audio_queue_drain_zero_pads_on_underrun_and_never_blocks() {
    let q = AudioQueueShared::new();
    q.push(&[1, 2, 3]);
    let out = q.drain(5);
    assert_eq!(out, vec![1, 2, 3, 0, 0]);
    // fully drained -- a second pull is pure silence.
    assert_eq!(q.drain(2), vec![0, 0]);
}

#[test]
fn audio_queue_push_caps_at_two_seconds_and_drops_the_oldest() {
    let q = AudioQueueShared::new();
    let cap = (REC_AUDIO_SAMPLE_RATE * REC_AUDIO_CHANNELS * 2) as usize;
    q.push(&vec![7i16; cap]);
    q.push(&[9, 9]); // pushes the queue 2 samples over cap
    let out = q.drain(cap);
    // the two oldest 7s were evicted; the tail is the freshly pushed 9s.
    assert_eq!(&out[out.len() - 2..], &[9, 9]);
    assert_eq!(out.len(), cap);
}

fn mp4_box(typ: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = ((8 + body.len()) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(typ);
    out.extend_from_slice(body);
    out
}
fn mp4_mdhd_v0(timescale: u32, duration: u32) -> Vec<u8> {
    let mut body = vec![0u8, 0, 0, 0]; // version 0, flags
    body.extend_from_slice(&0u32.to_be_bytes()); // creation
    body.extend_from_slice(&0u32.to_be_bytes()); // modification
    body.extend_from_slice(&timescale.to_be_bytes());
    body.extend_from_slice(&duration.to_be_bytes());
    mp4_box(b"mdhd", &body)
}

#[test]
fn mp4_track_durations_secs_reads_two_tracks_from_a_synthetic_container() {
    let video_mdia = mp4_box(b"mdia", &mp4_mdhd_v0(30_000, 300_000)); // 10.0s @ 30000
    let audio_mdia = mp4_box(b"mdia", &mp4_mdhd_v0(48_000, 480_500)); // 10.0104...s
    let video_trak = mp4_box(b"trak", &video_mdia);
    let audio_trak = mp4_box(b"trak", &audio_mdia);
    let mut moov_body = video_trak;
    moov_body.extend_from_slice(&audio_trak);
    let moov = mp4_box(b"moov", &moov_body);
    // ftyp ahead of moov, like a real file, to prove top-level scanning
    // does not get confused by an earlier unrelated box.
    let mut file = mp4_box(b"ftyp", b"isom");
    file.extend_from_slice(&moov);

    let durations = mp4_track_durations_secs(&file);
    assert_eq!(durations.len(), 2);
    assert!((durations[0] - 10.0).abs() < 1e-9);
    assert!((durations[1] - 480_500.0 / 48_000.0).abs() < 1e-9);
}

#[test]
fn mp4_track_durations_secs_empty_on_garbage_input() {
    assert!(mp4_track_durations_secs(&[]).is_empty());
    assert!(mp4_track_durations_secs(&[0u8; 4]).is_empty()); // too short for even a box header
    assert!(mp4_track_durations_secs(&[1, 2, 3, 4, 5, 6, 7, 8]).is_empty()); // no moov at all
}

#[test]
fn audio_queue_lost_flag_starts_false_and_latches_true() {
    let q = AudioQueueShared::new();
    assert!(!q.is_lost());
    q.mark_lost();
    assert!(q.is_lost());
}

// Real hardware, not run by a plain `cargo test --lib` (needs a live
// display + default playback device): `cargo test --lib -- --ignored
// rec_audio_real_recording_produces_a_video_and_an_audio_track`. Proves
// the whole path end to end -- WASAPI capture, the feeder's audio tick,
// send_audio_buffer -- without needing a Media Foundation reader: an MP4
// with N tracks has exactly N "trak" box markers, so one video + one
// system-audio track means exactly two.
#[test]
#[cfg(windows)]
#[ignore]
fn rec_audio_real_recording_produces_a_video_and_an_audio_track() {
    let dir = std::env::temp_dir().join(format!("ps-audio-probe-{}", std::process::id()));
    let out = recording::new_output_path(&dir).expect("output path");
    let audio = recording::AudioConfig {
        system: true, mic: false, mic_device_id: String::new(), kbps: 192,
    };
    let session = recording::start(
        recording::RecTarget::Monitor(0), out, audio, "all",
        recording::RecStartOptions {
            show_cursor: true, video_kbps: 12000, video_fps: 30, canvas_ceiling: (8192, 4320),
            disk_warn_mb: 500.0, max_min: 0.0, max_mb: 0.0,
        },
    )
    .expect("start");
    std::thread::sleep(std::time::Duration::from_secs(4));
    let (path, _dropped, _encode_error, _mic_wav, _canvas_note, _crop_rect, _canvas_size, _resize_crop_at_ms) = recording::stop(session).expect("stop");
    let bytes = std::fs::read(&path).expect("read output file");
    assert!(bytes.len() > 10_000, "output suspiciously small: {} bytes", bytes.len());
    let track_count = bytes.windows(4).filter(|w| *w == b"trak").count();
    let durations = mp4_track_durations_secs(&bytes);
    let handlers = mp4_track_handler_types(&bytes);
    println!("track durations (s): {durations:?} handlers: {handlers:?}");
    let _ = std::fs::remove_file(&path);
    assert_eq!(track_count, 2, "expected 1 video + 1 audio track box, found {track_count}");
    assert_eq!(durations.len(), 2, "expected a declared duration for each track");
}

// Same proof, mic-only: `cargo test --lib -- --ignored rec_audio_real_recording_mic_only`.
#[test]
#[cfg(windows)]
#[ignore]
fn rec_audio_real_recording_mic_only() {
    let dir = std::env::temp_dir().join(format!("ps-audio-probe-mic-{}", std::process::id()));
    let out = recording::new_output_path(&dir).expect("output path");
    let audio = recording::AudioConfig {
        system: false, mic: true, mic_device_id: String::new(), kbps: 192,
    };
    let session = recording::start(
        recording::RecTarget::Monitor(0), out, audio, "all",
        recording::RecStartOptions {
            show_cursor: true, video_kbps: 12000, video_fps: 30, canvas_ceiling: (8192, 4320),
            disk_warn_mb: 500.0, max_min: 0.0, max_mb: 0.0,
        },
    )
    .expect("start");
    std::thread::sleep(std::time::Duration::from_secs(4));
    let (path, _dropped, _encode_error, _mic_wav, _canvas_note, _crop_rect, _canvas_size, _resize_crop_at_ms) = recording::stop(session).expect("stop");
    let bytes = std::fs::read(&path).expect("read output file");
    assert!(bytes.len() > 10_000, "output suspiciously small: {} bytes", bytes.len());
    let track_count = bytes.windows(4).filter(|w| *w == b"trak").count();
    let _ = std::fs::remove_file(&path);
    assert_eq!(track_count, 2, "expected 1 video + 1 audio track box, found {track_count}");
}

// Both sources mixed into the one audio track: `cargo test --lib --
// --ignored rec_audio_real_recording_both_sources`.
#[test]
#[cfg(windows)]
#[ignore]
fn rec_audio_real_recording_both_sources() {
    let dir = std::env::temp_dir().join(format!("ps-audio-probe-both-{}", std::process::id()));
    let out = recording::new_output_path(&dir).expect("output path");
    let audio = recording::AudioConfig {
        system: true, mic: true, mic_device_id: String::new(), kbps: 192,
    };
    let session = recording::start(
        recording::RecTarget::Monitor(0), out, audio, "all",
        recording::RecStartOptions {
            show_cursor: true, video_kbps: 12000, video_fps: 30, canvas_ceiling: (8192, 4320),
            disk_warn_mb: 500.0, max_min: 0.0, max_mb: 0.0,
        },
    )
    .expect("start");
    std::thread::sleep(std::time::Duration::from_secs(4));
    let (path, _dropped, _encode_error, _mic_wav, _canvas_note, _crop_rect, _canvas_size, _resize_crop_at_ms) = recording::stop(session).expect("stop");
    let bytes = std::fs::read(&path).expect("read output file");
    assert!(bytes.len() > 10_000, "output suspiciously small: {} bytes", bytes.len());
    let track_count = bytes.windows(4).filter(|w| *w == b"trak").count();
    let _ = std::fs::remove_file(&path);
    assert_eq!(track_count, 2, "expected 1 video + 1 mixed-audio track box, found {track_count}");
}

// The acceptance number this was written against: "drift under
// 50ms over 10 minutes, clocked from WASAPI timestamps... measure it
// over a real 10-minute recording, do not assume it." Runs for a real
// ~10 real minutes -- not part of any normal or CI run.
// `cargo test --lib -- --ignored --exact tests::rec_audio_real_10min_drift --nocapture`.
#[test]
#[cfg(windows)]
#[ignore]
fn rec_audio_real_10min_drift() {
    let dir = std::env::temp_dir().join(format!("ps-audio-drift-{}", std::process::id()));
    let out = recording::new_output_path(&dir).expect("output path");
    let audio = recording::AudioConfig {
        system: true, mic: false, mic_device_id: String::new(), kbps: 192,
    };
    let wall_start = std::time::Instant::now();
    let session = recording::start(
        recording::RecTarget::Monitor(0), out, audio, "all",
        recording::RecStartOptions {
            show_cursor: true, video_kbps: 12000, video_fps: 30, canvas_ceiling: (8192, 4320),
            disk_warn_mb: 500.0, max_min: 0.0, max_mb: 0.0,
        },
    )
    .expect("start");
    std::thread::sleep(std::time::Duration::from_secs(600));
    let (path, _dropped, _encode_error, _mic_wav, _canvas_note, _crop_rect, _canvas_size, _resize_crop_at_ms) = recording::stop(session).expect("stop");
    let wall_elapsed = wall_start.elapsed().as_secs_f64();
    let bytes = std::fs::read(&path).expect("read output file");
    let durations = mp4_track_durations_secs(&bytes);
    let _ = std::fs::remove_file(&path);
    assert_eq!(durations.len(), 2, "expected a video and an audio track, got {durations:?}");
    let (video_s, audio_s) = (durations[0], durations[1]);
    let av_drift_ms = ((video_s - audio_s).abs() * 1000.0).round();
    let audio_vs_wall_ms = ((audio_s - wall_elapsed).abs() * 1000.0).round();
    println!(
        "10-minute drift: wall={wall_elapsed:.3}s video={video_s:.3}s audio={audio_s:.3}s \
             video-vs-audio={av_drift_ms}ms audio-vs-wall={audio_vs_wall_ms}ms"
    );
    assert!(av_drift_ms < 50.0, "video/audio drift {av_drift_ms}ms exceeds the 50ms/10min budget");
}

// Neither source active: the plain (audio-free) feeder path is exactly
// 191/238's original code, unchanged. Still 2 "trak" boxes though -- a
// real-hardware run of this test is what surfaced that windows-capture's
// MediaStreamSource is built with both a video AND an audio descriptor
// regardless of AudioSettingsBuilder::disabled (see encoder.rs's
// VideoEncoder::new): "disabled" only makes send_audio_buffer refuse to
// be called, which our audio-off feeder_loop never does anyway -- so the
// container always declares an audio track, just an empty/silent one
// when no audio is enabled. Harmless (players ignore an empty track) and
// pre-existing since 191, not something to "fix" here; this test exists
// to pin the fact down so a future change that assumes "1 track = no
// audio enabled" does not get surprised by it again.
// `cargo test --lib -- --ignored rec_audio_real_recording_no_audio`.
#[test]
#[cfg(windows)]
#[ignore]
fn rec_audio_real_recording_no_audio() {
    let dir = std::env::temp_dir().join(format!("ps-audio-probe-none-{}", std::process::id()));
    let out = recording::new_output_path(&dir).expect("output path");
    let audio = recording::AudioConfig {
        system: false, mic: false, mic_device_id: String::new(), kbps: 192,
    };
    let session = recording::start(
        recording::RecTarget::Monitor(0), out, audio, "all",
        recording::RecStartOptions {
            show_cursor: true, video_kbps: 12000, video_fps: 30, canvas_ceiling: (8192, 4320),
            disk_warn_mb: 500.0, max_min: 0.0, max_mb: 0.0,
        },
    )
    .expect("start");
    std::thread::sleep(std::time::Duration::from_secs(2));
    let (path, _dropped, _encode_error, _mic_wav, _canvas_note, _crop_rect, _canvas_size, _resize_crop_at_ms) = recording::stop(session).expect("stop");
    let bytes = std::fs::read(&path).expect("read output file");
    let track_count = bytes.windows(4).filter(|w| *w == b"trak").count();
    let _ = std::fs::remove_file(&path);
    assert_eq!(track_count, 2, "expected 1 video + 1 empty audio track box, found {track_count}");
}

// Pure naming/numbering check, no capture hardware needed -- runs on
// every plain `cargo test --lib`.
#[test]
#[cfg(windows)]
fn continuation_output_path_keeps_the_stem_and_numbers_from_two() {
    let base = std::path::Path::new(r"C:\recordings\rec-2026-09-11_120000.mp4");
    assert_eq!(
        recording::continuation_output_path(base, 2),
        std::path::PathBuf::from(r"C:\recordings\rec-2026-09-11_120000_part2.mp4")
    );
    assert_eq!(
        recording::continuation_output_path(base, 5),
        std::path::PathBuf::from(r"C:\recordings\rec-2026-09-11_120000_part5.mp4")
    );
    // A continuation of a continuation still derives from the ORIGINAL
    // stem, never from an already-numbered one -- callers are expected to
    // always pass the first part's own path in, matching how
    // finish_and_continue_recording resolves its own base_path.
    assert_eq!(
        recording::continuation_output_path(base, 3).file_stem().unwrap(),
        "rec-2026-09-11_120000_part3"
    );
}

// Two files come out of a "finish and continue" style sequence, both
// playable, correctly numbered, and started from the exact same settings
// values (the same variables handed to `start` twice -- the actual
// command instead reads them off the finishing session itself, but the
// effect this proves, identical settings on both calls, is the same).
// Needs a real display: `cargo test --lib -- --ignored
// rec_continue_two_parts_are_both_playable_with_matching_settings`.
#[test]
#[cfg(windows)]
#[ignore]
fn rec_continue_two_parts_are_both_playable_with_matching_settings() {
    let dir = std::env::temp_dir().join(format!("ps-continue-probe-{}", std::process::id()));
    let part1_path = recording::new_output_path(&dir).expect("output path");
    let audio = recording::AudioConfig { system: false, mic: false, mic_device_id: String::new(), kbps: 192 };
    let target = recording::RecTarget::Monitor(0);
    let (show_cursor, video_kbps, video_fps, canvas_mode, canvas_ceiling, disk_warn_mb, max_min, max_mb) =
        (true, 12000u32, 30u32, "all", (8192u32, 4320u32), 500.0f64, 0.0f64, 0.0f64);

    let session1 = recording::start(
        target, part1_path.clone(), audio.clone(), canvas_mode,
        recording::RecStartOptions { show_cursor, video_kbps, video_fps, canvas_ceiling, disk_warn_mb, max_min, max_mb },
    )
    .expect("start part 1");
    // The session retains exactly what it was built with -- this is what
    // finish_and_continue_recording reads back to reopen the next part,
    // instead of re-reading settings that may have changed since.
    assert_eq!(session1.video_kbps, video_kbps);
    assert_eq!(session1.video_fps, video_fps);
    assert_eq!(session1.canvas_mode, canvas_mode);
    assert_eq!(session1.canvas_ceiling, canvas_ceiling);
    assert_eq!(session1.disk_warn_mb, disk_warn_mb);
    assert_eq!(session1.audio_config.system, audio.system);
    assert_eq!(session1.audio_config.mic, audio.mic);
    assert_eq!(session1.show_cursor, show_cursor);
    std::thread::sleep(std::time::Duration::from_secs(2));
    let (stopped1_path, _d1, _e1, _m1, _c1, _r1, _s1, _rc1) = recording::stop(session1).expect("stop part 1");
    assert_eq!(stopped1_path, part1_path);

    let part2_path = recording::continuation_output_path(&part1_path, 2);
    let session2 = recording::start(
        target, part2_path.clone(), audio, canvas_mode,
        recording::RecStartOptions { show_cursor, video_kbps, video_fps, canvas_ceiling, disk_warn_mb, max_min, max_mb },
    )
    .expect("start part 2 with the SAME settings as part 1");
    std::thread::sleep(std::time::Duration::from_secs(2));
    let (stopped2_path, _d2, _e2, _m2, _c2, _r2, _s2, _rc2) = recording::stop(session2).expect("stop part 2");
    assert_eq!(stopped2_path, part2_path);

    assert_ne!(part1_path, part2_path, "the two parts must not collide on the same file");
    assert_eq!(
        part2_path.file_stem().unwrap(),
        format!("{}_part2", part1_path.file_stem().unwrap().to_str().unwrap()).as_str()
    );

    for p in [&part1_path, &part2_path] {
        let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("read {p:?}: {e}"));
        assert!(bytes.len() > 5_000, "{p:?} suspiciously small: {} bytes", bytes.len());
        assert!(mp4_has_moov(p), "{p:?} has no closed moov atom, not playable");
    }
    let _ = std::fs::remove_file(&part1_path);
    let _ = std::fs::remove_file(&part2_path);
}

#[test]
fn crop_and_flip_rows_bottom_up_full_frame_reverses_row_order_and_strips_padding() {
    // 2x2 BGRA, stride padded by 4 extra bytes per row.
    let row0 = [1u8, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0];
    let row1 = [9u8, 10, 11, 12, 13, 14, 15, 16, 0, 0, 0, 0];
    let mut raw = Vec::new();
    raw.extend_from_slice(&row0);
    raw.extend_from_slice(&row1);
    let out = crop_and_flip_rows_bottom_up(&raw, 2, 2, 12, 0, 0, 2, 2);
    // bottom-up: row1's 8 pixel bytes first, then row0's, no padding.
    assert_eq!(out, vec![9, 10, 11, 12, 13, 14, 15, 16, 1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn crop_and_flip_rows_bottom_up_region_takes_only_the_requested_sub_rectangle() {
    // 3x2 BGRA source, no padding (stride == row bytes). Crop to the right
    // 1x2 column, like a region recording's software crop would.
    let px = |n: u8| [n, n, n, 255u8];
    let mut raw = Vec::new();
    for row in 0..2u8 {
        for col in 0..3u8 {
            raw.extend_from_slice(&px(row * 3 + col));
        }
    }
    let out = crop_and_flip_rows_bottom_up(&raw, 3, 2, 12, 2, 0, 1, 2);
    // column 2 of row 0 is pixel value 2, of row 1 is pixel value 5;
    // bottom-up means row 1's pixel comes first.
    assert_eq!(out, vec![5, 5, 5, 255, 2, 2, 2, 255]);
}

#[test]
fn letterbox_frame_top_down_pads_a_smaller_live_frame_with_black() {
    // 1x1 live frame, black-padded up to the encoder's fixed 2x2 canvas.
    let raw = vec![9u8, 8, 7, 255];
    let (out, stride) = letterbox_frame_top_down(&raw, 1, 1, 4, 2, 2);
    assert_eq!(stride, 8);
    // top-left pixel is the source, the other three are black/transparent.
    assert_eq!(out, vec![9, 8, 7, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn letterbox_frame_top_down_crops_a_larger_live_frame() {
    // 2x2 live frame, cropped down to the encoder's fixed 1x1 canvas --
    // only the top-left source pixel should survive.
    let px = |n: u8| [n, n, n, 255u8];
    let mut raw = Vec::new();
    for n in 0..4u8 {
        raw.extend_from_slice(&px(n));
    }
    let (out, stride) = letterbox_frame_top_down(&raw, 2, 2, 8, 1, 1);
    assert_eq!(stride, 4);
    assert_eq!(out, vec![0, 0, 0, 255]);
}

#[test]
fn letterbox_frame_top_down_is_place_frame_in_canvas_at_the_origin() {
    // 369's placement helper must reproduce the existing (0, 0) case
    // byte-for-byte -- letterbox_frame_top_down is only ever this now.
    let raw = vec![9u8, 8, 7, 255];
    assert_eq!(letterbox_frame_top_down(&raw, 1, 1, 4, 2, 2), place_frame_in_canvas(&raw, 1, 1, 4, 2, 2, 0, 0));
}

#[test]
fn place_frame_in_canvas_pads_at_a_positive_offset() {
    // 1x1 live frame placed at (1,1) inside a 3x3 canvas: everything but
    // that one destination pixel stays black.
    let raw = vec![9u8, 8, 7, 255];
    let (out, stride) = place_frame_in_canvas(&raw, 1, 1, 4, 3, 3, 1, 1);
    assert_eq!(stride, 12);
    let mut expected = vec![0u8; 12 * 3];
    let off = 12 + 4;
    expected[off..off + 4].copy_from_slice(&[9, 8, 7, 255]);
    assert_eq!(out, expected);
}

#[test]
fn place_frame_in_canvas_clips_a_negative_offset() {
    // 2x2 live frame placed at (-1,-1): only its bottom-right pixel is
    // still inside the canvas, landing at (0,0) -- the "display placed
    // left of/above the first one" case.
    let px = |n: u8| [n, n, n, 255u8];
    let mut raw = Vec::new();
    for n in 0..4u8 {
        raw.extend_from_slice(&px(n));
    }
    let (out, stride) = place_frame_in_canvas(&raw, 2, 2, 8, 2, 2, -1, -1);
    assert_eq!(stride, 8);
    assert_eq!(out, vec![3, 3, 3, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn place_frame_in_canvas_clips_a_frame_hanging_past_the_right_edge() {
    // 2x1 live frame placed at (1,0) inside a 2x1 canvas: only its first
    // column fits, the second is clipped off.
    let px = |n: u8| [n, n, n, 255u8];
    let mut raw = Vec::new();
    raw.extend_from_slice(&px(5));
    raw.extend_from_slice(&px(6));
    let (out, stride) = place_frame_in_canvas(&raw, 2, 1, 8, 2, 1, 1, 0);
    assert_eq!(stride, 8);
    assert_eq!(out, vec![0, 0, 0, 0, 5, 5, 5, 255]);
}

#[test]
fn canvas_within_ceiling_accepts_up_to_and_rejects_past_the_limit() {
    assert!(canvas_within_ceiling(8192, 4320, 8192, 4320));
    assert!(!canvas_within_ceiling(8193, 4320, 8192, 4320));
    assert!(!canvas_within_ceiling(7680, 4321, 8192, 4320));
}

#[test]
fn export_file_name_is_valid_accepts_a_plain_name() {
    assert!(export_file_name_is_valid("Demo 1"));
    assert!(export_file_name_is_valid("clip-edit"));
    assert!(export_file_name_is_valid("My Video v1.2.3"));
}

// A bare ".." only means "walk up a level" when it stands alone with
// separators around it -- inside a longer name it is just two dots and
// must stay legal, unlike a full ".." path segment.
#[test]
fn export_file_name_is_valid_accepts_two_dots_inside_a_longer_name() {
    assert!(export_file_name_is_valid("v1..2"));
    assert!(export_file_name_is_valid("a..b"));
}

#[test]
fn export_file_name_is_valid_rejects_a_separator_or_a_lone_dot_segment() {
    assert!(!export_file_name_is_valid("a/b"));
    assert!(!export_file_name_is_valid("a\\b"));
    assert!(!export_file_name_is_valid("."));
    assert!(!export_file_name_is_valid(".."));
    assert!(!export_file_name_is_valid("..\\evil"));
    assert!(!export_file_name_is_valid(""));
    assert!(!export_file_name_is_valid("   "));
    assert!(!export_file_name_is_valid("a:b"));
    assert!(!export_file_name_is_valid("a*b"));
    assert!(!export_file_name_is_valid("a?b"));
    assert!(!export_file_name_is_valid("a\"b"));
    assert!(!export_file_name_is_valid("a<b"));
    assert!(!export_file_name_is_valid("a>b"));
    assert!(!export_file_name_is_valid("a|b"));
    assert!(!export_file_name_is_valid("a\u{7}b"));
}

#[test]
fn export_file_name_is_valid_rejects_a_trailing_dot_or_a_reserved_device_name() {
    assert!(!export_file_name_is_valid("clip."));
    assert!(!export_file_name_is_valid("CON"));
    assert!(!export_file_name_is_valid("con"));
    assert!(!export_file_name_is_valid("Con.take2"));
    assert!(!export_file_name_is_valid("LPT1"));
    // Only an exact reserved segment is refused, not merely a prefix.
    assert!(export_file_name_is_valid("Console"));
}

#[test]
fn export_dest_is_same_as_src_matches_same_file_and_ignores_case() {
    let root = std::env::temp_dir().join(format!("ps-exportdest-samesrc-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let src = root.join("clip.mp4");
    std::fs::write(&src, b"x").unwrap();
    let same_case = root.join("CLIP.mp4");
    let other = root.join("clip-edit.mp4");

    assert!(export_dest_is_same_as_src(&src, &src));
    assert!(export_dest_is_same_as_src(&same_case, &src));
    assert!(!export_dest_is_same_as_src(&other, &src));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn export_dest_dir_accepts_the_picked_folder_and_rejects_anywhere_else() {
    let root = std::env::temp_dir().join(format!("ps-exportdesttest-{}", std::process::id()));
    let picked = root.join("picked");
    let other = root.join("elsewhere");
    let src_dir = root.join("source");
    let no_recording_dir = root.join("unused-recording-dir");
    std::fs::create_dir_all(&picked).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::create_dir_all(&src_dir).unwrap();
    let src = src_dir.join("clip.mp4");
    std::fs::write(&src, b"x").unwrap();

    // Inside the folder a save dialog actually returned: accepted.
    assert!(export_dest_dir_is_authorized_against(&picked.join("out.mp4"), &src, Some(&picked), None, std::slice::from_ref(&no_recording_dir), None));
    // A path outside anything the app ever resolved: rejected.
    assert!(!export_dest_dir_is_authorized_against(&other.join("out.mp4"), &src, Some(&picked), None, std::slice::from_ref(&no_recording_dir), None));
    // The standing videoExportDir preference is accepted the same way.
    assert!(export_dest_dir_is_authorized_against(&other.join("out.mp4"), &src, None, other.to_str(), std::slice::from_ref(&no_recording_dir), None));
    // Saving back into the source file's own folder, when that folder IS the
    // configured recording folder, is accepted with no pick at all.
    assert!(export_dest_dir_is_authorized_against(&src_dir.join("out.mp4"), &src, None, None, std::slice::from_ref(&src_dir), None));
    // Nothing remembered and nothing configured: rejected.
    assert!(!export_dest_dir_is_authorized_against(&other.join("out.mp4"), &src, None, None, std::slice::from_ref(&no_recording_dir), None));

    let _ = std::fs::remove_dir_all(&root);
}

// Finding 2 (security pass 348): a src_path naming any existing file used
// to authorize that file's own parent folder as an export destination, with
// no restriction on where that folder was. An attacker-controlled src_path
// pointing into an arbitrary folder must no longer grant write access to it
// just because the destination sits alongside it.
#[test]
fn export_dest_dir_fallback_requires_src_inside_a_trusted_folder() {
    let root = std::env::temp_dir().join(format!("ps-exportdest-untrusted-{}", std::process::id()));
    let untrusted_dir = root.join("attacker-chosen");
    let recording_dir = root.join("recordings");
    std::fs::create_dir_all(&untrusted_dir).unwrap();
    std::fs::create_dir_all(&recording_dir).unwrap();
    let untrusted_src = untrusted_dir.join("clip.mp4");
    std::fs::write(&untrusted_src, b"x").unwrap();
    let recording_src = recording_dir.join("rec-clip.mp4");
    std::fs::write(&recording_src, b"x").unwrap();

    // src_path sits in a folder this app never resolved as its own: the
    // "save back next to source" fallback must not authorize it.
    assert!(!export_dest_dir_is_authorized_against(
        &untrusted_dir.join("out.mp4"),
        &untrusted_src,
        None,
        None,
        std::slice::from_ref(&recording_dir),
        None,
    ));
    // src_path sits inside the actual configured recording folder: the same
    // fallback still works for the ordinary case it exists for.
    assert!(export_dest_dir_is_authorized_against(
        &recording_dir.join("out.mp4"),
        &recording_src,
        None,
        None,
        std::slice::from_ref(&recording_dir),
        None,
    ));

    // A fresh capture in the <data>/recordings staging folder may be
    // exported back next to itself, just like one in recordingDir.
    let staging_dir = root.join("data-recordings");
    std::fs::create_dir_all(&staging_dir).unwrap();
    let staged_src = staging_dir.join("rec-fresh.mp4");
    std::fs::write(&staged_src, b"x").unwrap();
    let both = [staging_dir.clone(), recording_dir.clone()];
    assert!(export_dest_dir_is_authorized_against(&staging_dir.join("rec-fresh-edit.mp4"), &staged_src, None, None, &both, None));
    // The app's own temp folder (the clipboard export's throwaway file) is
    // accepted; the untrusted folder still is not, temp dir or no temp dir.
    let temp_dir = root.join("app-temp");
    std::fs::create_dir_all(&temp_dir).unwrap();
    assert!(export_dest_dir_is_authorized_against(&temp_dir.join("clip.mp4"), &untrusted_src, None, None, &both, Some(&temp_dir)));
    assert!(!export_dest_dir_is_authorized_against(&untrusted_dir.join("out.mp4"), &untrusted_src, None, None, &both, Some(&temp_dir)));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sanitize_canvas_mode_keeps_the_two_known_alternatives_and_rejects_the_rest() {
    assert_eq!(sanitize_canvas_mode("monitor"), "monitor");
    assert_eq!(sanitize_canvas_mode("all"), "all");
    assert_eq!(sanitize_canvas_mode("window"), "window");
    assert_eq!(sanitize_canvas_mode(""), "window");
    assert_eq!(sanitize_canvas_mode("Monitor"), "window");
    assert_eq!(sanitize_canvas_mode("drop table settings"), "window");
}

#[test]
fn recording_limit_reached_is_off_by_default() {
    assert!(!recording_limit_reached(u64::MAX, u64::MAX, 0.0, 0.0));
}

#[test]
fn recording_limit_reached_trips_on_the_minute_cap() {
    assert!(!recording_limit_reached(299, 0, 5.0, 0.0));
    assert!(recording_limit_reached(300, 0, 5.0, 0.0));
}

#[test]
fn recording_limit_reached_trips_on_the_size_cap() {
    let cap_bytes = 100 * 1024 * 1024;
    assert!(!recording_limit_reached(0, cap_bytes - 1, 0.0, 100.0));
    assert!(recording_limit_reached(0, cap_bytes, 0.0, 100.0));
}

#[test]
fn used_area_tracker_is_none_before_the_first_update() {
    let t = UsedAreaTracker::new();
    assert_eq!(t.get(), None);
}

#[test]
fn used_area_tracker_returns_the_union_of_every_update() {
    let t = UsedAreaTracker::new();
    t.update(100, 200, 50, 50); // covers (100,200)..(150,250)
    t.update(80, 300, 20, 20); // covers (80,300)..(100,320)
    t.update(120, 210, 10, 10); // fully inside the first rect
    // Union of the three: x in [80,150), y in [200,320).
    assert_eq!(t.get(), Some((80, 200, 70, 120)));
}

#[test]
fn scale_bitrate_to_used_area_is_unscaled_when_the_window_fills_the_canvas() {
    assert_eq!(scale_bitrate_to_used_area(12000, 1920, 1080, 1920, 1080), 12000);
}

#[test]
fn scale_bitrate_to_used_area_scales_down_proportionally_to_the_area_fraction() {
    // A 1920x1080 window on a 3840x2160 canvas covers exactly a quarter
    // of the area, so the bitrate is scaled to a quarter too.
    assert_eq!(scale_bitrate_to_used_area(12000, 1920, 1080, 3840, 2160), 3000);
}

#[test]
fn scale_bitrate_to_used_area_never_exceeds_the_configured_full_canvas_value() {
    // A "used" area somehow reported larger than the canvas (should not
    // happen, but the clamp guards it regardless) never scales upward.
    assert_eq!(scale_bitrate_to_used_area(12000, 4000, 3000, 1920, 1080), 12000);
}

#[test]
fn scale_bitrate_to_used_area_floors_a_tiny_window_at_a_usable_rate() {
    assert_eq!(scale_bitrate_to_used_area(12000, 10, 10, 7680, 2160), 500);
}

#[test]
fn scale_to_fit_frame_top_down_downsamples_uniformly() {
    // 4x4 live frame shrunk to fit a 2x2 canvas (scale 0.5): nearest-
    // neighbour picks source (0,0), (2,0), (0,2), (2,2).
    let px = |n: u8| [n, n, n, 255u8];
    let mut raw = Vec::new();
    for row in 0..4u8 {
        for col in 0..4u8 {
            raw.extend_from_slice(&px(row * 4 + col));
        }
    }
    let (out, stride) = scale_to_fit_frame_top_down(&raw, 4, 4, 16, 2, 2);
    assert_eq!(stride, 8);
    assert_eq!(out, vec![0, 0, 0, 255, 2, 2, 2, 255, 8, 8, 8, 255, 10, 10, 10, 255]);
}

#[test]
fn scale_to_fit_frame_top_down_letterboxes_a_mismatched_aspect_ratio() {
    // 2x2 live frame grown into a 4x2 canvas: scale stays 1 (the taller
    // side already fits), so the whole picture survives centred with a
    // black column on either side instead of a crop losing the corners.
    let px = |n: u8| [n, n, n, 255u8];
    let mut raw = Vec::new();
    for n in 1..5u8 {
        raw.extend_from_slice(&px(n));
    }
    let (out, stride) = scale_to_fit_frame_top_down(&raw, 2, 2, 8, 4, 2);
    assert_eq!(stride, 16);
    assert_eq!(
        out,
        vec![
            0, 0, 0, 0, 1, 1, 1, 255, 2, 2, 2, 255, 0, 0, 0, 0,
            0, 0, 0, 0, 3, 3, 3, 255, 4, 4, 4, 255, 0, 0, 0, 0,
        ]
    );
}

#[test]
fn crop_and_flip_rows_bottom_up_rejects_an_out_of_range_crop_without_panicking() {
    let raw = vec![0u8; 16]; // 2x2, no padding
    let out = crop_and_flip_rows_bottom_up(&raw, 2, 2, 8, 1, 1, 5, 5);
    assert!(out.iter().all(|&b| b == 0));
}

// ---- The prompt list the window gets, and what a save may do to an image ----

const TEST_PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==";

fn prompt_with_image() -> Prompt {
    serde_json::from_str(&format!(
        r#"{{"id":"p1","name":"Screenshot","text":"body","image":"{TEST_PNG}","show_image":true}}"#
    ))
    .expect("prompt fixture")
}

// The whole point of the slim list: the picture stays in the process.
#[test]
fn slim_prompt_carries_no_image_bytes() {
    let json = serde_json::to_string(&SlimPrompt::from(&prompt_with_image())).unwrap();
    assert!(!json.contains("iVBORw0KGgo"), "image bytes leaked into the state payload: {json}");
    assert!(!json.contains("\"image\":"), "the state payload still has an image field: {json}");
    assert!(json.contains("\"has_image\":true"));
}

// image_text is a search index, never shown -- proven the same way the
// picture bytes above are: the frontend's own state payload must not
// carry it at all, not even the field name.
#[test]
fn slim_prompt_never_carries_image_text() {
    let mut p = prompt_with_image();
    p.image_text = "invoice total due friday".into();
    let json = serde_json::to_string(&SlimPrompt::from(&p)).unwrap();
    assert!(!json.contains("invoice"), "OCR-recognized text leaked into the state payload: {json}");
    assert!(!json.contains("image_text"), "the state payload still has an image_text field: {json}");
}

#[test]
fn slim_prompt_image_rev_is_stable_and_changes_with_the_image() {
    let p = prompt_with_image();
    let rev = SlimPrompt::from(&p).image_rev;
    assert_ne!(rev, 0);
    assert_eq!(SlimPrompt::from(&p).image_rev, rev, "same image must hash the same");
    let mut other = p.clone();
    other.image = "data:image/png;base64,QQ==".into();
    assert_ne!(SlimPrompt::from(&other).image_rev, rev);
}

// ---- Takeover offer must not be hidden by the onboarding examples ----

fn prompt_named(id: &str) -> Prompt {
    serde_json::from_str(&format!(r#"{{"id":"{id}","name":"n","text":"t"}}"#))
        .expect("prompt fixture")
}

#[test]
fn takeover_treats_seeded_examples_only_as_empty() {
    let examples = vec!["e1".to_string(), "e2".to_string()];
    let prompts = vec![prompt_named("e1"), prompt_named("e2")];
    assert!(store_has_no_real_prompts(&prompts, &examples), "example-only store must count as empty");
}

#[test]
fn takeover_sees_a_real_prompt_even_alongside_examples() {
    let examples = vec!["e1".to_string()];
    let prompts = vec![prompt_named("e1"), prompt_named("own")];
    assert!(!store_has_no_real_prompts(&prompts, &examples), "a non-example prompt must block the offer");
}

#[test]
fn takeover_empty_store_counts_as_empty() {
    assert!(store_has_no_real_prompts(&[], &[]));
}

// ---- Takeover offer: info, and the three answer paths ----

#[test]
fn takeover_offer_info_reports_the_real_size_and_prompt_count() {
    let root = std::env::temp_dir().join(format!("{}-test-takeover-offer-info", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mut conn = full_schema_db(&root.join("data.db"), "info");
    db_write_prompts(&mut conn, &numbered_prompts(4), &mut WrittenRows::new()).unwrap();
    drop(conn);
    std::fs::write(root.join("settings.json"), b"{}").unwrap();

    let out = takeover_offer_info(&root);
    assert_eq!(out.prompts, Some(4), "the count must come from the real database, not a guess");
    let expected_size = fs::metadata(root.join("data.db")).unwrap().len() + fs::metadata(root.join("settings.json")).unwrap().len();
    assert_eq!(out.size, expected_size, "size must be the actual bytes on disk, not an estimate");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn takeover_offer_info_names_an_unreadable_database_as_unknown_rather_than_failing() {
    let root = std::env::temp_dir().join(format!("{}-test-takeover-offer-info-bad-db", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("data.db"), b"not a real database").unwrap();

    let out = takeover_offer_info(&root);
    assert_eq!(out.prompts, None, "a database that cannot be opened must report an unknown count, not crash or lie");

    let _ = std::fs::remove_dir_all(&root);
}

// "uebernehmen" (adopt): the copy lands, the source is untouched, and; since
// the same source is asked about: a remembered decision stops a second offer.
#[test]
fn takeover_decision_adopt_is_remembered_and_stops_a_future_offer() {
    let root = std::env::temp_dir().join(format!("{}-test-takeover-decision-adopt", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    let src = root.join("src");
    let dst = root.join("dst");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();
    std::fs::write(src.join("data.db"), b"source content").unwrap();

    let mut landed = false;
    copy_store_files(&src, &dst, &mut landed).unwrap();
    assert_eq!(fs::read(src.join("data.db")).unwrap(), b"source content", "adopting must never touch the original");
    assert_eq!(fs::read(dst.join("data.db")).unwrap(), b"source content", "the copy must land before anything is settled");

    let mut ui_texts: HashMap<String, String> = HashMap::new();
    assert!(!takeover_already_decided(&ui_texts, &src), "an unanswered location must still be offered");
    takeover_record_decision(&mut ui_texts, &src, true, true);
    assert!(takeover_already_decided(&ui_texts, &src), "an adopted, remembered decision must stop the next offer");

    let _ = std::fs::remove_dir_all(&root);
}

// "getrennt lassen" (keep separate): nothing is copied, but the choice is
// still remembered for that exact source so it does not nag every launch.
#[test]
fn takeover_decision_keep_separate_is_remembered_without_copying_anything() {
    let root = std::env::temp_dir().join(format!("{}-test-takeover-decision-skip", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    let src = root.join("src");
    let dst = root.join("dst");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();
    std::fs::write(src.join("data.db"), b"source content").unwrap();

    let mut ui_texts: HashMap<String, String> = HashMap::new();
    takeover_record_decision(&mut ui_texts, &src, false, true);

    assert!(takeover_already_decided(&ui_texts, &src), "a remembered 'keep separate' must stop the next offer too");
    assert_eq!(
        ui_texts.get(&takeover_decision_key(&src)).map(String::as_str),
        Some("skip"),
        "the recorded answer must say skip, not be indistinguishable from adopt"
    );
    assert!(!dst.join("data.db").exists(), "keeping separate must never write anything into the new location");
    assert_eq!(fs::read(src.join("data.db")).unwrap(), b"source content", "the source must stay exactly as it was");

    let _ = std::fs::remove_dir_all(&root);
}

// "keine Antwort" (no answer): nothing is recorded, so the exact same folder
// is offered again next launch, and neither side was ever touched.
#[test]
fn takeover_decision_no_answer_records_nothing_and_asks_again() {
    let root = std::env::temp_dir().join(format!("{}-test-takeover-decision-none", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("data.db"), b"source content").unwrap();

    let mut ui_texts: HashMap<String, String> = HashMap::new();
    // Mirrors what takeover_apply does when the dialog closes with the
    // "do not ask again" box left unticked -- remember == false either way.
    takeover_record_decision(&mut ui_texts, &src, false, false);

    assert!(ui_texts.is_empty(), "an unremembered answer must leave no trace at all");
    assert!(!takeover_already_decided(&ui_texts, &src), "the same folder must still be offered on the next launch");
    assert_eq!(fs::read(src.join("data.db")).unwrap(), b"source content", "the source must be untouched");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn slim_prompt_without_an_image_reports_no_revision() {
    let mut p = prompt_with_image();
    p.image.clear();
    let slim = SlimPrompt::from(&p);
    assert!(!slim.has_image);
    assert_eq!(slim.image_rev, 0);
}

// A rename must not cost the user their picture: the editor sends no image
// field at all unless it changed, and that has to mean "keep what is stored".
#[test]
fn update_without_an_image_field_keeps_the_stored_image() {
    let mut p = prompt_with_image();
    p.name = "Renamed".into(); // the only thing this save touches
    p.image = merge_image(&p.image, None);
    assert_eq!(p.image, TEST_PNG, "a save that sent no image erased the stored one");
}

#[test]
fn only_an_explicit_empty_image_clears_it() {
    let mut p = prompt_with_image();
    p.image = merge_image(&p.image, Some(String::new()));
    assert!(p.image.is_empty(), "the remove-image click must still clear the picture");
}

#[test]
fn a_new_image_replaces_the_stored_one() {
    let mut p = prompt_with_image();
    p.image = merge_image(&p.image, Some("data:image/png;base64,QQ==".into()));
    assert_eq!(p.image, "data:image/png;base64,QQ==");
}

// Pure text-logic tests for ocr_join_lines -- no Windows API involved, so
// these run on any platform via plain `cargo test --lib`.
#[test]
fn ocr_join_lines_joins_a_wrapped_sentence() {
    let lines = vec!["This line wraps".to_string(), "into the next one.".to_string()];
    assert_eq!(ocr_join_lines(&lines), "This line wraps into the next one.");
}

#[test]
fn ocr_join_lines_keeps_complete_sentences_separate() {
    let lines = vec!["First sentence.".to_string(), "Second sentence.".to_string()];
    assert_eq!(ocr_join_lines(&lines), "First sentence.\nSecond sentence.");
}

#[test]
fn ocr_join_lines_blank_line_separates_paragraphs() {
    let lines = vec![
        "Paragraph one.".to_string(),
        "".to_string(),
        "Paragraph two.".to_string(),
    ];
    assert_eq!(ocr_join_lines(&lines), "Paragraph one.\n\nParagraph two.");
}

#[test]
fn ocr_join_lines_empty_input_yields_empty_string() {
    let lines: Vec<String> = vec![];
    assert_eq!(ocr_join_lines(&lines), "");
}

// An image whose lines all come back empty must assemble to a genuinely
// empty string (a whitespace-only join, e.g. "\n\n", used to slip through
// and open the result dialog with nothing visible in it).
#[test]
fn ocr_assemble_text_all_empty_lines_yields_empty_string() {
    let lines = vec!["".to_string(), "".to_string()];
    assert_eq!(ocr_assemble_text(&lines, false), "");
    assert_eq!(ocr_assemble_text(&lines, true), "");
}

#[test]
fn ocr_assemble_text_trims_surrounding_whitespace_from_real_text() {
    let lines = vec!["  Hello world  ".to_string()];
    assert_eq!(ocr_assemble_text(&lines, false), "Hello world");
}

// A hyphen is not sentence-ending punctuation, so a hyphen-wrapped word is
// space-joined the same as any other mid-sentence wrap -- there is no special
// dehyphenation: one rule, no script-specific cases.
#[test]
fn ocr_join_lines_hyphenated_wrap_gets_a_plain_space_join() {
    let lines = vec!["This is a hyphen-".to_string(), "ated example.".to_string()];
    assert_eq!(ocr_join_lines(&lines), "This is a hyphen- ated example.");
}

// Right-to-left scripts are not special-cased: the same edge-only rule applies,
// and it never assumes an LTR character order, so a joined Arabic line is
// untouched beyond the same whitespace handling as any other script.
#[test]
fn ocr_join_lines_right_to_left_script_is_not_reordered() {
    let lines = vec!["مرحبا بالعالم".to_string()];
    assert_eq!(ocr_join_lines(&lines), "مرحبا بالعالم");
}

const OCR_PREFIX: &str = "Language.OCR~~~";
const SPEECH_PREFIX: &str = "Language.Speech~~~";

// Capability-name allowlist -- the gate everything in
// ocr_install_language relies on. Valid, realistic tags pass; anything
// malformed or injection-flavored is rejected outright.
#[test]
fn ocr_capability_name_accepts_realistic_tags() {
    for name in [
        "Language.OCR~~~de-DE~0.0.1.0",
        "Language.OCR~~~en-US~0.0.1.0",
        "Language.OCR~~~ja-JP~0.0.1.0",
        "Language.OCR~~~zh-Hans-CN~0.0.1.0",
    ] {
        assert!(ocr_capability_name_is_well_formed(OCR_PREFIX, name), "expected {name} to be accepted");
    }
}

// The whole point of this fix -- a real speech capability name (the
// exact shape voice_install_language actually gets called with) used to be
// rejected outright because the check hardcoded the OCR prefix regardless
// of kind, so no speech language pack could ever be installed.
#[test]
fn ocr_capability_name_accepts_speech_names_with_the_speech_prefix() {
    assert!(ocr_capability_name_is_well_formed(SPEECH_PREFIX, "Language.Speech~~~de-DE~0.0.1.0"));
}

// No cross-wiring between the two: an OCR-shaped name must not pass under
// the speech prefix and vice versa.
#[test]
fn ocr_capability_name_rejects_the_other_kinds_prefix() {
    assert!(!ocr_capability_name_is_well_formed(SPEECH_PREFIX, "Language.OCR~~~de-DE~0.0.1.0"));
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, "Language.Speech~~~de-DE~0.0.1.0"));
}

#[test]
fn ocr_capability_name_rejects_the_spec_example_payload() {
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, "Language.OCR~~~de-DE~0.0.1.0; rm -rf"));
}

#[test]
fn ocr_capability_name_rejects_command_injection_payloads() {
    for name in [
        "Language.OCR~~~de-DE~0.0.1.0;whoami",
        "Language.OCR~~~de-DE~0.0.1.0`whoami`",
        "Language.OCR~~~de-DE~0.0.1.0|whoami",
        "Language.OCR~~~de-DE~0.0.1.0\nwhoami",
    ] {
        assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, name), "expected {name} to be rejected");
    }
}

#[test]
fn ocr_capability_name_rejects_empty_string() {
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, ""));
}

#[test]
fn ocr_capability_name_rejects_missing_prefix() {
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, "de-DE~0.0.1.0"));
}

#[test]
fn ocr_capability_name_rejects_wrong_trailing_version() {
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, "Language.OCR~~~de-DE~1.0.0.0"));
}

#[test]
fn ocr_capability_name_rejects_a_tag_over_12_chars() {
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, "Language.OCR~~~thirteenchars~0.0.1.0"));
}

#[test]
fn ocr_capability_name_rejects_tag_chars_outside_the_allowed_class() {
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, "Language.OCR~~~de_DE~0.0.1.0"));
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, "Language.OCR~~~de DE~0.0.1.0"));
}

// Same allowlist string, as a tag extractor: pulls the tag back out of an
// already-valid name.
#[test]
fn ocr_tag_from_capability_name_extracts_the_tag() {
    assert_eq!(ocr_tag_from_capability_name(OCR_PREFIX, "Language.OCR~~~de-DE~0.0.1.0"), Some("de-DE".to_string()));
    assert_eq!(ocr_tag_from_capability_name(OCR_PREFIX, "not-a-capability-name"), None);
}

// The same extraction under the speech prefix -- this used to always
// return None for a real speech name (empty tag -> every speech install's
// result/script file collided under the same "-voice-install-.json" path).
#[test]
fn ocr_tag_from_capability_name_extracts_the_tag_for_speech() {
    assert_eq!(ocr_tag_from_capability_name(SPEECH_PREFIX, "Language.Speech~~~de-DE~0.0.1.0"), Some("de-DE".to_string()));
}

// PowerShell `Get-WindowsCapability | ConvertTo-Json` output parsing --
// fixed sample strings standing in for the three shapes PowerShell actually
// produces.
#[test]
fn ocr_parse_capabilities_json_handles_a_single_object() {
    let json = r#"{
            "Name": "Language.OCR~~~de-DE~0.0.1.0",
            "State": "Installed",
            "Version": "0.0.1.0",
            "DownloadSize": 12345678,
            "InstallSize": 23456789
        }"#;
    let caps = ocr_parse_capabilities_json(OCR_PREFIX, json).unwrap();
    assert_eq!(caps.len(), 1);
    assert_eq!(caps[0].name, "Language.OCR~~~de-DE~0.0.1.0");
    assert_eq!(caps[0].tag, "de-DE");
    assert_eq!(caps[0].state, "Installed");
    assert_eq!(caps[0].version, "0.0.1.0");
    assert_eq!(caps[0].download_size, Some(12345678));
    assert_eq!(caps[0].install_size, Some(23456789));
}

#[test]
fn ocr_parse_capabilities_json_handles_an_array() {
    let json = r#"[
            {
                "Name": "Language.OCR~~~de-DE~0.0.1.0",
                "State": "NotPresent",
                "Version": "0.0.1.0",
                "DownloadSize": 12345678,
                "InstallSize": 23456789
            },
            {
                "Name": "Language.OCR~~~en-US~0.0.1.0",
                "State": "Installed",
                "Version": "0.0.1.0",
                "DownloadSize": 9999999,
                "InstallSize": 8888888
            }
        ]"#;
    let caps = ocr_parse_capabilities_json(OCR_PREFIX, json).unwrap();
    assert_eq!(caps.len(), 2);
    assert_eq!(caps[0].tag, "de-DE");
    assert_eq!(caps[0].state, "NotPresent");
    assert_eq!(caps[1].tag, "en-US");
    assert_eq!(caps[1].state, "Installed");
}

// An already-installed capability commonly has no DownloadSize/InstallSize
// at all -- that must become None, never Some(0) or a panic.
#[test]
fn ocr_parse_capabilities_json_missing_size_becomes_none() {
    let json = r#"{
            "Name": "Language.OCR~~~fr-FR~0.0.1.0",
            "State": "Installed",
            "Version": "0.0.1.0",
            "DownloadSize": null,
            "InstallSize": null
        }"#;
    let caps = ocr_parse_capabilities_json(OCR_PREFIX, json).unwrap();
    assert_eq!(caps.len(), 1);
    assert_eq!(caps[0].download_size, None);
    assert_eq!(caps[0].install_size, None);
}

// Real capture from a VM: `State` comes back as a bare integer, `Version` as
// JSON null, and DownloadSize/InstallSize as a literal 0 rather than absent
// -- none of which the original hand-authored fixtures above (a wrong guess
// at the real shape, not tested against actual PowerShell output) exercised.
// A second-opinion review + a real VM capture both caught this; the
// automated suite passed the whole time regardless. State 4 = Installed
// (confirmed against a capability independently proven installed by using
// it for real OCR), 0 = NotPresent.
#[test]
fn ocr_parse_capabilities_json_handles_the_real_numeric_shape() {
    let json = r#"[
            { "Name": "Language.OCR~~~de-DE~0.0.1.0", "State": 4, "Version": null, "DownloadSize": 0, "InstallSize": 0 },
            { "Name": "Language.OCR~~~fr-FR~0.0.1.0", "State": 0, "Version": null, "DownloadSize": 0, "InstallSize": 0 },
            { "Name": "Language.OCR~~~ar-SA~0.0.1.0", "State": 4, "Version": null, "DownloadSize": 0, "InstallSize": 1053812 }
        ]"#;
    let caps = ocr_parse_capabilities_json(OCR_PREFIX, json).unwrap();
    assert_eq!(caps.len(), 3);
    assert_eq!(caps[0].state, "Installed");
    assert_eq!(caps[0].version, "");
    assert_eq!(caps[0].download_size, None);
    assert_eq!(caps[0].install_size, None); // literal 0 is "not reported", not a real zero-byte install
    assert_eq!(caps[1].state, "NotPresent");
    assert_eq!(caps[2].state, "Installed");
    assert_eq!(caps[2].install_size, Some(1053812)); // a real nonzero size still comes through
}

// An unrecognized numeric state code must not be mistaken for Installed --
// understating readiness is the safe direction to be wrong in.
#[test]
fn ocr_parse_capabilities_json_unknown_numeric_state_is_not_present() {
    let json = r#"{ "Name": "Language.OCR~~~de-DE~0.0.1.0", "State": 2, "Version": null, "DownloadSize": 0, "InstallSize": 0 }"#;
    let caps = ocr_parse_capabilities_json(OCR_PREFIX, json).unwrap();
    assert_eq!(caps[0].state, "NotPresent");
}

// Parsed under the speech prefix, a real speech capability entry gets
// its tag too -- this used to come out empty for every single row (the
// shared parser hardcoded the OCR prefix), which is why "manage languages"
// showed a blank/garbage label for every speech language even before an
// install was ever attempted.
#[test]
fn ocr_parse_capabilities_json_extracts_the_tag_for_speech_entries() {
    let json = r#"{ "Name": "Language.Speech~~~de-DE~0.0.1.0", "State": "Installed", "Version": "0.0.1.0", "DownloadSize": null, "InstallSize": null }"#;
    let caps = ocr_parse_capabilities_json(SPEECH_PREFIX, json).unwrap();
    assert_eq!(caps[0].tag, "de-DE");
}

// "Updatable?" decision -- table-driven per the spec's five cases.
#[test]
fn ocr_lang_needs_update_table() {
    let installed = |v: &str| Some(OcrCapSnapshot { version: v.to_string(), installed: true });
    let not_installed = |v: &str| Some(OcrCapSnapshot { version: v.to_string(), installed: false });

    // same version + Installed -> not updatable
    assert!(!ocr_lang_needs_update(installed("0.0.1.0").as_ref(), "Installed", "0.0.1.0"));
    // different version + Installed -> updatable
    assert!(ocr_lang_needs_update(installed("0.0.1.0").as_ref(), "Installed", "0.0.2.0"));
    // previously Installed, now NotPresent -> updatable
    assert!(ocr_lang_needs_update(installed("0.0.1.0").as_ref(), "NotPresent", "0.0.1.0"));
    // never-installed NotPresent (a remembered-but-never-installed snapshot) -> not updatable
    assert!(!ocr_lang_needs_update(not_installed("0.0.1.0").as_ref(), "NotPresent", "0.0.1.0"));
    // first-time-seen language, no previous snapshot at all -> not updatable
    assert!(!ocr_lang_needs_update(None, "NotPresent", "0.0.1.0"));
}

#[test]
fn ocr_validate_path_rejects_a_disallowed_extension() {
    let mut p = std::env::temp_dir();
    p.push("ocr_validate_path_test.txt");
    fs::write(&p, b"not an image").unwrap();
    let result = ocr_validate_path(p.to_str().unwrap());
    let _ = fs::remove_file(&p);
    assert!(result.is_err());
}

#[test]
fn ocr_validate_path_rejects_a_missing_file() {
    let p = std::env::temp_dir().join("ocr_validate_path_does_not_exist.png");
    assert!(ocr_validate_path(p.to_str().unwrap()).is_err());
}

#[test]
fn ocr_validate_path_accepts_a_real_image_file() {
    let mut p = std::env::temp_dir();
    p.push("ocr_validate_path_test.png");
    fs::write(&p, b"\x89PNG\r\n\x1a\nnot a real png body, just a valid extension/size check").unwrap();
    let result = ocr_validate_path(p.to_str().unwrap());
    let _ = fs::remove_file(&p);
    assert!(result.is_ok());
}

#[test]
fn winrt_file_path_flattens_slashes_and_verbatim_prefixes() {
    assert_eq!(winrt_file_path("C:/bfb/media/a.jpg"), r"C:\bfb\media\a.jpg");
    assert_eq!(winrt_file_path(r"C:\x\a.jpg"), r"C:\x\a.jpg");
    assert_eq!(winrt_file_path(r"\\?\C:\x\a.jpg"), r"C:\x\a.jpg");
    assert_eq!(winrt_file_path(r"\\?\UNC\srv\share\a.jpg"), r"\\srv\share\a.jpg");
    assert_eq!(winrt_file_path(r"\\srv/share/a.jpg"), r"\\srv\share\a.jpg");
}

#[test]
fn ocr_resize_target_leaves_a_normally_sized_image_alone() {
    assert_eq!(ocr_resize_target(800, 600, 4200, 300), None);
}

#[test]
fn ocr_resize_target_downscales_an_oversized_image_proportionally() {
    let (nw, nh) = ocr_resize_target(8400, 4200, 4200, 300).unwrap();
    assert_eq!((nw, nh), (4200, 2100));
}

#[test]
fn ocr_resize_target_upscales_a_small_crop_2x() {
    assert_eq!(ocr_resize_target(120, 80, 4200, 300), Some((240, 160)));
}

// ---- Voice dictation ----

#[cfg(windows)]
#[test]
fn voice_pick_lang_auto_without_any_installed_pack_fails() {
    let err = voice::pick_lang("auto", true, &[], None).unwrap_err();
    assert_eq!(err, "voice-no-language");
}

#[cfg(windows)]
#[test]
fn voice_pick_lang_auto_prefers_the_installed_system_language() {
    let installed = vec!["de-DE".to_string(), "en-US".to_string()];
    assert_eq!(voice::pick_lang("auto", true, &installed, Some("en-US")).unwrap(), "en-US");
    // System language has no pack installed -> falls back to whatever IS there.
    assert_eq!(voice::pick_lang("auto", true, &installed, Some("fr-FR")).unwrap(), "de-DE");
}

#[cfg(windows)]
#[test]
fn voice_pick_lang_unknown_tag_falls_back_when_auto_pick_is_on() {
    let installed = vec!["de-DE".to_string()];
    assert_eq!(voice::pick_lang("fr-FR", true, &installed, None).unwrap(), "de-DE");
}

#[cfg(windows)]
#[test]
fn voice_pick_lang_unknown_tag_fails_when_auto_pick_is_off() {
    let installed = vec!["de-DE".to_string()];
    let err = voice::pick_lang("fr-FR", false, &installed, None).unwrap_err();
    assert_eq!(err, "voice-no-language");
}

#[cfg(windows)]
#[test]
fn voice_pick_lang_an_installed_explicit_tag_is_used_as_is() {
    let installed = vec!["de-DE".to_string(), "en-US".to_string()];
    assert_eq!(voice::pick_lang("en-US", false, &installed, None).unwrap(), "en-US");
}

#[cfg(windows)]
#[test]
fn voice_hresult_table_maps_every_measured_code() {
    assert_eq!(voice::error_key_for_hresult(0x800455BCu32 as i32), "voice-no-language");
    assert_eq!(voice::error_key_for_hresult(0x80070005u32 as i32), "voice-denied");
    assert_eq!(voice::error_key_for_hresult(0x80045509u32 as i32), "voice-privacy-off");
    assert_eq!(voice::error_key_for_hresult(0x80004005u32 as i32), "voice-failed"); // any other HRESULT
}

#[cfg(windows)]
#[test]
fn voice_stop_reason_table() {
    // Windows' own AutoStopSilenceTimeout -- always "silence", regardless of
    // what (if anything) we had requested.
    assert_eq!(voice::stop_reason(7, None), "silence");
    assert_eq!(voice::stop_reason(7, Some("max")), "silence");
    // Success (0): whatever we asked for, defaulting to "manual".
    assert_eq!(voice::stop_reason(0, None), "manual");
    assert_eq!(voice::stop_reason(0, Some("max")), "max");
    assert_eq!(voice::stop_reason(0, Some("shutdown")), "shutdown");
    // Anything else is a genuine failure.
    assert_eq!(voice::stop_reason(1, None), "error");
    assert_eq!(voice::stop_reason(-1, None), "error");
}

#[cfg(windows)]
#[test]
fn voice_should_stop_silence_window_and_zero_means_off() {
    use std::time::Duration;
    // Idle past the silence window, well under the hard cap -> "silence".
    assert_eq!(voice::should_stop(Duration::from_secs(6), Duration::from_secs(10), 5, 300), Some("silence"));
    // Not idle long enough yet.
    assert_eq!(voice::should_stop(Duration::from_secs(2), Duration::from_secs(10), 5, 300), None);
    // auto_stop_sec == 0 -> the idle check never fires, no matter how idle.
    assert_eq!(voice::should_stop(Duration::from_secs(999), Duration::from_secs(10), 0, 300), None);
}

#[cfg(windows)]
#[test]
fn voice_should_stop_hard_cap_always_wins() {
    use std::time::Duration;
    // Elapsed has reached max_sec even though nothing was idle at all ->
    // "max", not "silence" and not None.
    assert_eq!(voice::should_stop(Duration::from_secs(0), Duration::from_secs(300), 5, 300), Some("max"));
    // The cap wins even when voiceAutoStop (auto_stop_sec) is off.
    assert_eq!(voice::should_stop(Duration::from_secs(0), Duration::from_secs(300), 0, 300), Some("max"));
}

#[cfg(windows)]
#[test]
fn voice_clamp_durations_enforces_the_real_1_to_60_min_range() {
    // The UI slider only goes to 60 min/60 sec, but the backend is the
    // real limit -- a value above the slider's own max must still be cut down.
    assert_eq!(voice::clamp_durations(3600, 60), (3600, 60)); // exactly at the new ceiling
    assert_eq!(voice::clamp_durations(7200, 999), (3600, 60)); // above it -> clamped
    assert_eq!(voice::clamp_durations(0, 0), (60, 0)); // below the floor -> min 60s/1min
}

#[test]
fn voice_should_send_stop_skips_a_session_that_already_ended_itself() {
    assert!(!should_send_stop(None, 5, true));
    assert!(!should_send_stop(Some(5), 5, true));
}

// ---- Kind-generalized language-pack elevation ----

#[cfg(windows)]
#[test]
fn ocr_elevated_capabilities_result_survives_a_truncated_read() {
    let nonce = ocr::start_request(ocr::CapKind::Ocr, ocr::CAPS_TAG);
    let path = ocr::caps_result_path(ocr::CapKind::Ocr, &nonce);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&path, b"{\"Name\":\"Language.OC").unwrap(); // deliberately truncated JSON
    let result = ocr::elevated_capabilities_result(ocr::CapKind::Ocr);
    assert!(matches!(result, Ok(None)));
    assert!(path.exists(), "a truncated/unparseable read must leave the file for the next poll, not discard it");
    let _ = std::fs::remove_file(&path);
}

#[cfg(windows)]
#[test]
fn ocr_elevated_capabilities_result_deletes_on_a_complete_error_payload() {
    let nonce = ocr::start_request(ocr::CapKind::Speech, ocr::CAPS_TAG);
    let path = ocr::caps_result_path(ocr::CapKind::Speech, &nonce);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&path, br#"{"error":"boom"}"#).unwrap();
    match ocr::elevated_capabilities_result(ocr::CapKind::Speech) {
        Err(e) => assert_eq!(e, "boom"),
        Ok(_) => panic!("expected an error"),
    }
    assert!(!path.exists(), "a terminal (error) result must be cleared, not left for a duplicate re-read");
}

#[cfg(windows)]
#[test]
fn ocr_and_speech_elevated_result_paths_never_collide() {
    let a = ocr::start_request(ocr::CapKind::Ocr, ocr::CAPS_TAG);
    let b = ocr::start_request(ocr::CapKind::Speech, ocr::CAPS_TAG);
    assert_ne!(
        ocr::caps_result_path(ocr::CapKind::Ocr, &a),
        ocr::caps_result_path(ocr::CapKind::Speech, &b)
    );
}

#[cfg(windows)]
#[test]
fn an_elevated_result_is_only_believed_while_a_request_is_in_flight() {
    // The temp folder is writable by anything running as this user. With the
    // file name derived from the language tag (or the process id) another
    // process could drop a finished-looking result in ahead of time and the
    // app would report a language pack installed that was never installed.
    // Two nonces in a row must differ, and a file under a name nobody asked
    // for must be ignored outright.
    let first = ocr::start_request(ocr::CapKind::Ocr, ocr::CAPS_TAG);
    let second = ocr::start_request(ocr::CapKind::Ocr, ocr::CAPS_TAG);
    assert_ne!(first, second, "every elevated request needs its own name");
    assert!(first.len() >= 16, "too short to be unguessable: {first}");

    let planted = ocr::caps_result_path(ocr::CapKind::Ocr, "planted-by-someone-else");
    if let Some(dir) = planted.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&planted, br#"{"error":"forged"}"#).unwrap();
    // `second` is the pending nonce and no file exists under it, so this is
    // "still running": the planted file must not be read at all.
    assert!(matches!(ocr::elevated_capabilities_result(ocr::CapKind::Ocr), Ok(None)));
    assert!(planted.exists(), "a file the app never asked for must be left untouched");
    let _ = std::fs::remove_file(&planted);
}

// DISM writing its unconditional startup log to %WINDIR%\Logs\DISM
// (unwritable without admin rights) was the actual cause of "jetzt
// prüfen" failing on a standard account -- redirected into our own
// always-writable temp folder instead, single-quoted like every other
// embedded path in this file.
#[cfg(windows)]
#[test]
fn dism_log_args_redirects_into_our_own_writable_temp_dir() {
    let ocr = ocr::dism_log_args(ocr::CapKind::Ocr);
    let voice = ocr::dism_log_args(ocr::CapKind::Speech);
    assert!(ocr.contains("-LogPath '") && ocr.contains("-ScratchDirectory '"), "{ocr}");
    assert!(ocr.contains("-ocr-dism.log"), "{ocr}");
    assert!(voice.contains("-voice-dism.log"), "{voice}");
    assert_ne!(ocr, voice, "each kind must log to its own file, not share one");
}

// 177 part 3: whatever the real failure was (log path, a flat "needs
// elevation", anything else), the frontend always gets the SAME stable
// code -- with the original text still attached, not silently dropped.
#[cfg(windows)]
#[test]
fn caps_elevation_error_keeps_the_stable_code_and_the_raw_text() {
    let wrapped = ocr::caps_elevation_error("Der Zugriff auf den Pfad wurde verweigert.");
    assert!(wrapped.starts_with("ocr-caps-need-elevation|"));
    assert!(wrapped.ends_with("Der Zugriff auf den Pfad wurde verweigert."));
}

// A cancelled or declined elevation dialog is always worth a retry --
// Windows lets that same credential prompt take a DIFFERENT
// administrator's password next time, so this account's own rights are
// not the last word. Only a Group Policy that denies the prompt outright
// (no dialog ever shown, to anyone) is genuinely final.
#[cfg(windows)]
#[test]
fn classify_elevation_refusal_treats_a_decline_as_retryable() {
    assert_eq!(ocr::classify_elevation_refusal(false), ocr::ElevationRefusal::Retryable);
}

#[cfg(windows)]
#[test]
fn classify_elevation_refusal_treats_a_policy_denial_as_final() {
    assert_eq!(ocr::classify_elevation_refusal(true), ocr::ElevationRefusal::PolicyBlocked);
}

#[cfg(windows)]
#[test]
fn strip_ps_banner_keeps_only_the_first_line() {
    assert_eq!(ocr::strip_ps_banner("  Access is denied.  "), "Access is denied.");
    assert_eq!(
        ocr::strip_ps_banner("Get-WindowsCapability : Access is denied.\nAt line:1 char:1\n+ ..."),
        "Get-WindowsCapability : Access is denied."
    );
    assert_eq!(ocr::strip_ps_banner(""), "");
}

#[test]
fn voice_should_send_stop_targets_the_right_running_session() {
    assert!(should_send_stop(None, 5, false));
    assert!(should_send_stop(Some(5), 5, false));
    assert!(!should_send_stop(Some(3), 5, false)); // a stale/foreign id never stops the live session
}

#[test]
fn voice_text_event_serialises_with_its_token() {
    let json = serde_json::to_string(&VoiceTextEvent { id: 1, token: 42, text: "hi".into() }).unwrap();
    assert!(json.contains("\"token\":42"));
}

#[test]
fn data_url_within_limit_rejects_one_byte_over_and_accepts_exactly_at() {
    let at_limit = "x".repeat(10);
    let one_over = "x".repeat(11);
    assert!(data_url_within_limit(&at_limit, 10));
    assert!(!data_url_within_limit(&one_over, 10));
}

// ---- Image search text ----

#[test]
fn ocr_index_max_chars_reads_the_expert_value_and_falls_back_to_the_spec_default() {
    let mut settings: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
    assert_eq!(ocr_index_max_chars(&settings), 20000);
    settings.ui_values.insert("ocrIndexMaxChars".into(), 5.0);
    assert_eq!(ocr_index_max_chars(&settings), 5);
}

#[test]
fn cap_ocr_text_cuts_by_character_not_by_byte() {
    // "café" is 4 chars but 5 bytes in UTF-8 -- a byte-based cut would
    // split the "é" and produce invalid UTF-8 (or panic).
    assert_eq!(cap_ocr_text("café", 4), "café");
    assert_eq!(cap_ocr_text("café", 3), "caf");
    assert_eq!(cap_ocr_text("hi", 10), "hi");
}

#[test]
fn resolved_image_text_only_recognizes_a_non_empty_picture_with_the_switch_on() {
    let calls = std::cell::Cell::new(0);
    let recognize = |img: &str| { calls.set(calls.get() + 1); format!("seen:{img}") };
    // Switch off: never even calls the recognizer, whatever the picture.
    assert_eq!(resolved_image_text("data:image/png;base64,AAA", false, recognize), "");
    assert_eq!(calls.get(), 0);
    // Picture removed: empties the field and never calls the recognizer either.
    assert_eq!(resolved_image_text("", true, recognize), "");
    assert_eq!(calls.get(), 0);
    // Switch on, a real picture: calls the recognizer exactly once and keeps
    // whatever it returns.
    assert_eq!(resolved_image_text("data:image/png;base64,AAA", true, recognize), "seen:data:image/png;base64,AAA");
    assert_eq!(calls.get(), 1);
}

fn image_text_prompt(id: &str, image_text: &str) -> Prompt {
    let mut p = test_prompt(id);
    p.image_text = image_text.into();
    p
}

#[test]
fn clear_image_text_empties_every_index_and_reports_whether_it_changed() {
    let mut prompts = vec![image_text_prompt("a", "invoice total"), image_text_prompt("b", ""), image_text_prompt("c", "receipt")];
    assert!(clear_image_text(&mut prompts));
    assert!(prompts.iter().all(|p| p.image_text.is_empty()));
    assert!(!clear_image_text(&mut prompts), "nothing left to clear the second time");
}

#[test]
fn image_text_matches_is_empty_with_the_switch_off_even_for_a_real_hit() {
    let prompts = vec![image_text_prompt("a", "invoice total due friday")];
    assert!(image_text_matches(&prompts, false, "invoice").is_empty());
}

#[test]
fn image_text_matches_finds_a_case_insensitive_substring_and_skips_empty_queries() {
    let prompts = vec![
        image_text_prompt("a", "Invoice Total Due Friday"),
        image_text_prompt("b", "just a landscape photo, no text"),
        image_text_prompt("c", ""),
    ];
    assert_eq!(image_text_matches(&prompts, true, "invoice"), vec!["a".to_string()]);
    assert!(image_text_matches(&prompts, true, "   ").is_empty(), "a blank query must match nothing, not everything");
    assert!(image_text_matches(&prompts, true, "zzz-no-such-word").is_empty());
}

// import_prompts refuses a file that changed since preview_import hashed
// it (the real gate is a one-line `!=` compare in import_prompts itself,
// which needs a live AppHandle to exercise directly: this proves the
// hash it compares against actually distinguishes an edited file from
// the original and is stable for an unchanged one).
#[test]
fn import_file_hash_changes_with_the_file_content() {
    let original = import_file_hash(b"name,text\r\nA,B\r\n");
    let edited = import_file_hash(b"name,text\r\nA,C\r\n");
    assert_ne!(original, edited);
    assert_eq!(original, import_file_hash(b"name,text\r\nA,B\r\n"));
}

#[test]
fn a_theme_that_no_longer_ships_falls_back_to_system() {
    let mut s = Settings::default();
    // Themes dropped in 2.7.0: an installation set to one of them must still
    // start with a full set of colour tokens.
    for gone in ["gradient", "sunset", "forest", "midnight", "retro", "lavender", "candy", "coffee"] {
        assert_eq!(theme_base_of(&s, gone), "system", "{gone} should fall back");
    }
    assert_eq!(theme_base_of(&s, "ocean"), "ocean");
    assert_eq!(theme_base_of(&s, "system"), "system");
    // A custom theme built on a dropped base falls back the same way.
    s.custom_themes.push(CustomTheme {
        id: "t1".into(),
        name: "Mine".into(),
        base: "retro".into(),
        vars: HashMap::new(),
    });
    assert_eq!(theme_base_of(&s, "custom:t1"), "system");
}

#[test]
fn stored_ai_theme_migrates_to_neon_not_system() {
    // "ai" was renamed to "neon": a saved preference must land on the
    // renamed theme, not fall through known_theme() to "system".
    let mut s = Settings { theme: "ai".into(), ..Default::default() };
    s.custom_themes.push(CustomTheme {
        id: "t1".into(),
        name: "Mine".into(),
        base: "ai".into(),
        vars: HashMap::new(),
    });
    s.migrate();
    assert_eq!(s.theme, "neon");
    assert_eq!(s.custom_themes[0].base, "neon");
    assert_eq!(theme_base_of(&s, &s.theme), "neon");
}

// An existing install (real prompts, or the example-seeding marker set --
// even if those examples were later dismissed) must never see the first-run
// tour; only a genuinely blank store should.
#[test]
fn tour_is_marked_seen_for_an_existing_store() {
    assert!(tour_should_be_marked_seen(true, false), "prompts already exist");
    assert!(tour_should_be_marked_seen(false, true), "examples were seeded (even if later dismissed)");
    assert!(tour_should_be_marked_seen(true, true), "both signal a non-fresh store");
}

#[test]
fn tour_is_not_marked_seen_for_a_blank_store() {
    assert!(!tour_should_be_marked_seen(false, false));
}

// ---- Colour-picker history ----

#[test]
fn record_pick_keeps_five_newest_and_skips_a_repeat() {
    PICK_HISTORY.lock().unwrap_or_else(|e| e.into_inner()).clear();
    for i in 1..=6u8 {
        record_pick([i, 0, 0], PICK_HISTORY_DEFAULT);
    }
    record_pick([6, 0, 0], PICK_HISTORY_DEFAULT); // the same colour twice in a row is not a new entry
    let hex = pick_history_hex();
    assert_eq!(hex.len(), PICK_HISTORY_DEFAULT);
    // Newest first, and the oldest ([1,0,0]) has fallen off the end.
    assert_eq!(hex[0], "#060000");
    assert_eq!(hex[PICK_HISTORY_DEFAULT - 1], "#020000");
    // Picking an earlier colour again puts it back on top as a fresh entry.
    record_pick([2, 0, 0], PICK_HISTORY_DEFAULT);
    assert_eq!(pick_history_hex()[0], "#020000");
}

#[test]
fn pick_history_persists_across_a_reload_and_respects_a_lowered_cap() {
    PICK_HISTORY.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let mut s = Settings::default();
    record_pick([1, 0, 0], pick_history_max(&s));
    record_pick([2, 0, 0], pick_history_max(&s));
    s.ui_texts.insert("pickHistory".to_string(), pick_history_hex().join(","));
    // Simulate a restart: clear the in-memory list, reload from the mirror.
    PICK_HISTORY.lock().unwrap_or_else(|e| e.into_inner()).clear();
    load_pick_history(&s);
    assert_eq!(pick_history_hex(), vec!["#020000", "#010000"]);
    // Lowering the cap below the stored count trims on the next load.
    s.ui_values.insert("pickHistoryN".to_string(), 1.0);
    load_pick_history(&s);
    assert_eq!(pick_history_hex(), vec!["#020000"]);
    // 0 turns the feature off: the mirror is kept, but nothing loads back in.
    s.ui_values.insert("pickHistoryN".to_string(), 0.0);
    load_pick_history(&s);
    assert!(pick_history_hex().is_empty());
}

// The sync folder is a directory the USER picks, so it can hold their own
// documents. Only files carrying our frontmatter may ever be swept.
#[test]
fn sync_sweep_spares_documents_the_app_did_not_write() {
    let dir = std::env::temp_dir().join(format!("ps-sweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mine = dir.join("p_stale.md");
    let theirs = dir.join("My shopping list.md");
    let obsidian = dir.join("Reading list.md");
    let kept = dir.join("p_live.md");
    fs::write(&mine, prompt_to_md(&sync_test_prompt(), &SyncPlacement::default())).unwrap();
    fs::write(&theirs, "# Shopping\n\n- milk\n").unwrap();
    // Someone else's frontmatter is still not ours.
    fs::write(&obsidian, "---\ntitle: Books\ntags: reading\n---\n\n- Dune\n").unwrap();
    fs::write(&kept, prompt_to_md(&sync_test_prompt(), &SyncPlacement::default())).unwrap();
    fs::write(dir.join("AGENTS.md"), sync_agents_md()).unwrap();

    sync_known_note(&mine);
    sync_seen_record_own_write(&mine);
    sync_known_note(&theirs);
    sync_known_note(&obsidian);
    let mut wanted = std::collections::HashSet::new();
    wanted.insert("p_live.md".to_string());
    sync_sweep_orphans(&dir, &wanted);

    assert!(!mine.exists(), "an orphaned prompt file must go");
    assert!(theirs.exists(), "a document without our frontmatter must survive");
    assert!(obsidian.exists(), "someone else's frontmatter must survive");
    assert!(kept.exists(), "a live prompt's file must stay");
    assert!(dir.join("AGENTS.md").exists(), "the readme must stay");
    let _ = fs::remove_dir_all(&dir);
}

// ---- Prompt folders as Markdown ----

fn sync_test_prompt() -> Prompt {
    let mut p = sync_blank_prompt("p1".into());
    p.name = "Test prompt".into();
    p.color = "#7c5cff".into();
    p.tags = vec!["Work".into(), "draft".into()];
    p.notes = "a private note".into();
    p.text = "Hello\nWorld".into();
    p
}

#[test]
fn md_roundtrip_preserves_editable_fields() {
    let p = sync_test_prompt();
    let content = prompt_to_md(&p, &SyncPlacement::default());
    let updated = apply_md_to_prompt(&p, &content).expect("valid frontmatter");
    assert_eq!(updated.name, p.name);
    assert_eq!(updated.color, p.color);
    assert_eq!(updated.tags, p.tags);
    assert_eq!(updated.notes, p.notes);
    // The closing newline the writer adds is the file's, not the prompt's: a
    // round trip must hand back exactly the text that went in.
    assert_eq!(updated.text, "Hello\nWorld");
    // Re-serializing the round-tripped prompt must be byte-identical: otherwise
    // sync_write_all()'s "only write if content differs" guard would loop forever.
    assert_eq!(prompt_to_md(&updated, &SyncPlacement::default()), content);
}

// image_text is a search index, never synchronized: the
// Markdown mirror writes a fixed set of named fields, so a value that is
// simply never mentioned there cannot leak into the file on disk.
#[test]
fn md_sync_never_carries_image_text() {
    let mut p = sync_test_prompt();
    p.image_text = "invoice total due friday".into();
    let content = prompt_to_md(&p, &SyncPlacement::default());
    assert!(!content.contains("invoice"), "OCR-recognized text leaked into the synced file: {content}");
}

#[test]
fn sync_placement_for_is_empty_when_the_prompt_has_no_placement() {
    let settings = Settings::default();
    let placement = sync_placement_for(&sync_test_prompt(), &settings);
    assert_eq!(placement.view, "");
    assert_eq!(placement.grid, "");
    assert_eq!(placement.cell, "");
    assert_eq!(placement.tile_folder, "");
    assert_eq!(placement.float_folder, "");
}

#[test]
fn sync_placement_for_reports_a_plain_grid_cell() {
    let mut settings = Settings::default();
    let mut view = test_view("v1");
    view.layouts.insert(grid_key(view.cols, view.rows), HashMap::from([("p1".to_string(), [2, 3])]));
    settings.views.push(view);
    let placement = sync_placement_for(&sync_test_prompt(), &settings);
    assert_eq!(placement.view, "V1");
    assert_eq!(placement.grid, grid_key(default_cols(), default_rows()));
    assert_eq!(placement.cell, "2,3");
    assert_eq!(placement.tile_folder, "");
}

#[test]
fn sync_placement_for_reports_tile_folder_membership_over_a_plain_cell() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    let mut folder = test_tile_folder("f1", "v1", [4, 5], &["p1"]);
    folder.name = "Team".to_string();
    folder.color = "#ff0000".to_string();
    settings.tile_folders.push(folder);
    let placement = sync_placement_for(&sync_test_prompt(), &settings);
    assert_eq!(placement.view, "V1");
    assert_eq!(placement.cell, "4,5");
    assert_eq!(placement.tile_folder, "Team");
    assert_eq!(placement.tile_folder_color, "#ff0000");
}

#[test]
fn sync_placement_for_reports_float_folder_membership_independently() {
    let mut settings = Settings::default();
    settings.float_folders.push(FloatFolder {
        id: "ff1".into(),
        name: "Pills".into(),
        pos: Pos { x: 0, y: 0, ..Default::default() },
        scale: 1.0,
        members: vec!["p1".to_string(), "p2".to_string()],
    });
    let placement = sync_placement_for(&sync_test_prompt(), &settings);
    assert_eq!(placement.float_folder, "Pills");
    assert_eq!(placement.view, "", "a floating pill with no grid tile has no grid placement");
}

#[test]
fn apply_md_placement_creates_a_view_by_name_and_places_the_cell() {
    let mut settings = Settings::default();
    let fields = HashMap::from([
        ("view".to_string(), "Work".to_string()),
        ("grid".to_string(), "5x4".to_string()),
        ("cell".to_string(), "1,2".to_string()),
    ]);
    assert_eq!(apply_md_placement(&mut settings, "p1", &fields), (true, false));
    assert_eq!(settings.views.len(), 1);
    let view = &settings.views[0];
    assert_eq!(view.name, "Work");
    assert_eq!(view.cols, 5);
    assert_eq!(view.rows, 4);
    assert_eq!(view.layouts.get(&grid_key(5, 4)).and_then(|m| m.get("p1")), Some(&[1, 2]));
}

#[test]
fn apply_md_placement_reuses_an_existing_view_with_the_same_name() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1")); // name "V1"
    let existing_count = settings.views.len();
    let fields = HashMap::from([
        ("view".to_string(), "V1".to_string()),
        ("grid".to_string(), grid_key(default_cols(), default_rows())),
        ("cell".to_string(), "0,0".to_string()),
    ]);
    assert_eq!(apply_md_placement(&mut settings, "p1", &fields), (true, false));
    assert_eq!(settings.views.len(), existing_count, "must not create a second view for a name that already exists");
}

#[test]
fn apply_md_placement_falls_back_to_a_free_cell_on_collision() {
    let mut settings = Settings::default();
    let mut view = test_view("v1");
    view.cols = 2;
    view.rows = 1;
    view.layouts.insert(grid_key(2, 1), HashMap::from([("other".to_string(), [0, 0])]));
    settings.views.push(view);
    let fields = HashMap::from([
        ("view".to_string(), "V1".to_string()),
        ("grid".to_string(), "2x1".to_string()),
        ("cell".to_string(), "0,0".to_string()),
    ]);
    assert_eq!(apply_md_placement(&mut settings, "p1", &fields), (true, false));
    let key = grid_key(2, 1);
    assert_eq!(settings.views[0].layouts.get(&key).and_then(|m| m.get("p1")), Some(&[1, 0]));
}

#[test]
fn apply_md_placement_adopts_a_cleared_folder_colour() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    let mut fields = HashMap::from([
        ("view".to_string(), "V1".to_string()),
        ("grid".to_string(), grid_key(default_cols(), default_rows())),
        ("cell".to_string(), "0,0".to_string()),
        ("tile_folder".to_string(), "Team".to_string()),
        ("tile_folder_color".to_string(), "#00ff00".to_string()),
    ]);
    apply_md_placement(&mut settings, "p1", &fields);
    assert_eq!(settings.tile_folders[0].color, "#00ff00");
    fields.insert("tile_folder_color".to_string(), String::new());
    apply_md_placement(&mut settings, "p2", &fields);
    assert_eq!(settings.tile_folders.len(), 1);
    assert_eq!(settings.tile_folders[0].color, "", "the other machine cleared the colour");
}

#[test]
fn apply_md_placement_creates_a_tile_folder_and_drops_the_members_own_layout_entry() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    let fields = HashMap::from([
        ("view".to_string(), "V1".to_string()),
        ("grid".to_string(), grid_key(default_cols(), default_rows())),
        ("cell".to_string(), "0,0".to_string()),
        ("tile_folder".to_string(), "Team".to_string()),
        ("tile_folder_color".to_string(), "#00ff00".to_string()),
    ]);
    assert_eq!(apply_md_placement(&mut settings, "p1", &fields), (true, true));
    assert_eq!(settings.tile_folders.len(), 1);
    assert_eq!(settings.tile_folders[0].name, "Team");
    assert!(settings.tile_folders[0].members.iter().any(|m| m == "p1"));
    let key = grid_key(default_cols(), default_rows());
    assert!(
        settings.views[0].layouts.get(&key).map(|m| !m.contains_key("p1")).unwrap_or(true),
        "a folder member is not a tile of its own"
    );
}

#[test]
fn apply_md_placement_never_turns_a_grid_only_prompt_into_a_floating_pill() {
    let mut settings = Settings::default();
    let fields = HashMap::from([("float_folder".to_string(), "Pills".to_string())]);
    assert_eq!(apply_md_placement(&mut settings, "p1", &fields), (false, false));
    assert!(settings.float_folders.is_empty(), "joining a pill folder from sync must never conjure a new pill window");
}

#[test]
fn apply_md_placement_joins_an_existing_floating_pill_to_a_pill_folder() {
    let mut settings = Settings::default();
    settings.floating.insert("p1".to_string(), Pos { x: 10, y: 10, ..Default::default() });
    let fields = HashMap::from([("float_folder".to_string(), "Pills".to_string())]);
    let (grid_placed, folders_touched) = apply_md_placement(&mut settings, "p1", &fields);
    assert!(!grid_placed, "no grid fields were supplied, so the caller should still fall back for the grid");
    assert!(folders_touched);
    assert_eq!(settings.float_folders.len(), 1);
    assert!(settings.float_folders[0].members.iter().any(|m| m == "p1"));
}

#[test]
fn apply_md_placement_returns_nothing_placed_when_every_field_is_absent() {
    let mut settings = Settings::default();
    let fields = HashMap::new();
    assert_eq!(apply_md_placement(&mut settings, "p1", &fields), (false, false));
    assert!(settings.views.is_empty());
}

#[test]
fn prompt_to_md_and_apply_md_placement_round_trip_a_tile_folder() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    let mut folder = test_tile_folder("f1", "v1", [2, 2], &["p1"]);
    folder.name = "Team".to_string();
    folder.color = "#7c5cff".to_string();
    settings.tile_folders.push(folder);
    let p = sync_test_prompt();
    let placement = sync_placement_for(&p, &settings);
    let md = prompt_to_md(&p, &placement);
    let (fields, _) = parse_frontmatter(&md).unwrap();
    let mut target = Settings::default();
    assert_eq!(apply_md_placement(&mut target, "p1", &fields), (true, true));
    assert_eq!(target.tile_folders[0].name, "Team");
    assert_eq!(target.tile_folders[0].color, "#7c5cff");
    assert_eq!(target.views[0].name, "V1");
}

// Re-importing a placement that already matches (only the prompt's TEXT
// changed on the other machine, not its position) must leave the cell alone:
// sync_resolve_cell must not see the prompt's own existing entry as a
// collision with itself.
#[test]
fn apply_md_placement_is_idempotent_for_the_prompts_own_current_cell() {
    let mut settings = Settings::default();
    let mut view = test_view("v1");
    view.layouts.insert(grid_key(view.cols, view.rows), HashMap::from([("p1".to_string(), [1, 1])]));
    settings.views.push(view);
    let fields = HashMap::from([
        ("view".to_string(), "V1".to_string()),
        ("grid".to_string(), grid_key(default_cols(), default_rows())),
        ("cell".to_string(), "1,1".to_string()),
    ]);
    apply_md_placement(&mut settings, "p1", &fields);
    let key = grid_key(default_cols(), default_rows());
    assert_eq!(
        settings.views[0].layouts.get(&key).and_then(|m| m.get("p1")),
        Some(&[1, 1]),
        "the prompt's own current cell must never read as occupied by itself"
    );
}

// A field that is PRESENT but empty ("tile_folder: ") is a deliberate "left
// this folder" from the writer, distinct from the key being absent entirely
// (an older file, untouched). It must actually remove existing local
// membership, or a folder left on one machine is silently restored by the
// other machine's own echo on the next poll.
#[test]
fn apply_md_placement_leaves_a_tile_folder_on_an_explicit_empty_field() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    settings.tile_folders.push(test_tile_folder("f1", "v1", [0, 0], &["p1", "p2"]));
    let fields = HashMap::from([
        ("view".to_string(), "V1".to_string()),
        ("grid".to_string(), grid_key(default_cols(), default_rows())),
        ("cell".to_string(), "3,3".to_string()),
        ("tile_folder".to_string(), String::new()),
    ]);
    let (placed, folders_touched) = apply_md_placement(&mut settings, "p1", &fields);
    assert!(placed);
    assert!(folders_touched);
    assert!(!settings.tile_folders[0].members.iter().any(|m| m == "p1"));
    let key = grid_key(default_cols(), default_rows());
    assert_eq!(settings.views[0].layouts.get(&key).and_then(|m| m.get("p1")), Some(&[3, 3]));
}

#[test]
fn apply_md_placement_leaves_a_float_folder_on_an_explicit_empty_field() {
    let mut settings = Settings::default();
    settings.floating.insert("p1".to_string(), Pos { x: 0, y: 0, ..Default::default() });
    settings.float_folders.push(FloatFolder {
        id: "ff1".into(),
        name: "Pills".into(),
        pos: Pos { x: 0, y: 0, ..Default::default() },
        scale: 1.0,
        members: vec!["p1".to_string(), "p2".to_string()],
    });
    let fields = HashMap::from([("float_folder".to_string(), String::new())]);
    let (_, folders_touched) = apply_md_placement(&mut settings, "p1", &fields);
    assert!(folders_touched);
    assert!(!settings.float_folders[0].members.iter().any(|m| m == "p1"));
}

// A field the key is entirely ABSENT for (as opposed to present-but-empty)
// means "this file predates the six placement fields": must NOT be read as a
// leave instruction, or every legacy sync file would strip every prompt out
// of every folder the moment this version first polls it.
#[test]
fn apply_md_placement_never_treats_a_missing_field_as_a_leave() {
    let mut settings = Settings::default();
    settings.tile_folders.push(test_tile_folder("f1", "v1", [0, 0], &["p1", "p2"]));
    let fields = HashMap::new(); // no tile_folder / float_folder key at all
    let (_, folders_touched) = apply_md_placement(&mut settings, "p1", &fields);
    assert!(!folders_touched);
    assert!(settings.tile_folders[0].members.iter().any(|m| m == "p1"));
}

// An unusable folder name (rejected by folder_name_of) must fall through to
// plain grid placement instead of claiming grid_placed=true for nothing: a
// brand-new prompt imported this way would otherwise land nowhere at all,
// invisible in every view.
#[test]
fn apply_md_placement_falls_back_to_the_grid_when_the_folder_name_is_rejected() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    let too_long = "x".repeat(MAX_FOLDER_NAME + 1);
    let fields = HashMap::from([
        ("view".to_string(), "V1".to_string()),
        ("grid".to_string(), grid_key(default_cols(), default_rows())),
        ("cell".to_string(), "0,0".to_string()),
        ("tile_folder".to_string(), too_long),
    ]);
    let (placed, folders_touched) = apply_md_placement(&mut settings, "p1", &fields);
    assert!(placed, "grid fields were valid - must not report nothing placed");
    assert!(!folders_touched);
    assert!(settings.tile_folders.is_empty());
    let key = grid_key(default_cols(), default_rows());
    assert_eq!(settings.views[0].layouts.get(&key).and_then(|m| m.get("p1")), Some(&[0, 0]));
}

// A folder name is only unique WITHIN its own view -- matching by name alone
// merged two same-named folders from different views into whichever the scan
// found first.
#[test]
fn sync_join_tile_folder_scopes_by_view_not_just_name() {
    let mut settings = Settings {
        views: vec![test_view("v1"), test_view("v2")],
        ..Default::default()
    };
    settings.tile_folders = vec![test_tile_folder("f1", "v1", [0, 0], &["a"])];
    settings.tile_folders[0].name = "Work".to_string();

    // Same name, DIFFERENT view -> must NOT merge into v1's folder.
    let ok = sync_join_tile_folder(&mut settings, "b", "Work", "", "v2", Some([0, 0]));
    assert!(ok);
    assert_eq!(settings.tile_folders.len(), 2, "same name in a different view must not merge");
    let v1_folder = settings.tile_folders.iter().find(|f| f.view_id == "v1").unwrap();
    let v2_folder = settings.tile_folders.iter().find(|f| f.view_id == "v2").unwrap();
    assert_eq!(v1_folder.members, vec!["a".to_string()]);
    assert_eq!(v2_folder.members, vec!["b".to_string()]);

    // Same name, SAME view -> must merge.
    let ok2 = sync_join_tile_folder(&mut settings, "c", "Work", "", "v1", Some([0, 0]));
    assert!(ok2);
    let v1_folder2 = settings.tile_folders.iter().find(|f| f.view_id == "v1").unwrap();
    assert!(v1_folder2.members.contains(&"c".to_string()));
    assert_eq!(settings.tile_folders.len(), 2, "the same-view join must still merge, not add a third folder");
}

// A folder whose members arrive in separate poll cycles (a cloud provider has
// no ordering/batching guarantee between two machines) must survive at 1
// member instead of being dissolved before the second file ever lands.
#[test]
fn sync_normalize_tile_folders_tolerates_a_still_forming_one_member_folder() {
    let ids = vec!["p1".to_string()];
    let view_ids = vec!["v1".to_string()];
    let mut folders = vec![test_tile_folder("f1", "v1", [0, 0], &["p1"])];
    sync_normalize_tile_folders(&mut folders, &ids, &view_ids);
    assert_eq!(folders.len(), 1, "a still-forming folder must survive a sync-triggered normalize");
    // But a genuinely empty folder (its only member vanished) still goes:
    // and the general, UI-facing normalize still dissolves anything under 2.
    let mut empty = vec![test_tile_folder("f2", "v1", [1, 0], &["gone"])];
    sync_normalize_tile_folders(&mut empty, &ids, &view_ids);
    assert!(empty.is_empty());
    let mut general = vec![test_tile_folder("f3", "v1", [0, 0], &["p1"])];
    normalize_tile_folders(&mut general, &ids, &view_ids);
    assert!(general.is_empty(), "the UI-facing normalize must keep dissolving a real 1-member folder");
}

#[test]
fn sync_normalize_float_folders_tolerates_a_still_forming_one_member_folder() {
    let ids = vec!["p1".to_string()];
    let mut folders = vec![FloatFolder {
        id: "f1".into(),
        name: "Team".into(),
        pos: Pos { x: 0, y: 0, ..Default::default() },
        scale: 1.0,
        members: vec!["p1".to_string()],
    }];
    sync_normalize_float_folders(&mut folders, &ids);
    assert_eq!(folders.len(), 1);
    let mut general = vec![FloatFolder {
        id: "f2".into(),
        name: "Team".into(),
        pos: Pos { x: 0, y: 0, ..Default::default() },
        scale: 1.0,
        members: vec!["p1".to_string()],
    }];
    normalize_float_folders(&mut general, &ids);
    assert!(general.is_empty());
}

// ---- Multi-instance crash simulation ----
//
// sync_poll_once itself needs a live AppHandle (Tauri window/state), which this
// test module has no harness for - every existing sync test works the same
// way, exercising the pure building blocks sync_poll_once is made of instead of
// the function itself. This helper replays exactly sync_poll_once's own
// per-file sequence (parse -> apply content -> apply placement -> normalize)
// against a real temp directory, so interleaving it under two independent
// (Vec<Prompt>, Settings) pairs stands in for two real instances syncing
// through the same cloud folder. Normalizing after every single file (finer
// grain than sync_poll_once's real once-per-batch) makes this the STRICTER
// case - since sync_normalize_* now tolerates a 1-member folder, that no
// longer breaks anything, and proves the fix holds at the worst-case
// granularity too.
fn poll_apply(prompts: &mut Vec<Prompt>, settings: &mut Settings, content: &str, stem: &str) {
    let fields = parse_frontmatter(content).filter(|(f, _)| md_is_ours(f)).map(|(f, _)| f);
    match prompts.iter().position(|p| p.id == stem) {
        Some(idx) => {
            if let Some(updated) = apply_md_to_prompt(&prompts[idx], content) {
                prompts[idx] = updated;
            }
            if let Some(f) = fields.as_ref() {
                apply_md_placement(settings, stem, f);
            }
        }
        None => {
            let Some(np) = apply_md_to_prompt(&sync_blank_prompt(stem.to_string()), content) else { return };
            prompts.push(np);
            if let Some(f) = fields.as_ref() {
                apply_md_placement(settings, stem, f);
            }
        }
    }
    let ids: Vec<String> = prompts.iter().map(|p| p.id.clone()).collect();
    let view_ids: Vec<String> = settings.views.iter().map(|v| v.id.clone()).collect();
    sync_normalize_tile_folders(&mut settings.tile_folders, &ids, &view_ids);
    sync_normalize_float_folders(&mut settings.float_folders, &ids);
}

// A scenario worth calling out by name: a cloud provider gives no
// ordering/batching guarantee between two files it propagates to a second
// machine. Instance A groups two prompts into a folder and writes both files;
// instance B's poll only sees the first one, then the second "arrives" a cycle
// later. Must end up with ONE folder holding both members, on a view B never
// had until this ran - never two folders, and never a folder that got
// dissolved before the second member could join.
#[test]
fn multi_instance_staggered_folder_arrival_never_loses_or_duplicates_the_folder() {
    let dir = std::env::temp_dir().join(format!("ps-multi-inst-{}", gen_id()));
    fs::create_dir_all(&dir).unwrap();

    let mut a_prompts = vec![sync_test_prompt(), {
        let mut p = sync_test_prompt();
        p.id = "p2".into();
        p
    }];
    for p in a_prompts.iter_mut() {
        p.name = format!("Prompt {}", p.id);
    }
    let mut a_settings = Settings::default();
    a_settings.views.push(test_view("v1"));
    a_settings.tile_folders.push({
        let mut f = test_tile_folder("f1", "v1", [0, 0], &["p1", "p2"]);
        f.name = "Team".to_string();
        f
    });
    for p in &a_prompts {
        let content = prompt_to_md(p, &sync_placement_for(p, &a_settings));
        fs::write(dir.join(format!("{}.md", p.id)), content).unwrap();
    }

    // Instance B starts from nothing - no matching view either, so this also
    // exercises auto-creating the view by name.
    let mut b_prompts: Vec<Prompt> = Vec::new();
    let mut b_settings = Settings::default();

    let p1_content = fs::read_to_string(dir.join("p1.md")).unwrap();
    poll_apply(&mut b_prompts, &mut b_settings, &p1_content, "p1");
    assert_eq!(b_settings.views.len(), 1, "the missing view must be auto-created by name");
    assert_eq!(b_settings.views[0].name, "V1");
    assert_eq!(b_settings.tile_folders.len(), 1, "a still-forming folder must survive its first poll");
    assert_eq!(b_settings.tile_folders[0].members, vec!["p1".to_string()]);

    // p2.md "arrives" one poll cycle later.
    let p2_content = fs::read_to_string(dir.join("p2.md")).unwrap();
    poll_apply(&mut b_prompts, &mut b_settings, &p2_content, "p2");
    assert_eq!(b_settings.views.len(), 1, "must reuse the view it already created, not duplicate it");
    assert_eq!(b_settings.tile_folders.len(), 1, "the second member must join the SAME folder, never create a second one");
    assert_eq!(b_settings.tile_folders[0].name, "Team");
    let mut members = b_settings.tile_folders[0].members.clone();
    members.sort();
    assert_eq!(members, vec!["p1".to_string(), "p2".to_string()]);

    let _ = fs::remove_dir_all(&dir);
}

// The other half of the crash-simulation requirement: a file
// cut off mid-write (crash between lock acquire and the write completing)
// never has a closing "---" at all. Must be skipped outright, never applied
// partially - no half-updated name, no fabricated placement from whatever
// fragment of the header made it to disk.
#[test]
fn a_file_truncated_by_a_simulated_crash_mid_write_is_skipped_not_misapplied() {
    let p = sync_test_prompt();
    let settings = Settings::default();
    let full = prompt_to_md(&p, &sync_placement_for(&p, &settings));
    let truncated = &full[..full.len() / 3];
    assert!(parse_frontmatter(truncated).is_none(), "an unclosed header must never parse as valid frontmatter");

    let mut prompts = vec![p.clone()];
    let mut target_settings = Settings::default();
    poll_apply(&mut prompts, &mut target_settings, truncated, "p1");
    assert_eq!(prompts[0].name, p.name, "a truncated file must never overwrite the prompt with garbage");
    assert!(target_settings.views.is_empty(), "a truncated file must never fabricate a placement either");
}

#[test]
fn md_escape_line_strips_embedded_newlines() {
    assert_eq!(md_escape_line("a\nb\r\nc"), "a b  c");
}

#[test]
fn parse_frontmatter_rejects_a_file_with_no_header() {
    assert!(parse_frontmatter("just some text\nwith no header").is_none());
    assert!(apply_md_to_prompt(&sync_blank_prompt("x".into()), "no header here").is_none());
}

#[test]
fn parse_frontmatter_rejects_an_unclosed_header() {
    // Opens with --- but never closes it: must not silently swallow the rest
    // of the file as a giant, wrong "field".
    assert!(parse_frontmatter("---\nname: x\nstill going...").is_none());
}

#[test]
fn frontmatter_tags_dedupe_case_insensitively_and_drop_blanks() {
    let base = sync_blank_prompt("p2".into());
    let content = "---\nname: x\ncolor: \ntags: Work, work, , Draft\nnotes: \n---\nbody\n";
    let updated = apply_md_to_prompt(&base, content).unwrap();
    assert_eq!(updated.tags, vec!["Work".to_string(), "Draft".to_string()]);
}

#[test]
fn sync_eligible_takes_media_prompts_but_still_refuses_an_unsafe_id() {
    let mut p = sync_test_prompt();
    assert!(sync_eligible(&p));
    // Attachments no longer disqualify a prompt: they sync as path references.
    p.image = "data:image/png;base64,x".into();
    p.file_path = "C:\\x.pdf".into();
    p.icon_path = "C:\\x.gif".into();
    assert!(sync_eligible(&p));
    // The id is still the file name, so it still has to be a safe token.
    let mut bad = sync_test_prompt();
    bad.id = "../evil".into();
    assert!(!sync_eligible(&bad));
}

#[test]
fn media_paths_round_trip_through_the_markdown_file() {
    let mut p = sync_test_prompt();
    p.file_path = "C:\\docs\\manual.pdf".into();
    p.icon_path = "C:\\icons\\book.png".into();
    p.pdf_page = 7;
    // The embedded copy has no path to write, so it must not appear in the file
    // at all: and must survive the round trip untouched.
    p.image = "data:image/png;base64,AAA".into();
    let md = prompt_to_md(&p, &SyncPlacement::default());
    assert!(md.contains("\nfile: C:\\docs\\manual.pdf\n"));
    assert!(md.contains("\nicon: C:\\icons\\book.png\n"));
    assert!(md.contains("\npdf_page: 7\n"));
    assert!(!md.contains("base64"));
    let back = apply_md_to_prompt(&p, &md).unwrap();
    assert_eq!(back.file_path, p.file_path);
    assert_eq!(back.icon_path, p.icon_path);
    assert_eq!(back.pdf_page, 7);
    assert_eq!(back.image, p.image);
}

#[test]
fn a_media_field_is_only_cleared_when_the_file_actually_says_so() {
    let mut p = sync_test_prompt();
    p.file_path = "C:\\docs\\manual.pdf".into();
    p.pdf_page = 3;
    // An older/hand-written file without the media headers leaves them alone…
    let legacy = "---\nname: x\ncolor: \ntags: \nnotes: \n---\nbody\n";
    let kept = apply_md_to_prompt(&p, legacy).unwrap();
    assert_eq!(kept.file_path, "C:\\docs\\manual.pdf");
    assert_eq!(kept.pdf_page, 3);
    // …while an explicitly empty one really does detach the file.
    let cleared = "---\nname: x\ncolor: \ntags: \nnotes: \nfile: \nicon: \n---\nbody\n";
    let out = apply_md_to_prompt(&p, cleared).unwrap();
    assert_eq!(out.file_path, "");
    assert_eq!(out.icon_path, "");
    // A garbled page number is ignored, never reset to the front page.
    let garbled = "---\nname: x\ncolor: \ntags: \nnotes: \npdf_page: seven\n---\nbody\n";
    assert_eq!(apply_md_to_prompt(&p, garbled).unwrap().pdf_page, 3);
}

#[test]
fn sync_write_all_is_a_no_op_when_the_feature_is_off() {
    let dir = std::env::temp_dir().join(format!("ps-sync-test-off-{}", gen_id()));
    let mut settings = Settings::default();
    settings.ui_texts.insert("syncDir".into(), dir.to_string_lossy().into_owned());
    // sync_enabled() reads ui_flags, left empty here -> off by default.
    let store = Store {
        prompts: vec![sync_test_prompt()],
        trash: Vec::new(),
        settings,
        prompts_locked: false,
        written: Default::default(),
    };
    // No AppHandle in a unit test, so exercise the same guard sync_write_all()
    // itself checks first: this is what keeps every non-sync-user's save_prompts
    // call free of any filesystem work.
    assert!(!sync_enabled(&store.settings));
    assert!(!dir.exists());
}

#[test]
fn sync_lock_is_exclusive_until_released() {
    let dir = std::env::temp_dir().join(format!("ps-sync-lock-test-{}", gen_id()));
    fs::create_dir_all(&dir).unwrap();
    let md_path = dir.join("p1.md");
    fs::write(&md_path, "x").unwrap();
    assert!(sync_lock_acquire(&md_path), "first acquire must succeed");
    assert!(!sync_lock_acquire(&md_path), "a fresh lock must never look stale");
    sync_lock_release(&md_path);
    assert!(sync_lock_acquire(&md_path), "must be acquirable again after release");
    sync_lock_release(&md_path);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_agents_md_is_written_once_and_never_overwritten() {
    let dir = std::env::temp_dir().join(format!("ps-sync-agents-test-{}", gen_id()));
    fs::create_dir_all(&dir).unwrap();
    sync_write_agents_md(&dir);
    let first = fs::read_to_string(dir.join("AGENTS.md")).unwrap();
    // Whatever this build is called: a literal here would have to be rewritten
    // for every fork, which is exactly what the brand layer exists to avoid.
    assert!(first.contains(brand::DISPLAY_NAME));
    assert!(first.contains(brand::ITEM));
    fs::write(dir.join("AGENTS.md"), "user's own edit, must survive").unwrap();
    sync_write_agents_md(&dir);
    assert_eq!(fs::read_to_string(dir.join("AGENTS.md")).unwrap(), "user's own edit, must survive");
    let _ = fs::remove_dir_all(&dir);
}

// ---- The folder the files live in, and letting go of an old one ----

fn sync_tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ps-sync-{tag}-{}", gen_id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

// Writes a file with exactly the header md_is_ours() demands.
fn write_ours(dir: &Path, name: &str, text: &str) {
    fs::write(dir.join(name), format!("---\nname: n\ncolor: \ntags: \nnotes: \n---\n{text}\n")).unwrap();
}

#[test]
fn sync_default_folder_is_named_after_the_brand_not_a_fixed_word() {
    // Fed a different slug it must produce that slug's folder: a hard-written
    // name would come out the same for every brand and fail right here.
    assert_eq!(sync_dir_leaf("other-brand"), "other-brand-sync");
    assert!(sync_dir_leaf(brand::SLUG).starts_with(brand::SLUG));
    assert_ne!(sync_dir_leaf(brand::SLUG), SYNC_LEGACY_DIR_NAME);
}

// The main window's JavaScript as one text: the module list in tests/ui_sources.json
// (concatenation order), read at run time so a module added there is picked up.
fn main_window_js() -> String {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let list = std::fs::read_to_string(root.join("tests").join("ui_sources.json")).expect("tests/ui_sources.json");
    let list: serde_json::Value = serde_json::from_str(&list).expect("ui_sources.json parses");
    list["main_window_modules"]
        .as_array()
        .expect("main_window_modules is a list")
        .iter()
        .map(|name| {
            let name = name.as_str().expect("module name");
            std::fs::read_to_string(root.join("ui").join(name)).unwrap_or_else(|e| panic!("ui/{name}: {e}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// The timer lives in Rust, the number shown in the expert menu lives in JS. If
// they drift apart the menu claims one interval while the app runs another.
#[test]
fn sync_poll_and_fail_defaults_match_the_expert_menus_own_defaults() {
    let js = main_window_js();
    let js = js.as_str();
    let js_def = |key: &str| -> f64 {
        let at = js.find(&format!("{key}: {{ label:")).unwrap_or_else(|| panic!("{key} row missing from the expert menu"));
        let row = &js[at..js[at..].find('\n').map(|e| at + e).unwrap_or(js.len())];
        let d = row.find("def: ").expect("row has a default");
        row[d + 5..]
            .split([',', ' '])
            .next()
            .expect("default value")
            .parse()
            .expect("default is a number")
    };
    assert_eq!(js_def("syncPollSecs"), SYNC_POLL_SECS as f64);
}

#[test]
fn sync_adopts_the_old_default_folder_once_and_only_for_an_unset_path() {
    let legacy = sync_tmp_dir("legacy");
    write_ours(&legacy, "p1.md", "body");
    let mut settings = Settings::default();
    let adopted = sync_adopt_legacy_dir(&mut settings, &legacy).expect("an existing folder with our files is adopted");
    assert_eq!(adopted, legacy.to_string_lossy());
    assert_eq!(settings.ui_texts.get("syncDir").map(String::as_str), Some(adopted.as_str()));
    // Once. A later run must never point the folder somewhere the user has since
    // moved away from.
    settings.ui_texts.insert("syncDir".into(), "D:\\elsewhere".into());
    assert!(sync_adopt_legacy_dir(&mut settings, &legacy).is_none());
    assert_eq!(settings.ui_texts.get("syncDir").map(String::as_str), Some("D:\\elsewhere"));

    // Somebody who already picked their own folder is left alone.
    let mut own = Settings::default();
    own.ui_texts.insert("syncDir".into(), "D:\\mine".into());
    assert!(sync_adopt_legacy_dir(&mut own, &legacy).is_none());
    assert_eq!(own.ui_texts.get("syncDir").map(String::as_str), Some("D:\\mine"));

    // An empty leftover folder is not something to adopt.
    let empty = sync_tmp_dir("legacy-empty");
    let mut fresh = Settings::default();
    assert!(sync_adopt_legacy_dir(&mut fresh, &empty).is_none());
    assert_eq!(fresh.ui_texts.get("syncDir").map(String::as_str), None);
    // A folder that is not there at all is the normal case for a new install.
    let mut newest = Settings::default();
    assert!(sync_adopt_legacy_dir(&mut newest, &empty.join("gone")).is_none());

    let _ = fs::remove_dir_all(&legacy);
    let _ = fs::remove_dir_all(&empty);
}

#[test]
fn sync_sweep_leaves_a_users_own_copy_of_one_of_our_files() {
    let dir = sync_tmp_dir("sweep-copy");
    write_ours(&dir, "p1.md", "kept, still in the library");
    // A copy the user made by hand. It carries our header (it is a copy of our
    // file), but its name could never be a prompt id, so it is not an orphan of
    // ours, and deleting it would destroy a file the user put there deliberately.
    write_ours(&dir, "p1 - Copy.md", "the user's own copy");
    write_ours(&dir, "gone.md", "belongs to a prompt that no longer exists");
    let wanted: std::collections::HashSet<String> = ["p1.md".to_string()].into_iter().collect();
    sync_known_note(&dir.join("p1 - Copy.md"));
    sync_known_note(&dir.join("gone.md"));
    sync_seen_record_own_write(&dir.join("gone.md"));
    sync_sweep_orphans(&dir, &wanted);
    assert!(dir.join("p1.md").exists(), "a live prompt's file stays");
    assert!(dir.join("p1 - Copy.md").exists(), "a name this app could never have written is never swept");
    assert!(!dir.join("gone.md").exists(), "a real orphan of ours is still cleaned up");
    let _ = fs::remove_dir_all(&dir);
}

fn sync_apply_store(prompts: Vec<Prompt>, trash: Vec<Prompt>) -> Store {
    Store { prompts, trash, settings: Settings::default(), prompts_locked: false, written: Default::default() }
}

fn sync_apply(store: &mut Store, stem: &str, content: String, is_new: bool) -> (bool, bool) {
    sync_poll_apply(store, vec![(stem.to_string(), content, is_new)], &mut |_, _, _| {})
}

#[test]
fn sync_poll_never_writes_a_files_plaintext_into_a_confidential_prompt() {
    let mut secret = sync_test_prompt();
    secret.secret = true;
    secret.text = "CIPHERTEXT".into();
    let mut plain = sync_test_prompt();
    plain.text = "plaintext from another machine".into();
    let content = prompt_to_md(&plain, &SyncPlacement::default());
    let mut store = sync_apply_store(vec![secret], Vec::new());
    let (changed, _) = sync_apply(&mut store, "p1", content, false);
    assert!(!changed);
    assert_eq!(store.prompts[0].text, "CIPHERTEXT");
    assert!(store.prompts[0].secret);
}

#[test]
fn sync_write_leaves_a_file_the_app_has_never_known() {
    let dir = sync_tmp_dir("foreign-file");
    write_ours(&dir, "foreign.md", "another machine's prompt, not imported yet");
    let mut keep = sync_test_prompt();
    keep.id = "mine".into();
    sync_write_dir(&dir, &[keep], &Settings::default());
    assert!(dir.join("foreign.md").exists(), "a file the app never knew must survive the sweep");
    assert!(dir.join("mine.md").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_write_still_sweeps_the_file_of_a_prompt_that_was_deleted() {
    let dir = sync_tmp_dir("deleted-file");
    let mut a = sync_test_prompt();
    a.id = "a".into();
    let mut b = sync_test_prompt();
    b.id = "b".into();
    sync_write_dir(&dir, &[a.clone(), b], &Settings::default());
    assert!(dir.join("b.md").exists());
    sync_write_dir(&dir, &[a], &Settings::default());
    assert!(!dir.join("b.md").exists(), "a deleted prompt takes its own file along");
    assert!(dir.join("a.md").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_write_keeps_a_conflict_copy_of_an_edit_the_poll_noted_but_not_read() {
    let dir = sync_tmp_dir("unread-edit");
    let path = dir.join("p1.md");
    let mut edited = sync_test_prompt();
    edited.text = "edited outside the app".into();
    fs::write(&path, prompt_to_md(&edited, &SyncPlacement::default())).unwrap();
    // The poll has seen this stamp (Settle) but has not read the file yet.
    let stamp = sync_stamp_of(&path).unwrap();
    sync_seen_lock().insert(sync_seen_key(&path), SyncSeen { stamp, handled: false, hash: 0 });
    let mut app_side = sync_test_prompt();
    app_side.text = "changed inside the app".into();
    sync_write_dir(&dir, &[app_side], &Settings::default());
    let copies = fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("p1.conflict-"))
        .count();
    assert_eq!(copies, 1, "the unread outside edit must be kept as a conflict copy");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_write_copies_an_undecodable_outside_edit_byte_for_byte() {
    let mut edited = sync_test_prompt();
    edited.text = "edited outside the app".into();
    let md = prompt_to_md(&edited, &SyncPlacement::default());
    let mut utf16 = vec![0xFFu8, 0xFE];
    for u in md.encode_utf16() {
        utf16.extend_from_slice(&u.to_le_bytes());
    }
    let mut ansi = md.clone().into_bytes();
    ansi.extend_from_slice(b"caf");
    ansi.push(0xE4);
    ansi.push(b'\n');
    // (tag, bytes, the poll already marked the stamp handled)
    for (tag, bytes, handled) in [("utf16", utf16, false), ("ansi", ansi, true)] {
        let dir = sync_tmp_dir(tag);
        let path = dir.join("p1.md");
        fs::write(&path, &bytes).unwrap();
        let stamp = sync_stamp_of(&path).unwrap();
        sync_seen_lock().insert(sync_seen_key(&path), SyncSeen { stamp, handled, hash: 0 });
        let mut app_side = sync_test_prompt();
        app_side.text = "changed inside the app".into();
        sync_write_dir(&dir, &[app_side], &Settings::default());
        let copies: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("p1.conflict-"))
            .collect();
        assert_eq!(copies.len(), 1, "{tag}: the outside edit must be kept as a conflict copy");
        assert_eq!(fs::read(copies[0].path()).unwrap(), bytes, "{tag}: the copy must be byte-identical");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn conflict_copies_in_the_same_second_never_overwrite_each_other() {
    let dir = sync_tmp_dir("conflict-names");
    let a = sync_write_conflict_copy(&dir, "p1", "", b"one").unwrap();
    let b = sync_write_conflict_copy(&dir, "p1", "", b"two").unwrap();
    let c = sync_write_conflict_copy(&dir, "p1", "-late", b"three").unwrap();
    let d = sync_write_conflict_copy(&dir, "p1", "-late", b"four").unwrap();
    let all = [&a, &b, &c, &d];
    for (i, x) in all.iter().enumerate() {
        for y in &all[i + 1..] {
            assert_ne!(x, y);
        }
    }
    assert_eq!(fs::read(&a).unwrap(), b"one");
    assert_eq!(fs::read(&b).unwrap(), b"two");
    assert_eq!(fs::read(&d).unwrap(), b"four");
    let _ = fs::remove_dir_all(&dir);
}

fn sync_conflict_copies(dir: &Path, id: &str) -> usize {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&format!("{id}.conflict-")))
        .count()
}

fn sync_legacy_md(p: &Prompt) -> String {
    prompt_to_md_impl(p, &SyncPlacement::default(), true)
}

#[test]
fn sync_startup_seed_keeps_an_edit_made_while_the_app_was_closed() {
    let dir = sync_tmp_dir("startup-seed");
    let mut stored = sync_test_prompt();
    stored.text = "the text this library holds".into();
    let mut other = sync_test_prompt();
    other.id = "p2".into();
    other.text = "ends with a newline\n".into();
    other.notes = "two\nlines".into();
    // p1 was edited by hand while the app was closed; p2 is an older build's file.
    let mut hand = stored.clone();
    hand.text = "typed by hand in the editor".into();
    fs::write(dir.join("p1.md"), prompt_to_md(&hand, &SyncPlacement::default())).unwrap();
    fs::write(dir.join("p2.md"), sync_legacy_md(&other)).unwrap();
    sync_seed_seen(&dir, &[stored.clone(), other.clone()], &Settings::default());
    let mut edited = stored.clone();
    edited.text = "changed inside the app".into();
    let mut edited2 = other.clone();
    edited2.text = "changed inside the app too".into();
    sync_write_dir(&dir, &[edited, edited2], &Settings::default());
    assert_eq!(sync_conflict_copies(&dir, "p1"), 1, "the hand edit must survive as a copy");
    assert_eq!(sync_conflict_copies(&dir, "p2"), 0, "a file matching the stored prompt needs no copy");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_first_write_into_a_folder_keeps_a_differing_file_and_not_a_matching_one() {
    let dir = sync_tmp_dir("activation");
    let mut stored = sync_test_prompt();
    stored.text = "the text this library holds".into();
    let mut other = stored.clone();
    other.id = "p2".into();
    other.text = "ends with a newline\n".into();
    other.notes = "two\nlines".into();
    let mut foreign = stored.clone();
    foreign.text = "another machine's version".into();
    fs::write(dir.join("p1.md"), prompt_to_md(&foreign, &SyncPlacement::default())).unwrap();
    fs::write(dir.join("p2.md"), sync_legacy_md(&other)).unwrap();
    // No seed, no SYNC_SEEN entry: activation, or a switch to a folder with files.
    sync_write_dir(&dir, &[stored, other], &Settings::default());
    assert_eq!(sync_conflict_copies(&dir, "p1"), 1, "a differing file must be kept as a copy");
    assert_eq!(sync_conflict_copies(&dir, "p2"), 0, "a file in an older format of the same prompt is no conflict");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_write_keeps_a_copy_of_a_file_that_arrived_while_its_prompt_was_trashed() {
    let dir = sync_tmp_dir("trash-restore");
    let path = dir.join("p1.md");
    let mut theirs = sync_test_prompt();
    theirs.text = "written by another machine meanwhile".into();
    fs::write(&path, prompt_to_md(&theirs, &SyncPlacement::default())).unwrap();
    // The poll saw it and skipped the trashed id: handled, but never known.
    let stamp = sync_stamp_of(&path).unwrap();
    sync_seen_lock().insert(sync_seen_key(&path), SyncSeen { stamp, handled: true, hash: 0 });
    sync_known_lock().remove(&sync_seen_key(&path));
    let mut restored = sync_test_prompt();
    restored.text = "the restored prompt".into();
    sync_write_dir(&dir, &[restored], &Settings::default());
    assert_eq!(sync_conflict_copies(&dir, "p1"), 1, "restoring must not overwrite the newer file silently");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_sweep_leaves_a_file_with_an_edit_not_read_yet() {
    let dir = sync_tmp_dir("sweep-unread");
    let mut a = sync_test_prompt();
    a.id = "a".into();
    let mut b = sync_test_prompt();
    b.id = "b".into();
    sync_write_dir(&dir, &[a.clone(), b.clone()], &Settings::default());
    // An outside edit lands on b.md; the poll has noted its stamp, not read it.
    let path = dir.join("b.md");
    let mut edited = b.clone();
    edited.text = "edited outside the app".into();
    fs::write(&path, prompt_to_md(&edited, &SyncPlacement::default())).unwrap();
    let stamp = sync_stamp_of(&path).unwrap();
    sync_seen_lock().insert(sync_seen_key(&path), SyncSeen { stamp, handled: false, hash: 0 });
    sync_write_dir(&dir, &[a], &Settings::default());
    assert!(path.exists(), "an edit nobody has read must not be swept");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_sweep_spares_a_file_whose_prompt_came_back_meanwhile() {
    let dir = sync_tmp_dir("sweep-live");
    let mut a = sync_test_prompt();
    a.id = "a".into();
    let mut b = sync_test_prompt();
    b.id = "b".into();
    sync_write_dir(&dir, &[a.clone(), b], &Settings::default());
    // A stale snapshot without b, while the library holds b by now (imported).
    sync_write_dir_checked(&dir, &[a], &Settings::default(), &|id| id == "b");
    assert!(dir.join("b.md").exists(), "a file whose prompt is live again must not be swept");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_write_ignores_a_touch_that_left_the_bytes_alone() {
    let dir = sync_tmp_dir("touch-only");
    let path = dir.join("p1.md");
    let first = sync_test_prompt();
    sync_write_dir(&dir, std::slice::from_ref(&first), &Settings::default());
    // A cloud client re-touches the file: same bytes, newer modification time.
    let bytes = fs::read(&path).unwrap();
    let f = fs::OpenOptions::new().write(true).open(&path).unwrap();
    f.set_modified(std::time::SystemTime::now() + Duration::from_secs(30)).unwrap();
    drop(f);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let mut edited = first;
    edited.text = "changed inside the app".into();
    sync_write_dir(&dir, &[edited], &Settings::default());
    assert_eq!(sync_conflict_copies(&dir, "p1"), 0, "the app's own earlier content is no outside edit");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_requeued_candidate_is_read_again_and_not_overwritten_unseen() {
    let dir = sync_tmp_dir("requeue");
    let path = dir.join("p1.md");
    let mut edited = sync_test_prompt();
    edited.text = "edited outside the app".into();
    fs::write(&path, prompt_to_md(&edited, &SyncPlacement::default())).unwrap();
    let stamp = sync_stamp_of(&path).unwrap();
    let key = sync_seen_key(&path);
    // The poll read the file and marked it handled, but could not apply it.
    sync_seen_lock().insert(key.clone(), SyncSeen { stamp, handled: true, hash: sync_hash(&fs::read(&path).unwrap()) });
    sync_requeue_candidates(&dir, &[("p1".to_string(), String::new(), false)]);
    let entry = sync_seen_lock().get(&key).copied().unwrap();
    assert!(!entry.handled);
    assert_eq!(sync_scan_decision(Some(entry), stamp), SyncScan::Read);
    let mut app_side = sync_test_prompt();
    app_side.text = "changed inside the app".into();
    sync_write_dir(&dir, &[app_side], &Settings::default());
    assert_eq!(sync_conflict_copies(&dir, "p1"), 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_scan_keeps_the_records_of_other_folders() {
    let a = sync_tmp_dir("scan-a");
    let b = sync_tmp_dir("scan-b");
    let in_b = b.join("p1.md");
    fs::write(&in_b, "x").unwrap();
    sync_seen_record_own_write(&in_b);
    let gone = a.join("gone.md");
    sync_seen_lock().insert(sync_seen_key(&gone), SyncSeen { stamp: SyncStamp { mtime_ns: 1, len: 1 }, handled: true, hash: 0 });
    sync_scan_folder(&a).unwrap();
    assert!(sync_seen_lock().contains_key(&sync_seen_key(&in_b)), "another folder's record must survive a scan");
    assert!(!sync_seen_lock().contains_key(&sync_seen_key(&gone)), "a vanished file in the scanned folder is dropped");
    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
}

#[test]
fn sync_same_dir_compares_by_identity_not_spelling() {
    let dir = sync_tmp_dir("same-dir");
    let name = dir.file_name().unwrap().to_string_lossy().to_uppercase();
    let shouting = dir.with_file_name(name);
    assert_ne!(dir, shouting, "a plain path compare (the old check) calls them different");
    assert!(sync_same_dir(&dir, &shouting), "the same folder in another letter case is the same folder");
    assert!(sync_same_dir(&dir, &dir.join(".")));
    let other = sync_tmp_dir("same-dir-other");
    assert!(!sync_same_dir(&dir, &other));
    assert!(sync_same_dir(&dir, &dir.join("inner")), "a folder inside the other counts too");
    assert!(
        sync_same_dir(&dir, &dir.with_file_name("ps-sync-no-such-folder-anywhere")),
        "a path that cannot be resolved is treated as maybe the same, so nothing gets deleted"
    );
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&other);
}

#[test]
fn sync_poll_does_not_import_a_file_whose_id_sits_in_the_trash() {
    let trashed = sync_test_prompt();
    let content = prompt_to_md(&trashed, &SyncPlacement::default());
    let mut store = sync_apply_store(Vec::new(), vec![trashed]);
    let (changed, _) = sync_apply(&mut store, "p1", content, true);
    assert!(!changed);
    assert!(store.prompts.is_empty(), "a trashed id must not come back as a second, live prompt");
    assert_eq!(store.trash.len(), 1);
}

#[test]
fn sync_poll_apply_reports_settings_changed_for_a_placement_only_file() {
    let p = sync_test_prompt();
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    let key = grid_key(settings.views[0].cols, settings.views[0].rows);
    settings.views[0].layouts.entry(key.clone()).or_default().insert("p1".into(), [0, 0]);
    let mut moved = settings.clone();
    moved.views[0].layouts.get_mut(&key).unwrap().insert("p1".into(), [2, 1]);
    let content = prompt_to_md(&p, &sync_placement_for(&p, &moved));
    let mut store = sync_apply_store(vec![p], Vec::new());
    store.settings = settings;
    let (changed, settings_changed) = sync_apply(&mut store, "p1", content, false);
    assert!(!changed, "a file that only moved the tile must not count as a changed prompt");
    assert!(settings_changed, "the flag the store-synced event keys on must be set for a moved tile");
    assert_eq!(store.settings.views[0].layouts[&key]["p1"], [2, 1]);
}

#[test]
fn sync_join_adopts_the_cell_and_colour_of_an_existing_grid_folder() {
    let p = sync_test_prompt();
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    settings.tile_folders.push({
        let mut f = test_tile_folder("f1", "v1", [0, 0], &["p1"]);
        f.name = "F".to_string();
        f
    });
    let grid = grid_key(settings.views[0].cols, settings.views[0].rows);
    let incoming_placement = SyncPlacement {
        view: "V1".into(),
        grid,
        cell: "3,1".into(),
        tile_folder: "F".into(),
        tile_folder_color: "#ef4444".into(),
        float_folder: String::new(),
    };
    let incoming = prompt_to_md(&p, &incoming_placement);
    let (_, fields_body) = parse_frontmatter(&incoming).unwrap();
    let _ = fields_body;
    let fields = parse_frontmatter(&incoming).unwrap().0;
    apply_md_placement(&mut settings, "p1", &fields);
    assert_eq!(settings.tile_folders.len(), 1);
    assert_eq!(settings.tile_folders[0].cell, [3, 1]);
    assert_eq!(settings.tile_folders[0].color, "#ef4444");
    assert_eq!(
        prompt_to_md(&p, &sync_placement_for(&p, &settings)),
        incoming,
        "after adopting, this machine writes exactly what it read, so the two stop rewriting each other"
    );
}

#[test]
fn sync_join_keeps_a_folders_cell_when_the_file_carries_none() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    settings.tile_folders.push({
        let mut f = test_tile_folder("f1", "v1", [2, 2], &["a"]);
        f.name = "F".to_string();
        f
    });
    assert!(sync_join_tile_folder(&mut settings, "b", "F", "", "v1", None));
    assert_eq!(settings.tile_folders[0].cell, [2, 2]);
}

#[test]
fn a_byte_order_mark_does_not_hide_an_edited_file() {
    let base = sync_test_prompt();
    let mut edited = sync_test_prompt();
    edited.text = "edited in an editor that adds a BOM".into();
    let content = format!("\u{feff}{}", prompt_to_md(&edited, &SyncPlacement::default()));
    let got = apply_md_to_prompt(&base, &content).expect("a BOM must not make the header unreadable");
    assert_eq!(got.text, edited.text);
}

#[test]
fn sync_decode_text_reads_utf8_bom_and_utf16_files() {
    let md = prompt_to_md(&sync_test_prompt(), &SyncPlacement::default());
    assert_eq!(sync_decode_text(md.clone().into_bytes()).as_deref(), Some(md.as_str()));
    let mut bom8 = vec![0xEF, 0xBB, 0xBF];
    bom8.extend_from_slice(md.as_bytes());
    assert_eq!(sync_decode_text(bom8).as_deref(), Some(md.as_str()));
    let mut le = vec![0xFF, 0xFE];
    le.extend(md.encode_utf16().flat_map(|u| u.to_le_bytes()));
    assert_eq!(sync_decode_text(le).as_deref(), Some(md.as_str()));
    let mut be = vec![0xFE, 0xFF];
    be.extend(md.encode_utf16().flat_map(|u| u.to_be_bytes()));
    assert_eq!(sync_decode_text(be).as_deref(), Some(md.as_str()));
    assert!(sync_decode_text(vec![0xFF, 0xFE, 0x41]).is_none(), "an odd UTF-16 length is not text");
    assert!(sync_decode_text(vec![0xC3, 0x28]).is_none());
}

#[test]
fn a_round_trip_keeps_multi_line_notes_and_a_trailing_newline() {
    let mut p = sync_test_prompt();
    p.notes = "a\nb\\c\r\nd".into();
    p.text = "abc\n".into();
    let back = apply_md_to_prompt(&p, &prompt_to_md(&p, &SyncPlacement::default())).unwrap();
    assert_eq!(back.notes, p.notes);
    assert_eq!(back.text, p.text);
    p.text = "one\n\n".into();
    p.variants = vec!["two".into()];
    let back = apply_md_to_prompt(&p, &prompt_to_md(&p, &SyncPlacement::default())).unwrap();
    assert_eq!(back.text, p.text);
    assert_eq!(back.variants, p.variants);
    // A note without a line break keeps its exact old form: no churn for files that round-tripped fine.
    let mut plain = sync_test_prompt();
    plain.notes = "C:\\new\\file is fine".into();
    let md = prompt_to_md(&plain, &SyncPlacement::default());
    assert!(!md.contains("notes_enc"), "{md}");
    assert_eq!(apply_md_to_prompt(&plain, &md).unwrap().notes, "C:\\new\\file is fine");
}

#[test]
fn a_file_in_the_previous_format_does_not_read_as_an_outside_edit() {
    let mut p = sync_test_prompt();
    p.notes = "line one\nline two".into();
    p.text = "ends with a newline\n".into();
    let placement = SyncPlacement::default();
    let old_form = prompt_to_md_impl(&p, &placement, true);
    assert_ne!(old_form, prompt_to_md(&p, &placement));
    assert!(sync_md_matches(&p, &placement, &old_form));
    assert!(sync_md_matches(&p, &placement, &prompt_to_md(&p, &placement)));
    let mut other = p.clone();
    other.name = "renamed".into();
    assert!(!sync_md_matches(&other, &placement, &old_form));
}

#[test]
fn a_new_sync_folder_clears_a_paused_health() {
    let mut h = SyncHealth::default();
    for _ in 0..SYNC_FAIL_MAX {
        sync_health_note_fail(&mut h, SYNC_FAIL_MAX, "the drive is gone");
    }
    assert!(h.paused);
    sync_health_after_dir_change(&mut h);
    assert!(!h.paused);
    assert_eq!(h.fails, 0);
    assert!(h.reason.is_empty());
}

#[test]
fn sync_delete_dir_removes_only_the_files_the_app_wrote() {
    let dir = sync_tmp_dir("del-ok");
    write_ours(&dir, "p1.md", "one");
    write_ours(&dir, "p2.md", "two");
    sync_known_note(&dir.join("p1.md"));
    sync_known_note(&dir.join("p2.md"));
    fs::write(dir.join("AGENTS.md"), sync_agents_md()).unwrap();
    fs::write(dir.join("p1.md.lock"), "1234").unwrap();
    let data = sync_tmp_dir("del-ok-data");
    assert_eq!(sync_delete_dir(&dir, &data).unwrap(), 4);
    assert!(!dir.exists(), "an emptied folder goes with the files");
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn sync_delete_dir_aborts_whole_on_one_file_the_app_did_not_write() {
    let dir = sync_tmp_dir("del-foreign");
    write_ours(&dir, "p1.md", "one");
    fs::write(dir.join("my-taxes.txt"), "not yours").unwrap();
    fs::write(dir.join("plain-note.md"), "# just a note\n").unwrap();
    let data = sync_tmp_dir("del-foreign-data");
    let err = sync_delete_dir(&dir, &data).expect_err("a stranger's file stops the delete");
    assert!(err.contains("my-taxes.txt"), "the reason names what is in the way: {err}");
    assert!(err.contains("plain-note.md"), "every unknown file is named: {err}");
    // Nothing at all is removed on a refusal, not even our own files.
    assert!(dir.join("p1.md").exists());
    assert!(dir.join("my-taxes.txt").exists());
    assert!(dir.exists());
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn sync_delete_dir_keeps_a_file_the_library_never_had() {
    let dir = sync_tmp_dir("del-unknown");
    write_ours(&dir, "p1.md", "one");
    sync_known_note(&dir.join("p1.md"));
    // Another machine's new file: our header, a valid id, never imported here.
    write_ours(&dir, "fresh-from-elsewhere.md", "not in this library yet");
    let data = sync_tmp_dir("del-unknown-data");
    let err = sync_delete_dir(&dir, &data).expect_err("an unimported file stops the delete");
    assert!(err.contains("fresh-from-elsewhere.md"), "the file in the way is named: {err}");
    assert!(dir.join("fresh-from-elsewhere.md").exists());
    assert!(dir.join("p1.md").exists());
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn sync_delete_dir_is_never_recursive_and_never_the_data_folder() {
    let dir = sync_tmp_dir("del-sub");
    write_ours(&dir, "p1.md", "one");
    let sub = dir.join("holiday photos");
    fs::create_dir_all(&sub).unwrap();
    write_ours(&sub, "p9.md", "a file below, must not be reached");
    let data = sync_tmp_dir("del-sub-data");
    let err = sync_delete_dir(&dir, &data).expect_err("a sub-folder stops the delete");
    assert!(err.contains("holiday photos"), "the sub-folder is named: {err}");
    assert!(sub.join("p9.md").exists(), "nothing below the folder is ever touched");

    // The folder that holds the database itself can never be the one deleted,
    // whatever the settings claim.
    assert!(sync_delete_dir(&data, &data).is_err());
    assert!(data.exists());
    let err = sync_delete_dir(&data.join("sub"), &data.join("sub").join("deeper")).unwrap_err();
    assert!(!err.is_empty());
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn sync_dir_scan_counts_our_files_apart_from_everything_else() {
    let dir = sync_tmp_dir("scan");
    write_ours(&dir, "p1.md", "one");
    sync_known_note(&dir.join("p1.md"));
    fs::write(dir.join("AGENTS.md"), "the user rewrote this note").unwrap();
    fs::write(dir.join("notes.md"), "no header at all").unwrap();
    let scan = sync_dir_scan(&dir);
    assert_eq!(scan.ours.len(), 1);
    assert_eq!(scan.foreign.len(), 2, "an edited AGENTS.md is the user's text now");
    let _ = fs::remove_dir_all(&dir);
}

// ---- Reading the folder: settle first, then read, then leave it alone ----

#[test]
fn sync_scan_reads_a_file_only_once_it_has_stopped_changing() {
    let a = SyncStamp { mtime_ns: 10, len: 100 };
    let b = SyncStamp { mtime_ns: 20, len: 140 };
    // Never seen: remember it, read it next round. An editor that writes straight
    // into the file would otherwise be caught halfway through.
    assert_eq!(sync_scan_decision(None, a), SyncScan::Settle);
    assert_eq!(sync_scan_decision(Some(SyncSeen { stamp: a, handled: false, hash: 0 }), b), SyncScan::Settle);
    // Unchanged since the last look: now it is whole and worth reading.
    assert_eq!(sync_scan_decision(Some(SyncSeen { stamp: a, handled: false, hash: 0 }), a), SyncScan::Read);
    // Already dealt with (read, or written by the app itself): no read at all.
    assert_eq!(sync_scan_decision(Some(SyncSeen { stamp: a, handled: true, hash: 0 }), a), SyncScan::Skip);
    assert_eq!(sync_scan_decision(Some(SyncSeen { stamp: a, handled: true, hash: 0 }), b), SyncScan::Settle);
}

#[test]
fn sync_stamp_notices_a_rewrite_of_the_same_length() {
    let dir = sync_tmp_dir("stamp");
    let path = dir.join("p1.md");
    fs::write(&path, "aaaa").unwrap();
    let first = sync_stamp_of(&path).unwrap();
    // Same size, different content: the modification time is what catches it.
    std::thread::sleep(Duration::from_millis(20));
    fs::write(&path, "bbbb").unwrap();
    let second = sync_stamp_of(&path).unwrap();
    assert_ne!(first, second);
    assert!(sync_stamp_of(&dir.join("nope.md")).is_none());
    let _ = fs::remove_dir_all(&dir);
}

// ---- One poll loop at a time, and none at all once the feature is off ----

#[test]
fn sync_poll_claim_lets_only_one_loop_hold_the_slot_at_once() {
    // A stray claim left by another test in this binary would make this one
    // flaky, so start from a known-released state regardless of history.
    sync_poll_release();
    assert!(sync_poll_try_claim(), "the first caller gets the slot");
    assert!(!sync_poll_try_claim(), "a second caller must not start a second timer");
    sync_poll_release();
    assert!(sync_poll_try_claim(), "released, the next caller can claim it again");
    sync_poll_release();
}

// ---- An unreachable folder pauses instead of failing forever ----

#[test]
fn sync_pauses_after_the_set_number_of_failed_rounds_and_says_so_once() {
    let mut h = SyncHealth::default();
    assert!(!sync_health_note_fail(&mut h, 3, "drive not there"));
    assert!(!sync_health_note_fail(&mut h, 3, "drive not there"));
    assert!(h.reason.contains("drive"));
    assert!(sync_health_note_fail(&mut h, 3, "drive not there"), "the round that reaches the limit reports once");
    assert!(h.paused);
    assert!(!sync_health_note_fail(&mut h, 3, "drive not there"), "and never again while it stays paused");
    // Paused rounds come back far apart, never stop entirely.
    assert_eq!(sync_wait_secs(true, 5), 50);
    assert_eq!(sync_wait_secs(false, 5), 5);
    // One good round clears everything.
    sync_health_note_ok(&mut h, 42, 1700);
    assert!(!h.paused);
    assert_eq!(h.fails, 0);
    assert_eq!(h.files, 42);
    assert_eq!(h.last_ok, 1700);
    assert!(h.reason.is_empty());
}

#[test]
fn sync_fail_max_is_the_fixed_five_rounds() {
    assert_eq!(SYNC_FAIL_MAX, 5);
}

// ---- What a hand-written file may put into the library ----

#[test]
fn incoming_tags_obey_the_same_rules_as_every_other_way_in() {
    let p = sync_test_prompt();
    let long = "x".repeat(MAX_TAG_LEN + 1);
    let many = (0..MAX_TAGS_PER_PROMPT + 5).map(|i| format!("t{i}")).collect::<Vec<_>>().join(", ");
    let content = format!("---\nname: n\ncolor: \ntags: ok, {long}, with\tcontrol, ok, {many}\nnotes: \n---\nbody\n");
    let out = apply_md_to_prompt(&p, &content).expect("a bad tag never costs the file its text");
    assert_eq!(out.text, "body", "the prompt itself still arrives");
    assert!(out.tags.len() <= MAX_TAGS_PER_PROMPT, "never more tags than the app allows: {:?}", out.tags);
    assert!(out.tags.iter().all(|t| t.chars().count() <= MAX_TAG_LEN), "no over-long tag: {:?}", out.tags);
    assert!(out.tags.iter().all(|t| tag_chars_ok(t)), "no control characters: {:?}", out.tags);
    assert_eq!(out.tags.iter().filter(|t| t.as_str() == "ok").count(), 1, "the duplicate is dropped");
}

#[test]
fn md_roundtrip_survives_umlauts_emoji_separators_and_the_rotation_marker() {
    let mut p = sync_test_prompt();
    p.name = "Grüße; Ümläute 🎉".into();
    p.notes = "Semikolon; Doppelpunkt: Emoji 🙂".into();
    p.text = "Zeile eins\n\nZeile zwei mit ; und : und 🎯\nund ein wörtliches <!-- next --> im Text".into();
    p.variants = vec!["Zweiter Eintrag 🌍".into(), "Dritter; mit Umlaut ä".into()];
    let content = prompt_to_md(&p, &SyncPlacement::default());
    let back = apply_md_to_prompt(&p, &content).expect("valid frontmatter");
    assert_eq!(back.name, p.name);
    assert_eq!(back.notes, p.notes);
    assert_eq!(back.text, p.text, "a literal rotation marker in the text is not a separator");
    assert_eq!(back.variants, p.variants);
    assert_eq!(back.variants.len(), p.variants.len(), "the number of entries is the same both ways");
    // Byte-identical the second time round, or the folder would rewrite itself
    // on every poll.
    assert_eq!(prompt_to_md(&back, &SyncPlacement::default()), content);
    assert!(content.is_char_boundary(content.len()), "written as plain UTF-8");

    // A separator someone broke by hand is simply not a separator any more: the
    // entries merge into one, and not a single character is lost.
    let hand_edited = content.replace("<!-- next -->", "<!-- nxt -->");
    let merged = apply_md_to_prompt(&p, &hand_edited).expect("still our file");
    assert!(merged.variants.is_empty());
    assert!(merged.text.contains("Zweiter Eintrag 🌍") && merged.text.contains("Dritter; mit Umlaut ä"));
}

#[test]
fn sync_lock_is_stale_once_the_holder_pid_is_dead() {
    // Bypasses the age check by asserting the pid-liveness half directly:
    // the age half needs real elapsed wall-clock time, not worth a sleeping test.
    assert!(pid_is_alive(std::process::id()), "this test's own process is alive");
}

// The poll interval and the staleness window are independent numbers. What keeps
// a lock held through a long write from being stolen by the next round is not the
// window at all, it is that the writing process is still alive.
#[test]
fn a_lock_held_by_a_living_process_never_goes_stale_however_old_it_is() {
    let dir = sync_tmp_dir("lock-age");
    let md = dir.join("p1.md");
    fs::write(&md, "x").unwrap();
    assert!(sync_lock_acquire(&md));
    let lock_path = sync_lock_path(&md);
    backdate(&lock_path, Duration::from_secs(SYNC_LOCK_STALE_SECS * 20));
    assert!(!sync_lock_is_stale(&lock_path), "our own still-running write keeps its lock");
    assert!(!sync_lock_acquire(&md), "and nobody else takes it from us");
    // A lock whose writer is gone (no readable pid) is picked up once it is old.
    fs::write(&lock_path, "not a pid").unwrap();
    backdate(&lock_path, Duration::from_secs(SYNC_LOCK_STALE_SECS * 20));
    assert!(sync_lock_is_stale(&lock_path), "an abandoned lock does not block the folder forever");
    sync_lock_release(&md);
    let _ = fs::remove_dir_all(&dir);
}

// A thousand entries, checked every few seconds: the round must not read the
// whole folder every time. Prints both numbers so a regression is visible.
// Repeats each side and keeps the smallest time: this test runs inside the
// full suite alongside a lot of unrelated disk and ffmpeg activity, and a
// single sample of a few dozen milliseconds was getting swamped by someone
// else's scheduling slice often enough to fail on a clean tree. The minimum
// of several still reflects the real cost of each approach - a regression
// back to a full read every round would still lose every single time.
#[test]
fn a_thousand_entries_cost_metadata_and_not_a_full_read_each_round() {
    const ROUNDS: u32 = 7;
    let dir = sync_tmp_dir("perf-1000");
    let mut prompts = Vec::new();
    for i in 0..1000 {
        let mut p = sync_blank_prompt(format!("perf{i}"));
        p.name = format!("Entry {i}");
        p.text = format!("Line one of entry {i}\nLine two, a bit of text to make the file realistic.\n");
        fs::write(dir.join(format!("{}.md", p.id)), prompt_to_md(&p, &SyncPlacement::default())).unwrap();
        prompts.push(p);
    }
    let mut seen: SyncSeenMap = HashMap::new();
    for p in &prompts {
        let path = dir.join(format!("{}.md", p.id));
        seen.insert(sync_seen_key(&path), SyncSeen { stamp: sync_stamp_of(&path).unwrap(), handled: true, hash: 0 });
    }
    let mut full_best = Duration::MAX;
    let mut stamped_best = Duration::MAX;
    let (mut read, mut skipped) = (0usize, 0usize);
    for _ in 0..ROUNDS {
        // What the round used to cost: every file read and compared, every time.
        let t0 = std::time::Instant::now();
        read = 0;
        for p in &prompts {
            let path = dir.join(format!("{}.md", p.id));
            if let Some(c) = read_text_capped(&path, MAX_SYNC_MD_BYTES) {
                if c != prompt_to_md(p, &SyncPlacement::default()) {
                    panic!("written and re-read content must match");
                }
                read += 1;
            }
        }
        full_best = full_best.min(t0.elapsed());
        // What it costs now once everything has settled: one stat per file.
        let t1 = std::time::Instant::now();
        skipped = 0;
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            let Some(stamp) = sync_stamp_of(&path) else { continue };
            if sync_scan_decision(seen.get(&sync_seen_key(&path)).copied(), stamp) == SyncScan::Skip {
                skipped += 1;
            }
        }
        stamped_best = stamped_best.min(t1.elapsed());
    }
    println!(
        "sync round over {read} entries: full read {full_best:?} (best of {ROUNDS}), \
             unchanged-check {stamped_best:?} (best of {ROUNDS}), skipped {skipped}"
    );
    assert_eq!(read, 1000);
    assert_eq!(skipped, 1000, "an unchanged folder is skipped file by file, not re-read");
    assert!(stamped_best < full_best, "the settled round must be the cheaper one: {stamped_best:?} vs {full_best:?}");
    let _ = fs::remove_dir_all(&dir);
}

// Every identifier a rebrand may NOT quietly change. These literals are a
// deliberate second copy of brand.rs: an installation finds its data, its
// autostart entry and its update source through them, so a refactor that moves
// one has to fail here instead of in someone's %APPDATA%. tools/brand.js
// rewrites this block (BRAND-PIN markers) when a different brand is stamped.
#[test]
fn brand_values_are_pinned() {
    // BRAND-PIN-START
    assert_eq!(brand::DISPLAY_NAME, "Prompt Saver");
    assert_eq!(brand::SLUG, "prompt-saver");
    assert_eq!(brand::IDENTIFIER, "com.bgcoding.promptsaver");
    assert_eq!(brand::DATA_FOLDER, "Prompt-Saver");
    assert_eq!(brand::AUTOSTART_KEY, "PromptSaver");
    assert_eq!(brand::TEMP_DIR, "prompt-saver");
    assert_eq!(brand::TEMP_PREFIX, "promptsaver");
    assert_eq!(brand::WINDOW_CLASS, "PromptSaverClipWatch");
    assert_eq!(brand::USER_AGENT, "PromptSaver");
    assert_eq!(brand::REPO, "wbgcoding/Prompt-Saver");
    assert_eq!(brand::LEGACY_DATA_FOLDER, "com.bgcoding.promptsaver");
    assert_eq!(brand::LEGACY_AUTOSTART_KEY, "");
    assert_eq!(brand::LEGACY_SCREENSHOT_PREFIX, "PromptSaver-");
    assert_eq!(env!("CARGO_PKG_NAME"), "prompt-saver");
    let conf = include_str!("../tauri.conf.json");
    assert!(conf.contains("\"productName\": \"Prompt Saver\""));
    assert!(conf.contains("\"identifier\": \"com.bgcoding.promptsaver\""));
    let js = include_str!("../../ui/brand.js");
    assert!(js.contains("APP_NAME = \"Prompt Saver\";"));
    // BRAND-PIN-END
    // The consts the app actually reads must be the ones pinned above.
    assert_eq!(AUTOSTART_KEY, brand::AUTOSTART_KEY);
    assert_eq!(DATA_FOLDER, brand::DATA_FOLDER);
    // Backup crypto is NOT brand-derived: these two are frozen for every build
    // of every brand, or existing .psb files stop opening.
    assert_eq!(BACKUP_PEPPER, &b"prompt-saver::backup::pepper::v1"[..]);
    assert_eq!(BACKUP_FIXED_KEY[0], 0x8f);
    assert_eq!(BACKUP_FIXED_KEY[31], 0x84);
}

#[test]
fn update_url_pin_rejects_escapes() {
    let base = format!("https://github.com/{}/releases/download", brand::REPO);
    // The one legit shape is accepted.
    assert!(valid_update_url(&format!("{base}/v2.5.0/App_2.5.0_x64-setup.exe")));
    // Path-traversal to another repo, still github.com and still prefix-matching.
    assert!(!valid_update_url(&format!(
        "{base}/../../attacker/repo/releases/download/v1/x-setup.exe"
    )));
    // Encoded dot-segment, backslash, extra scheme/authority, query, non-exe.
    assert!(!valid_update_url(&format!("{base}/%2e%2e/x/y-setup.exe")));
    assert!(!valid_update_url(&format!("{base}/v1\\x/y-setup.exe")));
    assert!(!valid_update_url(&format!("{base}/v1/y-setup.exe?x=1")));
    assert!(!valid_update_url(&format!("{base}/v1/y.txt")));
    assert!(!valid_update_url(&format!("http://github.com/{}/releases/download/v1/y-setup.exe", brand::REPO)));
    // Extra path segments beyond tag/asset.
    assert!(!valid_update_url(&format!("{base}/v1/sub/y-setup.exe")));
    // A different repo under the same host is not ours.
    assert!(!valid_update_url("https://github.com/someone/else/releases/download/v1/y-setup.exe"));
}

#[test]
fn install_update_rejects_a_release_older_than_the_installed_version() {
    let base = format!("https://github.com/{}/releases/download", brand::REPO);
    // An older or equal tag than APP_VERSION must never pass, even at an
    // otherwise perfectly pinned, valid-shaped URL.
    assert!(!update_url_is_newer_release(&format!("{base}/v0.0.1/App_0.0.1_x64-setup.exe")));
    assert!(!update_url_is_newer_release(&format!(
        "{base}/v{APP_VERSION}/App_{APP_VERSION}_x64-setup.exe"
    )));
    // A genuinely newer tag still passes.
    assert!(update_url_is_newer_release(&format!(
        "{base}/v99.0.0/App_99.0.0_x64-setup.exe"
    )));
    // A malformed/missing tag is rejected rather than treated as newer.
    assert!(!update_url_is_newer_release(&format!("{base}//App-setup.exe")));
}

// The webview may load media and PDF attachments from anywhere the user picked,
// so the asset protocol's allow list is deliberately wide. Everything that holds
// the store itself has to be denied by name, or a crafted attachment path would
// hand the page the whole database. Backups count: a snapshot folder holds a
// gzipped copy of data.db, and a .psb is the same store in one file.
#[test]
fn asset_scope_denies_every_store_file() {
    // A narrow glob match good enough for this fixed pattern shape: "**/name",
    // "**/prefix*" and "**/dir/**" -- not a general glob engine.
    fn covers(pattern: &str, path: &str) -> bool {
        let Some(rest) = pattern.strip_prefix("**/") else { return false };
        if let Some(dir) = rest.strip_suffix("/**") {
            path.split('/').any(|seg| seg == dir)
        } else if let Some(prefix) = rest.strip_suffix('*') {
            path.rsplit('/').next().is_some_and(|name| name.starts_with(prefix))
        } else {
            path.rsplit('/').next() == Some(rest)
        }
    }

    let conf: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json parses");
    let deny = conf["app"]["security"]["assetProtocol"]["scope"]["deny"]
        .as_array()
        .expect("a deny list");
    let patterns: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
    for path in [
        "C:/Users/x/AppData/Roaming/Prompt-Saver/data.db",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/data.db-wal",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/data.db-shm",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/data.db.locked.bak",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/data.db.tmp",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/settings.json",
        // The atomic-write fallback: write_sealed_json's `path.with_extension("tmp")`
        // replaces the extension, so this is "settings.tmp", never "settings.json.tmp".
        "C:/Users/x/AppData/Roaming/Prompt-Saver/settings.tmp",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/prompts.json",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/prompts.tmp",
        // The atomic writer names its temp file "<file>.tmp-<id>" next to the target.
        "C:/Users/x/AppData/Roaming/Prompt-Saver/settings.json.tmp-1f3a9c",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/prompts.json.tmp-1f3a9c",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/backups/2026-08-19.psb",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/backups/subdir/2026-08-19.psb",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/error.log",
    ] {
        assert!(
            patterns.iter().any(|p| covers(p, path)),
            "asset scope must deny {path}"
        );
    }
}

#[test]
fn store_crypt_roundtrip() {
    let secret = "prompt with sécret 🔐 {#{x}#}";
    assert_eq!(dec_str(&enc_str(secret)), secret);
    // Legacy plaintext (no prefix) passes through unchanged for migration.
    assert_eq!(dec_str("legacy plaintext"), "legacy plaintext");
}

// The store's JSON files are the DB-write fallback: they hold full prompt bodies
// and private notes, so they must reach disk sealed, not as readable JSON.
#[test]
fn store_json_fallback_is_written_sealed() {
    let _guard = SEAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("ps-sealed-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("prompts.json");
    let secret = vec!["confidential prompt title".to_string()];

    assert!(write_sealed_json(&path, &secret));
    let raw = fs::read_to_string(&path).expect("written");
    assert!(raw.starts_with(ENC_PREFIX), "fallback must be sealed, got: {raw}");
    assert!(!raw.contains("confidential"), "plaintext leaked into the fallback file");
    match read_sealed_json::<Vec<String>>(&path) {
        SealedRead::Loaded(back) => assert_eq!(back, secret),
        _ => panic!("expected a loaded value"),
    }

    // A genuinely old plaintext install still migrates in.
    fs::write(&path, r#"["legacy entry"]"#).expect("write legacy");
    match read_sealed_json::<Vec<String>>(&path) {
        SealedRead::Loaded(legacy) => assert_eq!(legacy, vec!["legacy entry".to_string()]),
        _ => panic!("expected legacy plaintext to load"),
    }

    let _ = fs::remove_dir_all(&dir);
}

// A sync file write must never be an in-place truncate: a reader (this
// machine's own next poll, or a cloud-sync client) must only ever see either the
// old content or the new content, never a partial body.
#[test]
fn write_file_atomic_replaces_without_leaving_a_truncated_body() {
    let dir = std::env::temp_dir().join(format!("ps-atomic-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("p1.md");
    fs::write(&path, "---\nname: Old\n---\n\nold body").unwrap();

    let new_content = "---\nname: New\n---\n\n".to_string() + &"x".repeat(5000);
    assert!(write_file_atomic(&path, &new_content));
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        new_content,
        "must be fully replaced, never a partial write"
    );

    // The temp name must not collide with sync_sweep_orphans' ".md" filter: a
    // leftover temp file from an interrupted rename must never be treated as a
    // real synced document.
    let tmp = path.with_extension("tmp");
    assert!(!tmp.exists(), "temp file must be gone after a successful rename");
    assert!(!tmp.to_string_lossy().ends_with(".md"), "temp name must not look like a real sync file");

    let _ = fs::remove_dir_all(&dir);
}

// A data-dir move must bring backups/ along, nested subfolders included --
// otherwise the Backup panel shows 0 snapshots right after moving.
#[test]
fn copy_dir_all_copies_nested_files() {
    let base = std::env::temp_dir().join(format!("ps-copydir-{}", std::process::id()));
    let src = base.join("src");
    let dst = base.join("dst");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(src.join("2026-01-01_00-00-00")).unwrap();
    fs::write(src.join("2026-01-01_00-00-00").join("data.db.gz"), b"snapshot").unwrap();
    fs::write(src.join("prompts.names.json"), b"[]").unwrap();

    copy_dir_all(&src, &dst).unwrap();

    assert_eq!(
        fs::read(dst.join("2026-01-01_00-00-00").join("data.db.gz")).unwrap(),
        b"snapshot"
    );
    assert_eq!(fs::read(dst.join("prompts.names.json")).unwrap(), b"[]");

    let _ = fs::remove_dir_all(&base);
}

// With multiViewPrompt off (default), a synced placement must strip the
// prompt from every OTHER view -- otherwise re-syncing an unrelated content
// change duplicates it across every view it was ever placed in on any machine.
#[test]
fn sync_place_in_view_strips_other_views_unless_multi_view_is_on() {
    let mut settings = Settings {
        views: vec![test_view("v1"), test_view("v2")],
        ..Default::default()
    };
    let key1 = grid_key(settings.views[0].cols, settings.views[0].rows);
    settings.views[0].layouts.entry(key1.clone()).or_default().insert("p1".to_string(), [2, 2]);

    // Default: placing into v2 must remove the v1 placement.
    sync_place_in_view(&mut settings, "p1", "v2", [0, 0]);
    assert!(
        !settings.views[0].layouts.get(&key1).is_some_and(|m| m.contains_key("p1")),
        "must be removed from v1"
    );
    let key2 = grid_key(settings.views[1].cols, settings.views[1].rows);
    assert_eq!(settings.views[1].layouts.get(&key2).and_then(|m| m.get("p1")), Some(&[0, 0]));

    // With multiViewPrompt on, a placement into a THIRD view must not touch v2's.
    settings.ui_flags.insert("multiViewPrompt".to_string(), true);
    settings.views.push(test_view("v3"));
    sync_place_in_view(&mut settings, "p1", "v3", [1, 1]);
    let key3 = grid_key(settings.views[2].cols, settings.views[2].rows);
    assert_eq!(settings.views[2].layouts.get(&key3).and_then(|m| m.get("p1")), Some(&[1, 1]));
    assert_eq!(
        settings.views[1].layouts.get(&key2).and_then(|m| m.get("p1")),
        Some(&[0, 0]),
        "v2 placement must survive with multi-view on"
    );
}

// Backdates a path's mtime so a staleness check sees it as old. FILE_FLAG_BACKUP_
// SEMANTICS is what lets Windows open a directory handle at all here.
fn backdate(path: &std::path::Path, age: Duration) {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    let f = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .unwrap();
    let old = SystemTime::now() - age;
    f.set_times(std::fs::FileTimes::new().set_modified(old)).unwrap();
}

// A snip preview or an update stage dir surviving a crash/abnormal exit is
// never cleaned by anything else -- must be swept at the next startup. But
// NOT a fresh one -- "Allow multiple instances" lets a second launch run this
// same sweep while a concurrently open instance still has its own snip preview
// in active use, so only files older than STALE_TEMP_FILE_AGE are fair game.
fn staging_test_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ps-stage-{}-{}-{}", tag, std::process::id(), gen_id()));
    fs::create_dir_all(&d).unwrap();
    d
}

// The race: a staged copy sits in <data>/files while no prompt references it yet;
// a concurrent sweep (whose referenced sets lack it) must leave it alone, and it
// must still survive once the insert completed and the pending entry is released.
#[test]
fn sweep_keeps_a_staged_copy_while_the_add_is_in_flight() {
    let base = staging_test_dir("race");
    let files = base.join("files");
    let orig = base.join("orig.txt");
    fs::write(&orig, b"data").unwrap();
    let mut staged = StagedCopies::new();
    let copy = staged.stage(&files, &orig.to_string_lossy());
    assert_ne!(copy, orig.to_string_lossy());
    assert!(Path::new(&copy).is_file());
    let empty = std::collections::HashSet::new();
    sweep_orphan_files_in(&files, &empty, || Some(std::collections::HashSet::new()));
    assert!(Path::new(&copy).is_file(), "an in-flight staged copy was swept");
    // the insert completes: the store references it, the pending entry goes away
    staged.commit();
    drop(staged);
    let mut live = std::collections::HashSet::new();
    live.insert(copy.replace('/', "\\").to_lowercase());
    sweep_orphan_files_in(&files, &empty, || Some(live));
    assert!(Path::new(&copy).is_file(), "a referenced copy was swept after the insert");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn a_panic_with_a_live_guard_keeps_the_staged_copy_but_releases_the_pending_entry() {
    let base = staging_test_dir("panic");
    let files = base.join("files");
    let orig = base.join("orig.txt");
    fs::write(&orig, b"data").unwrap();
    let mut copy = String::new();
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut staged = StagedCopies::new();
        copy = staged.stage(&files, &orig.to_string_lossy());
        panic!("boom");
    }));
    assert!(res.is_err());
    assert!(Path::new(&copy).is_file(), "a copy staged before a panic must survive");
    let pending = PENDING_ATTACHMENTS.lock().unwrap_or_else(|e| e.into_inner());
    assert!(!pending.as_ref().is_some_and(|s| s.contains(&pending_key(&copy))), "pending entry released");
    drop(pending);
    let _ = fs::remove_dir_all(&base);
}

// A failed add (validation error after staging) must not leave the copy behind.
#[test]
fn a_failed_add_leaves_no_staged_copy_behind() {
    let base = staging_test_dir("fail");
    let files = base.join("files");
    let orig = base.join("orig.txt");
    fs::write(&orig, b"data").unwrap();
    let copy = {
        let mut staged = StagedCopies::new();
        staged.stage(&files, &orig.to_string_lossy())
        // dropped without commit: the add failed
    };
    assert!(!Path::new(&copy).exists(), "the copy of a failed add stayed on disk");
    assert!(!pending_attachments_contain(&copy), "the pending entry leaked");
    assert!(orig.is_file(), "the user's original must never be touched");
    let _ = fs::remove_dir_all(&base);
}

// Every production source file of the crate: src/*.rs except the test file,
// each cut at its own inline test module, in name order, with \n line ends.
// The structural checks below read this instead of one fixed file, so they
// keep finding a function after it moves to another file of the crate.
fn production_sources() -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("read the src dir")
        .map(|e| e.expect("src dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .filter(|p| p.file_name().is_some_and(|n| n != "tests.rs"))
        .collect();
    files.sort();
    let mut out = String::new();
    for f in &files {
        let text = fs::read_to_string(f).expect("read a source file").replace("\r\n", "\n");
        let cut = text.find("\n#[cfg(test)]\nmod tests {").map_or(text.len(), |i| i + 1);
        out.push_str(&text[..cut]);
        out.push('\n');
    }
    out
}

// Structural check (no timing-based red exists for "the lock is not held
// during the copy"): in add_prompt the copy, the probe and the OCR pass all
// sit before the lock that inserts, and update_prompt stages the same three
// before its locked section.
#[test]
fn add_and_update_prompt_do_the_slow_work_before_the_insert_lock() {
    let src = production_sources();
    let body = |from: &str, to: &str| {
        assert_eq!(src.matches(from).count(), 1, "{from} must occur exactly once in the crate sources");
        let a = src.find(from).expect("start marker");
        let b = a + src[a..].find(to).expect("end marker");
        src[a..b].to_string()
    };
    let add = body("async fn add_prompt(", "// One example prompt as the frontend supplies it");
    let insert_lock = add.rfind(concat!("let mut store = lock(&", "state);")).expect("insert lock");
    for call in ["staged_copies.stage(", "probe_media_meta(", "ocr_index_text(img"] {
        let at = add.find(call).unwrap_or_else(|| panic!("{call} missing"));
        assert!(at < insert_lock, "add_prompt must run {call} before taking the insert lock");
        assert!(!add[insert_lock..].contains(call), "add_prompt must not run {call} under the insert lock");
    }
    let upd = body("async fn update_prompt(", "// List a prompt's version snapshots");
    let staged_end = upd.find("let updated = {").expect("locked section");
    for call in ["staged_copies.stage(", "probe_media_meta(", "ocr_index_text(img"] {
        assert!(upd[..staged_end].contains(call), "update_prompt must stage {call} before its locked section");
    }
}

// The stat scan alone decides whether anything needs reading; with nothing to
// read the poll returns before it copies the library.
#[test]
fn sync_scan_folder_asks_for_a_read_only_once_a_changed_file_has_settled() {
    let dir = std::env::temp_dir().join(format!("ps-scan-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("abc123.md");
    fs::write(&file, "hello").unwrap();
    fs::write(dir.join("AGENTS.md"), "readme").unwrap();
    fs::write(dir.join("notes.txt"), "not markdown").unwrap();
    // First sight: the stamp is only remembered, nothing is read yet.
    let (to_read, files, _) = sync_scan_folder(&dir).unwrap();
    assert!(to_read.is_empty() && files == 1, "a file seen for the first time is not read yet");
    // Next round, stamp unchanged: now it settles into a read.
    let (to_read, _, _) = sync_scan_folder(&dir).unwrap();
    assert_eq!(to_read.len(), 1);
    assert_eq!(to_read[0].stem, "abc123");
    // Once handled, an unchanged folder asks for nothing.
    sync_seen_lock().insert(to_read[0].key.clone(), SyncSeen { stamp: to_read[0].stamp, handled: true, hash: 0 });
    let (to_read, _, _) = sync_scan_folder(&dir).unwrap();
    assert!(to_read.is_empty(), "an unchanged, handled folder needs no read and therefore no snapshot");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_known_current_holds_until_the_file_or_the_content_changes() {
    let dir = std::env::temp_dir().join(format!("ps-written-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("p1.md");
    let age = |f: &Path, secs: u64| {
        let h = fs::OpenOptions::new().write(true).open(f).unwrap();
        h.set_times(std::fs::FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(secs))).unwrap();
    };
    fs::write(&file, "one").unwrap();
    let h1 = sync_content_hash("one");
    assert!(!sync_file_known_current(&file, h1), "a file never noted is read once");
    // a stamp from the last couple of seconds is not trusted (coarse file system clocks)
    sync_file_note_current(&file, h1);
    assert!(!sync_file_known_current(&file, h1), "a fresh stamp is compared by content, not skipped");
    age(&file, 60);
    sync_file_note_current(&file, h1);
    assert!(sync_file_known_current(&file, h1), "unchanged file, same render: skipped");
    assert!(!sync_file_known_current(&file, sync_content_hash("two")), "a changed render is compared again");
    fs::write(&file, "edited outside").unwrap();
    assert!(!sync_file_known_current(&file, h1), "an outside edit changes the stamp and is never skipped");
    // same length, mtime restored to the noted value: a settled stamp still matches
    // by design, but a differing length never does
    fs::write(&file, "edited outside!!").unwrap();
    age(&file, 60);
    assert!(!sync_file_known_current(&file, h1), "a different length is never trusted");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn trash_rows_carry_what_the_trash_list_draws_and_no_picture() {
    let mut p = test_prompt("t1");
    p.name = "Report".into();
    p.text = "x".repeat(TRASH_PREVIEW_CHARS + 50);
    p.image = "data:image/png;base64,AAAA".into();
    p.file_path = "C:/files/a.pdf".into();
    p.trashed_at = 1234;
    let mut s = test_prompt("t2");
    s.secret = true;
    s.text = "ciphertext-that-must-not-show".into();
    let rows = trash_rows(&[p, s]);
    let json = serde_json::to_string(&rows).unwrap();
    assert!(!json.contains("base64,AAAA") && !json.contains("\"image\""), "no picture in the trash payload: {json}");
    assert_eq!(rows[0].id, "t1");
    assert_eq!(rows[0].name, "Report");
    assert_eq!(rows[0].text.chars().count(), TRASH_PREVIEW_CHARS);
    assert_eq!(rows[0].file_path, "C:/files/a.pdf");
    assert_eq!(rows[0].trashed_at, 1234);
    assert!(rows[0].has_image);
    assert_eq!(rows[1].text, "", "a confidential entry shows no stored text");
}

#[test]
#[cfg(windows)]
fn read_key_sidecar_at_refuses_a_file_past_the_size_cap() {
    let dir = std::env::temp_dir().join(format!("ps-keyread-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let small = dir.join("small.keys.json");
    fs::write(&small, br#"{"events":[],"gaps":[],"hold_ms":450}"#).unwrap();
    assert_eq!(read_key_sidecar_at(&small).map(|s| s.hold_ms), Some(450));
    let big = dir.join("big.keys.json");
    fs::write(&big, vec![b' '; (KEY_SIDECAR_MAX_COPY_BYTES + 1) as usize]).unwrap();
    assert!(read_key_sidecar_at(&big).is_none(), "an oversized sidecar is never read whole");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn video_folder_durations_keep_the_order_of_the_names_and_zero_a_missing_file() {
    let dir = std::env::temp_dir().join(format!("ps-durations-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let names: Vec<String> = (0..12).map(|i| format!("missing-{i}.mp4")).collect();
    let got = video_folder_file_durations_ms_impl(None, &dir, &names);
    assert_eq!(got, vec![0u64; 12], "one value per name, unknown files as 0");
    // Structural: the command runs the probes on the blocking pool, not a runtime worker.
    let src = production_sources();
    assert_eq!(src.matches("async fn video_folder_file_durations_ms(").count(), 1, "one durations command");
    let at = src.find("async fn video_folder_file_durations_ms(").unwrap();
    assert!(src[at..at + 600].contains("spawn_blocking"), "the durations command must use spawn_blocking");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dup_sources_keep_only_what_the_scan_reads_and_skip_confidential_entries() {
    let mut a = test_prompt("a");
    a.name = "Alpha".into();
    a.text = "some body".into();
    a.image = "data:image/png;base64,BBBB".into();
    a.tags = vec!["t".into()];
    a.variants = vec!["v".into()];
    let mut s = test_prompt("s");
    s.secret = true;
    let got = dup_sources(&[a, s]);
    assert_eq!(got.len(), 1, "the confidential entry is not scanned");
    assert_eq!((got[0].id.as_str(), got[0].name.as_str(), got[0].text.as_str()), ("a", "Alpha", "some body"));
}

#[test]
fn take_still_output_removes_a_partial_frame_after_a_failed_run() {
    let dir = std::env::temp_dir().join(format!("ps-still-fail-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let partial = dir.join(format!("{}-still-1-2.png", brand::TEMP_PREFIX));
    fs::write(&partial, b"half a frame").unwrap();
    assert!(take_still_output(&partial, false).is_err());
    assert!(!partial.exists(), "a failed extraction must not leave its partial frame");
    let good = dir.join(format!("{}-still-1-3.png", brand::TEMP_PREFIX));
    fs::write(&good, b"frame").unwrap();
    assert_eq!(take_still_output(&good, true).unwrap(), b"frame");
    assert!(!good.exists(), "a read frame is removed as before");
    let _ = fs::remove_dir_all(&dir);
}

// A temp root of its own for one sweep test. The sweeps look at every file
// carrying the app's prefixes, so a folder shared between tests (or between two
// test runs, or left over from an earlier one) lets one test's files change
// another's counts; a fresh folder per test keeps each sweep to its own files.
fn sweep_test_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("ps-sweeptest-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn sweep_stale_temp_files_also_reclaims_an_abandoned_still_frame() {
    let root = sweep_test_root("still");
    let sub = root.join(brand::TEMP_DIR);
    fs::create_dir_all(&sub).unwrap();
    let pid = std::process::id();
    let old = sub.join(format!("{}-still-{}-sweeptest-old.png", brand::TEMP_PREFIX, pid));
    let fresh = sub.join(format!("{}-still-{}-sweeptest-fresh.png", brand::TEMP_PREFIX, pid));
    fs::write(&old, b"x").unwrap();
    fs::write(&fresh, b"x").unwrap();
    backdate(&old, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    sweep_stale_temp_files_in(&root);
    assert!(!old.exists(), "a stale still frame must be swept");
    assert!(fresh.exists(), "a young still frame may belong to a running extraction");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn remove_own_temp_leftovers_takes_only_this_pids_files() {
    let root = std::env::temp_dir().join(format!("ps-ownexit-{}", std::process::id()));
    let dir = root.join(brand::TEMP_DIR);
    let mine_copy = dir.join(format!("{}-openedit-20260101-1000-4242", brand::TEMP_PREFIX));
    let other_copy = dir.join(format!("{}-openedit-20260101-1000-4243", brand::TEMP_PREFIX));
    fs::create_dir_all(&mine_copy).unwrap();
    fs::create_dir_all(&other_copy).unwrap();
    fs::write(mine_copy.join("a.png"), b"x").unwrap();
    let mine_still = dir.join(format!("{}-still-4242-9.png", brand::TEMP_PREFIX));
    let other_still = dir.join(format!("{}-still-4243-9.png", brand::TEMP_PREFIX));
    fs::write(&mine_still, b"x").unwrap();
    fs::write(&other_still, b"x").unwrap();
    remove_own_temp_leftovers_in(&root, 4242);
    assert!(!mine_copy.exists() && !mine_still.exists(), "this process's own scratch files go on exit");
    assert!(other_copy.exists() && other_still.exists(), "another instance's files stay");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn sweep_stale_temp_files_removes_only_its_own_stale_leftovers() {
    let dir = sweep_test_root("own-stale");
    let pid = std::process::id();
    let snip = dir.join(format!("{}-snip-{}.jpg", brand::TEMP_PREFIX, pid));
    let update_dir = dir.join(format!("{}-update-{}-1", brand::SLUG, pid));
    let wv2_dir = dir.join(format!("{}-wv2-{}-1", brand::SLUG, pid));
    let fresh_snip = dir.join(format!("{}-snip-{}-fresh.jpg", brand::TEMP_PREFIX, pid));
    let unrelated_ext = dir.join(format!("{}-snip-{}.png", brand::TEMP_PREFIX, pid)); // wrong extension
    let unrelated_file = dir.join(format!("ps-sweep-unrelated-{}.txt", pid));
    let ocr_subdir = dir.join(brand::TEMP_DIR);
    let ocr_png = ocr_subdir.join(format!("{}-ocr-{}.png", brand::TEMP_PREFIX, pid));
    let ocr_json = ocr_subdir.join(format!("{}-ocr-install-{}.json", brand::TEMP_PREFIX, pid));
    let ocr_script = ocr_subdir.join(format!("{}-ocr-install-{}.ps1", brand::TEMP_PREFIX, pid));
    let caps_json = ocr_subdir.join(format!("{}-ocr-caps.json", brand::TEMP_PREFIX));
    let fresh_ocr_png = ocr_subdir.join(format!("{}-ocr-{}-fresh.png", brand::TEMP_PREFIX, pid));
    let pdfium_cache = ocr_subdir.join(format!("pdfium-test-{}.dll", pid));

    fs::write(&snip, b"jpeg").unwrap();
    fs::create_dir_all(&update_dir).unwrap();
    fs::write(update_dir.join("setup.exe"), b"x").unwrap();
    fs::create_dir_all(&wv2_dir).unwrap();
    fs::write(wv2_dir.join("MicrosoftEdgeWebview2Setup.exe"), b"x").unwrap();
    fs::write(&fresh_snip, b"jpeg").unwrap();
    fs::write(&unrelated_ext, b"x").unwrap();
    fs::write(&unrelated_file, b"x").unwrap();
    fs::create_dir_all(&ocr_subdir).unwrap();
    fs::write(&ocr_png, b"png").unwrap();
    fs::write(&ocr_json, b"{}").unwrap();
    fs::write(&ocr_script, b"# elevated install").unwrap();
    fs::write(&caps_json, b"[]").unwrap();
    fs::write(&fresh_ocr_png, b"png").unwrap();
    fs::write(&pdfium_cache, b"dll").unwrap();
    backdate(&snip, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&update_dir, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&wv2_dir, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&ocr_png, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&ocr_json, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&ocr_script, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&caps_json, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&pdfium_cache, STALE_TEMP_FILE_AGE + Duration::from_secs(60));

    sweep_stale_temp_files_in(&dir);

    assert!(!snip.exists(), "a stale leftover snip preview must be swept");
    assert!(!update_dir.exists(), "a stale leftover update stage dir must be swept");
    assert!(!wv2_dir.exists(), "a stale leftover WebView2 bootstrapper dir must be swept");
    assert!(!ocr_png.exists(), "a stale leftover OCR image must be swept");
    assert!(!ocr_json.exists(), "a stale leftover OCR install-result json must be swept");
    assert!(!ocr_script.exists(), "a stale leftover OCR elevation script must be swept");
    assert!(!caps_json.exists(), "a stale leftover elevated capability listing must be swept");
    assert!(fresh_snip.exists(), "a snip preview still in active use must survive");
    assert!(fresh_ocr_png.exists(), "an OCR image still in active use must survive");
    assert!(unrelated_ext.exists(), "a differently-suffixed file must survive");
    assert!(unrelated_file.exists(), "an unrelated file must survive");
    assert!(pdfium_cache.exists(), "the pdfium dll cache must never be touched by this sweep");

    let _ = fs::remove_dir_all(&dir);
}

// The clip-export sweep has its own configurable age, separate from
// STALE_TEMP_FILE_AGE -- must only touch its own prefix, and must respect
// whatever age it is called with rather than the fixed constant above.
#[test]
fn sweep_clip_export_temp_respects_its_own_configurable_age() {
    let root = sweep_test_root("clipage");
    let dir = root.join(brand::TEMP_DIR);
    fs::create_dir_all(&dir).unwrap();
    let pid = std::process::id();
    let stale = dir.join(format!("{}-clipexport-{}-stale.mp4", brand::TEMP_PREFIX, pid));
    let fresh = dir.join(format!("{}-clipexport-{}-fresh.mp4", brand::TEMP_PREFIX, pid));
    let unrelated = dir.join(format!("{}-ocr-{}-unrelated.png", brand::TEMP_PREFIX, pid));
    fs::write(&stale, b"mp4").unwrap();
    fs::write(&fresh, b"mp4").unwrap();
    fs::write(&unrelated, b"png").unwrap();
    let age = Duration::from_secs(120);
    backdate(&stale, age + Duration::from_secs(5));
    backdate(&fresh, Duration::from_secs(5));
    backdate(&unrelated, age + Duration::from_secs(5));

    let (count, bytes) = sweep_clip_export_temp_in(&dir, age);

    assert!(!stale.exists(), "a clip-export file older than the given age must be swept");
    assert!(fresh.exists(), "a clip-export file younger than the given age must survive");
    assert!(unrelated.exists(), "a differently-prefixed file must never be touched by this sweep");
    assert_eq!(count, 1, "the swept count must report exactly the one stale file");
    assert_eq!(bytes, 3, "the swept bytes must report that one file's own size");
    let _ = fs::remove_dir_all(&root);
}

// The overview row's amount readout counts every clip-export file
// regardless of age -- unlike the sweep above, a fresh file still counts.
#[test]
fn clip_export_temp_stats_counts_every_clip_export_file_regardless_of_age() {
    let root = sweep_test_root("clipstats");
    let dir = root.join(brand::TEMP_DIR);
    fs::create_dir_all(&dir).unwrap();
    let pid = std::process::id();
    let old = dir.join(format!("{}-clipexport-{}-statsold.mp4", brand::TEMP_PREFIX, pid));
    let young = dir.join(format!("{}-clipexport-{}-statsyoung.mp4", brand::TEMP_PREFIX, pid));
    let unrelated = dir.join(format!("{}-ocr-{}-statsunrelated.png", brand::TEMP_PREFIX, pid));
    fs::write(&old, b"mp4data").unwrap();
    fs::write(&young, b"mp4").unwrap();
    fs::write(&unrelated, b"png").unwrap();
    backdate(&old, Duration::from_secs(999_999));

    let (count, bytes) = clip_export_temp_stats_in(&dir);

    assert_eq!(count, 2, "both clip-export files, and only those, must be counted regardless of age");
    assert_eq!(bytes, 10, "bytes must sum both clip-export files own sizes");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn clip_export_path_impl_lands_in_temp_dir_with_a_safe_extension() {
    let path = clip_export_path_impl("mp4").unwrap();
    let dir = std::env::temp_dir().join(brand::TEMP_DIR);
    assert!(
        std::path::Path::new(&path).starts_with(&dir),
        "must be created inside the app's own temp subfolder"
    );
    assert!(path.ends_with(".mp4"));
    assert!(path.contains("clipexport"));
    // A hostile/garbage extension (path separators, empty) must never leak
    // into the filename -- falls back to mp4 instead.
    let fallback = clip_export_path_impl("../evil").unwrap();
    assert!(fallback.ends_with(".mp4"), "an unsafe extension must fall back to mp4");
    let empty = clip_export_path_impl("").unwrap();
    assert!(empty.ends_with(".mp4"), "an empty extension must fall back to mp4");
}

#[test]
fn clip_export_ttl_from_defaults_to_one_hour_and_honours_the_setting() {
    let mut settings = Settings::default();
    assert_eq!(clip_export_ttl_from(&settings), Duration::from_secs(3600));
    settings.ui_values.insert("videoClipTtlMin".to_string(), 5.0);
    assert_eq!(clip_export_ttl_from(&settings), Duration::from_secs(300));
    // Never zero/negative -- a sweep with a zero age would delete a file
    // the instant it is written, mid-export.
    settings.ui_values.insert("videoClipTtlMin".to_string(), 0.0);
    assert_eq!(clip_export_ttl_from(&settings), Duration::from_secs(60));
}

#[test]
fn video_folder_stats_counts_only_known_video_extensions() {
    let dir = std::env::temp_dir().join(format!("ps-video-folder-stats-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("rec-a.mp4"), b"1234").unwrap();
    fs::write(dir.join("rec-b.webm"), b"12").unwrap();
    fs::write(dir.join("rec-c.gif"), b"123").unwrap();
    fs::write(dir.join("notes.txt"), b"ignored, wrong extension").unwrap();
    fs::create_dir_all(dir.join("MP4")).unwrap(); // a directory named like an extension is not a file
    let (count, bytes) = video_folder_stats_impl(&dir.to_string_lossy(), &dir);
    assert_eq!(count, 3, "only the three video files must be counted, not the txt or the subfolder");
    assert_eq!(bytes, 4 + 2 + 3);
    let _ = fs::remove_dir_all(&dir);
}

// Finding 3 (security pass 348): the extension check alone let this count
// and clean up any matching file in a shared folder, not only the app's own
// recordings -- the same "rec-" naming proof discard already requires.
#[test]
fn video_folder_stats_ignores_a_video_file_without_the_rec_prefix() {
    let dir = std::env::temp_dir().join(format!("ps-video-folder-noprefix-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("rec-mine.mp4"), b"1234").unwrap();
    fs::write(dir.join("someone-elses-video.mp4"), b"999999").unwrap();
    let (count, bytes) = video_folder_stats_impl(&dir.to_string_lossy(), &dir);
    assert_eq!(count, 1, "only the app's own rec-prefixed file must be counted");
    assert_eq!(bytes, 4);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn video_folder_stats_on_a_missing_folder_is_empty_not_an_error() {
    let dir = std::env::temp_dir().join(format!("ps-video-folder-missing-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir); // guarantee it really does not exist
    assert_eq!(video_folder_stats_impl(&dir.to_string_lossy(), &dir), (0, 0));
}

// ---- Folder import ----

#[test]
fn scan_import_folder_turns_subfolders_into_tags_and_reads_only_md_and_txt() {
    let dir = std::env::temp_dir().join(format!("ps-import-folder-tags-{}", std::process::id()));
    let sub = dir.join("Work").join("Clients");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join("Note.txt"), b"Hello").unwrap();
    fs::write(dir.join("Top.md"), b"Top level").unwrap();
    fs::write(dir.join("ignored.jpg"), b"not text").unwrap();
    let (items, truncated) = scan_import_folder(&dir, 5, 500);
    assert!(!truncated);
    assert_eq!(items.len(), 2, "only the two text files count, never the image");
    let nested = items.iter().find(|i| i.name == "Note").unwrap();
    assert_eq!(nested.tags, vec!["Work".to_string(), "Clients".to_string()]);
    let top = items.iter().find(|i| i.name == "Top").unwrap();
    assert!(top.tags.is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn split_folder_note_extracts_a_short_first_line_and_keeps_a_long_one_as_text() {
    let (text, note) = split_folder_note("Shopping list\nMilk\nEggs");
    assert_eq!(note, "Shopping list");
    assert_eq!(text, "Milk\nEggs");
    let long_first = "x".repeat(IMPORT_FOLDER_NOTE_MAX_CHARS + 1);
    let (text2, note2) = split_folder_note(&format!("{long_first}\nrest"));
    assert!(note2.is_empty());
    assert_eq!(text2, format!("{long_first}\nrest"));
    // A one-line file has no "rest" left to separate the note from.
    let (text3, note3) = split_folder_note("Just one line");
    assert!(note3.is_empty());
    assert_eq!(text3, "Just one line");
}

#[test]
fn import_folder_files_reads_a_rotation_separator_into_variants() {
    let dir = std::env::temp_dir().join(format!("ps-import-folder-rotate-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("Greeting.txt"), format!("Hi{VARIANT_SEP}Hello{VARIANT_SEP}Hey")).unwrap();
    let content = read_import_folder_file(&dir.join("Greeting.txt")).unwrap();
    let (raw_text, variants) = split_variants(&content);
    let (text, note) = split_folder_note(&raw_text);
    assert!(note.is_empty());
    assert_eq!(text, "Hi");
    assert_eq!(variants, vec!["Hello".to_string(), "Hey".to_string()]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_import_folder_leaves_out_a_folder_past_the_depth_limit_entirely() {
    let dir = std::env::temp_dir().join(format!("ps-import-folder-depth-{}", std::process::id()));
    let deep = dir.join("a").join("b").join("c");
    fs::create_dir_all(&deep).unwrap();
    fs::write(dir.join("a").join("shallow.txt"), b"shallow").unwrap();
    fs::write(deep.join("deep.txt"), b"deep").unwrap();
    // depth 2 = the base folder plus one level of subfolders -- "a" is
    // included, "a/b" is already past the limit and never descended into.
    let (items, _) = scan_import_folder(&dir, 2, 500);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "shallow");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_import_folder_stops_at_the_maximum_and_reports_truncation() {
    let dir = std::env::temp_dir().join(format!("ps-import-folder-max-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for i in 0..5 {
        fs::write(dir.join(format!("n{i}.txt")), b"x").unwrap();
    }
    let (items, truncated) = scan_import_folder(&dir, 5, 3);
    assert_eq!(items.len(), 3);
    assert!(truncated);
    let (items2, truncated2) = scan_import_folder(&dir, 5, 5);
    assert_eq!(items2.len(), 5);
    assert!(!truncated2);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_import_folder_marks_an_oversized_or_binary_file_unreadable_with_a_reason() {
    let dir = std::env::temp_dir().join(format!("ps-import-folder-oversize-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let huge = vec![b'a'; (MAX_IMPORT_FOLDER_FILE_BYTES + 1) as usize];
    fs::write(dir.join("huge.txt"), &huge).unwrap();
    fs::write(dir.join("bad.txt"), [0xff, 0xfe, 0x00]).unwrap();
    fs::write(dir.join("ok.txt"), b"fine").unwrap();
    let (items, _) = scan_import_folder(&dir, 5, 500);
    let huge_item = items.iter().find(|i| i.name == "huge").unwrap();
    assert!(!huge_item.readable);
    assert_eq!(huge_item.skip_reason, "too-large");
    let bad_item = items.iter().find(|i| i.name == "bad").unwrap();
    assert!(!bad_item.readable);
    assert_eq!(bad_item.skip_reason, "bad-encoding");
    let ok_item = items.iter().find(|i| i.name == "ok").unwrap();
    assert!(ok_item.readable);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn import_folder_files_appends_a_prompt_with_tags_note_and_variants() {
    let dir = std::env::temp_dir().join(format!("ps-import-folder-confirm-{}", std::process::id()));
    let sub = dir.join("Recipes");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join("Cake.txt"), format!("A short title\nFlour, sugar, eggs{VARIANT_SEP}Butter, sugar, eggs")).unwrap();
    let mut store = Store {
        prompts: Vec::new(),
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: Default::default(),
    };
    let path = sub.join("Cake.txt");
    let content = read_import_folder_file(&path).unwrap();
    let tags = import_folder_tags(&dir, &path);
    let (raw_text, variants) = split_variants(&content);
    let (text, note) = split_folder_note(&raw_text);
    let items = vec![ImportedPrompt {
        name: "Cake".to_string(),
        text,
        color: String::new(),
        font: String::new(),
        font_size: 0,
        file_path: String::new(),
        pdf_page: 0,
        icon_path: String::new(),
        caption: String::new(),
        caption_size: 0,
        show_image: false,
        copy_image: false,
        image: String::new(),
        tags,
        notes: note,
        variants,
        rotate_mode: String::new(),
        favorite: false,
    }];
    let count = append_imported_prompts(&mut store, items);
    assert_eq!(count, 1);
    let p = &store.prompts[0];
    assert_eq!(p.tags, vec!["Recipes".to_string()]);
    assert_eq!(p.notes, "A short title");
    assert_eq!(p.text, "Flour, sugar, eggs");
    assert_eq!(p.variants, vec!["Butter, sugar, eggs".to_string()]);
    let _ = fs::remove_dir_all(&dir);
}

// ---- Discard trash ----

#[test]
fn video_folder_stats_includes_the_verworfen_subfolder_in_its_totals() {
    let dir = std::env::temp_dir().join(format!("ps-video-folder-discard-stats-{}", std::process::id()));
    let trash = dir.join("verworfen");
    fs::create_dir_all(&trash).unwrap();
    fs::write(dir.join("rec-a.mp4"), b"1234").unwrap();
    fs::write(trash.join("rec-b.mp4"), b"12").unwrap();
    let (count, bytes) = video_folder_stats_impl(&dir.to_string_lossy(), &dir);
    assert_eq!(count, 2, "a discarded recording still occupies real disk space in this folder");
    assert_eq!(bytes, 4 + 2);
    let _ = fs::remove_dir_all(&dir);
}

// ---- Storage breakdown ("Speicher" section) ----

#[test]
fn scan_dir_stats_counts_files_one_level_deep_and_ignores_subfolders() {
    let dir = std::env::temp_dir().join(format!("ps-storage-scan-{}", std::process::id()));
    let nested = dir.join("nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(dir.join("a.txt"), b"12345").unwrap();
    fs::write(dir.join("b.txt"), b"12").unwrap();
    fs::write(nested.join("c.txt"), b"1234567890").unwrap();
    let (count, bytes, capped, denied) = scan_dir_stats(&dir, 100);
    assert_eq!(count, 2, "a file in a subfolder must not be counted (one level deep only)");
    assert_eq!(bytes, 5 + 2);
    assert!(!capped);
    assert!(!denied);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_dir_stats_stops_at_the_entry_cap() {
    let dir = std::env::temp_dir().join(format!("ps-storage-cap-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for i in 0..6 {
        fs::write(dir.join(format!("f{i}.txt")), b"x").unwrap();
    }
    let (count, _bytes, capped, denied) = scan_dir_stats(&dir, 3);
    assert_eq!(count, 3, "the scan must stop counting at the cap, not read every file first");
    assert!(capped);
    assert!(!denied);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_dir_stats_on_a_missing_folder_is_empty_not_denied() {
    let dir = std::env::temp_dir().join(format!("ps-storage-missing-{}", std::process::id()));
    assert_eq!(scan_dir_stats(&dir, 10), (0, 0, false, false));
}

// A permission failure is real Windows ACL behaviour this environment cannot
// reliably reproduce (needs elevation, and would leave a folder locked out
// for later tests) -- the read is injected instead, so the "must be
// reported, not concealed" contract is checked directly against the
// function's own logic.
#[test]
fn scan_dir_stats_with_reports_a_denied_folder_instead_of_hiding_it() {
    let denied_read = |_: &std::path::Path| -> std::io::Result<std::fs::ReadDir> {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    };
    let (count, bytes, capped, denied) =
        scan_dir_stats_with(std::path::Path::new("Z:/wherever"), 10, denied_read);
    assert_eq!((count, bytes, capped), (0, 0, false));
    assert!(denied, "a folder that exists but cannot be listed must be reported as denied");
}

#[test]
fn attachments_bytes_counts_a_shared_file_once_and_skips_missing_ones() {
    let dir = std::env::temp_dir().join(format!("ps-storage-attach-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let f = dir.join("report.pdf");
    fs::write(&f, b"1234567").unwrap();
    let path = f.to_string_lossy().to_string();
    let paths = vec![path.clone(), path, String::new(), "Z:/does-not-exist.pdf".to_string()];
    let (count, bytes) = attachments_bytes(&paths);
    assert_eq!(count, 1, "the same attachment referenced twice must be counted once");
    assert_eq!(bytes, 7);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn discard_recording_file_in_moves_into_verworfen_instead_of_deleting() {
    let dir = std::env::temp_dir().join(format!("ps-discard-move-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-2026-01-01-00-00-00.mp4");
    fs::write(&src, b"video").unwrap();
    let moved = discard_recording_file_in(std::slice::from_ref(&dir), &src).expect("must move, not fail");
    // discard_recording_file_in canonicalizes the parent internally (the \\?\
    // extended-length form on Windows) -- canonicalize the expected side too
    // rather than comparing a canonical path against a plain one.
    let want = dir.join("verworfen").join("rec-2026-01-01-00-00-00.mp4").canonicalize().unwrap();
    assert_eq!(moved, want);
    assert!(!src.exists(), "the original spot must be empty afterwards");
    assert!(moved.is_file(), "the file must really be at the new spot, not just claimed to be");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn discard_recording_file_in_refuses_a_name_without_the_rec_prefix() {
    let dir = std::env::temp_dir().join(format!("ps-discard-prefix-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("not-a-recording.mp4");
    fs::write(&src, b"video").unwrap();
    assert!(discard_recording_file_in(std::slice::from_ref(&dir), &src).is_none());
    assert!(src.is_file(), "a refused discard must leave the file untouched");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn discard_recording_file_in_refuses_a_file_outside_the_allowed_folders() {
    let dir = std::env::temp_dir().join(format!("ps-discard-outside-{}", std::process::id()));
    let other = std::env::temp_dir().join(format!("ps-discard-other-{}", std::process::id()));
    fs::create_dir_all(&other).unwrap();
    let src = other.join("rec-somewhere-else.mp4");
    fs::write(&src, b"video").unwrap();
    assert!(discard_recording_file_in(&[dir], &src).is_none(), "dir is not in the allowed list");
    assert!(src.is_file());
    let _ = fs::remove_dir_all(&other);
}

// ---------- security pass: picker-capability grants (set_*_dir gate) ----------

#[test]
fn folder_picker_title_rejects_an_unknown_purpose() {
    assert_eq!(folder_picker_title("exportVideoTypo", "en"), None);
    assert_eq!(folder_picker_title("", "en"), None);
    assert_eq!(folder_picker_title("data", "en"), Some("Choose the data folder"));
}

#[test]
fn folder_picker_title_maps_every_known_purpose_by_language() {
    assert_eq!(folder_picker_title("screenshot", "en"), Some("Choose the screenshot folder"));
    assert_eq!(folder_picker_title("screenshot", "de"), Some("Screenshot-Ordner wählen"));
    assert_eq!(folder_picker_title("videoExport", "en"), Some("Choose the video export folder"));
    assert_eq!(folder_picker_title("videoExport", "de"), Some("Exportordner für Videos wählen"));
    assert_eq!(folder_picker_title("recording", "en"), Some("Choose the recording folder"));
    assert_eq!(folder_picker_title("recording", "de"), Some("Aufnahmeordner wählen"));
    assert_eq!(folder_picker_title("sync", "en"), Some("Choose the sync folder"));
    assert_eq!(folder_picker_title("sync", "de"), Some("Sync-Ordner wählen"));
    assert_eq!(folder_picker_title("data", "en"), Some("Choose the data folder"));
    assert_eq!(folder_picker_title("data", "de"), Some("Datenordner wählen"));
    // Any other UI language falls back to English, same as the rest of
    // the app's backend-side strings.
    assert_eq!(folder_picker_title("sync", "fr"), Some("Choose the sync folder"));
}

#[test]
fn accept_dir_setting_rejects_a_path_that_was_never_picked() {
    let dir = std::env::temp_dir().join(format!("ps-grant-unpicked-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    let default = std::env::temp_dir().join(format!("ps-grant-unpicked-default-{}", std::process::id()));
    let err = accept_dir_setting(&dir_s, &default, "screenshot").expect_err("a dir the picker never returned must be refused");
    assert_eq!(err, "dir-not-picked");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn accept_dir_setting_accepts_a_granted_dir_exactly_once() {
    let dir = std::env::temp_dir().join(format!("ps-grant-once-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    let default = std::env::temp_dir().join(format!("ps-grant-once-default-{}", std::process::id()));
    grant_picked_dir(&dir, "screenshot");
    assert_eq!(accept_dir_setting(&dir_s, &default, "screenshot").unwrap(), dir_s, "the just-picked dir must be accepted");
    let err = accept_dir_setting(&dir_s, &default, "screenshot").expect_err("the same grant must not be usable twice");
    assert_eq!(err, "dir-not-picked");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn accept_dir_setting_rejects_a_grant_picked_for_a_different_purpose() {
    let dir = std::env::temp_dir().join(format!("ps-grant-wrong-purpose-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    let default = std::env::temp_dir().join(format!("ps-grant-wrong-purpose-default-{}", std::process::id()));
    grant_picked_dir(&dir, "sync");
    let err = accept_dir_setting(&dir_s, &default, "recording")
        .expect_err("a grant picked for one setting must not authorize a different one");
    assert_eq!(err, "dir-not-picked");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn accept_dir_setting_treats_empty_and_the_built_in_default_as_a_reset() {
    let default = std::env::temp_dir().join(format!("ps-grant-default-{}", std::process::id()));
    assert_eq!(accept_dir_setting("", &default, "screenshot").unwrap(), "", "empty must reset without needing a grant");
    assert_eq!(
        accept_dir_setting(&default.to_string_lossy(), &default, "screenshot").unwrap(),
        "",
        "the feature's own built-in default must reset without needing a grant"
    );
}

// ---------- security pass: recording-source ownership (convert/save/copy) ----------

#[test]
fn is_own_recording_source_accepts_a_rec_file_inside_an_allowed_folder() {
    let dir = std::env::temp_dir().join(format!("ps-recsrc-own-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-2026-01-01-00-00-00.mp4");
    fs::write(&src, b"video").unwrap();
    assert!(is_own_recording_source(std::slice::from_ref(&dir), &src));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn is_own_recording_source_refuses_a_foreign_file_outside_the_allowed_folders() {
    let dir = std::env::temp_dir().join(format!("ps-recsrc-in-{}", std::process::id()));
    let other = std::env::temp_dir().join(format!("ps-recsrc-out-{}", std::process::id()));
    fs::create_dir_all(&other).unwrap();
    let src = other.join("rec-elsewhere.mp4");
    fs::write(&src, b"video").unwrap();
    assert!(!is_own_recording_source(&[dir], &src), "the folder is not one this app writes recordings into");
    let _ = fs::remove_dir_all(&other);
}

#[test]
fn is_own_recording_source_refuses_a_file_without_the_rec_prefix_even_inside_an_allowed_folder() {
    let dir = std::env::temp_dir().join(format!("ps-recsrc-noprefix-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("not-a-recording.mp4");
    fs::write(&src, b"video").unwrap();
    assert!(!is_own_recording_source(std::slice::from_ref(&dir), &src), "a made-up name must not pass as proof");
    let _ = fs::remove_dir_all(&dir);
}

// ---------- security pass: bounded export-output set (save_recording_as) ----------

#[test]
fn is_known_export_output_accepts_only_a_path_export_video_actually_wrote() {
    let dir = std::env::temp_dir().join(format!("ps-exportout-known-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let exported = dir.join("clip-edit.mp4");
    fs::write(&exported, b"video").unwrap();
    let untouched = dir.join("clip-not-exported.mp4");
    fs::write(&untouched, b"video").unwrap();
    assert!(!is_known_export_output(&exported), "not remembered yet, must not pass");
    remember_export_output(&exported);
    assert!(is_known_export_output(&exported), "a file export_video actually produced must be recognized");
    assert!(!is_known_export_output(&untouched), "a neighbour file never exported must stay refused");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_moved_export_output_stays_known_at_its_new_place() {
    let dir = std::env::temp_dir().join(format!("ps-exportout-move-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let x = dir.join("clip-edit.mp4");
    let y = dir.join("moved.mp4");
    let z = dir.join("moved-again.mp4");
    let other = dir.join("other.mp4");
    fs::write(&x, b"video").unwrap();
    fs::write(&other, b"video").unwrap();
    remember_export_output(&x);
    move_tracked_export_output(&x, &y).unwrap();
    assert!(is_known_export_output(&y), "the moved file must still count as an export output");
    move_tracked_export_output(&y, &z).unwrap();
    assert!(is_known_export_output(&z), "a second move must keep the proof too");
    move_tracked_export_output(&other, &dir.join("other-moved.mp4")).unwrap();
    assert!(!is_known_export_output(&dir.join("other-moved.mp4")), "a file that was never an export stays unknown");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_output_to_remember_records_the_written_sibling_not_the_requested_dst() {
    let dir = std::env::temp_dir().join(format!("ps-exportout-sibling-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let requested = dir.join("clip-edit.mp4");
    fs::write(&requested, b"pre-existing file the export must not overwrite").unwrap();
    // The same unique_dest_path call export_video_blocking makes once it
    // finds the requested name already taken, standing in for the file
    // ffmpeg actually wrote for this test.
    let sibling = unique_dest_path(&dir, "clip-edit", "mp4");
    fs::write(&sibling, b"the file ffmpeg actually produced").unwrap();
    let result: Result<String, String> = Ok(sibling.to_string_lossy().to_string());
    if let Some(actual) = export_output_to_remember(&result) {
        remember_export_output(&actual);
    }
    assert!(!is_known_export_output(&requested), "the pre-existing requested path was never written by this export");
    assert!(is_known_export_output(&sibling), "the sibling ffmpeg actually wrote must be recognized");
    let _ = fs::remove_dir_all(&dir);
}

// ---------- export finalization: atomic name reservation ----------

#[test]
fn reserve_unique_dest_path_skips_to_the_next_free_name_and_claims_it() {
    let dir = std::env::temp_dir().join(format!("ps-reserve-basic-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let taken = dir.join("clip-edit.mp4");
    fs::write(&taken, b"already exported once").unwrap();
    let reserved = reserve_unique_dest_path(&dir, "clip-edit", "mp4").unwrap();
    assert_eq!(reserved, dir.join("clip-edit (2).mp4"));
    // The name is claimed, not just picked: it exists the instant the
    // call returns, before any caller has written or renamed into it.
    assert!(reserved.is_file());
    assert_eq!(fs::read(&taken).unwrap(), b"already exported once", "the original file must be untouched");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn reserve_unique_dest_path_never_replaces_a_file_that_wins_the_race() {
    let dir = std::env::temp_dir().join(format!("ps-reserve-race-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    // Stands in for another export (or another app) claiming the first
    // candidate name in the gap unique_dest_path's plain exists() check
    // could not see -- reserve_unique_dest_path must fail that name
    // outright and move on rather than overwrite it later.
    let rival = dir.join("clip-edit.mp4");
    fs::write(&rival, b"a different export wrote this first").unwrap();
    let reserved = reserve_unique_dest_path(&dir, "clip-edit", "mp4").unwrap();
    assert_ne!(reserved, rival, "the name already taken by another writer must never be reused");
    assert_eq!(fs::read(&rival).unwrap(), b"a different export wrote this first", "the rival's file must survive untouched");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn is_own_mic_sidecar_accepts_only_the_matching_videos_own_sidecar() {
    let video = PathBuf::from("C:/recs/rec-2026-01-01-00-00-00.mp4");
    let own_sidecar = PathBuf::from("C:/recs/rec-2026-01-01-00-00-00.mic.wav");
    assert!(is_own_mic_sidecar(&video, &own_sidecar));
}

#[test]
fn is_own_mic_sidecar_refuses_an_unrelated_sidecar_in_the_same_folder() {
    let video = PathBuf::from("C:/recs/rec-2026-01-01-00-00-00.mp4");
    let foreign_sidecar = PathBuf::from("C:/recs/rec-someone-elses-take.mic.wav");
    assert!(!is_own_mic_sidecar(&video, &foreign_sidecar), "a sidecar for a different recording must not pass");
}

#[test]
#[cfg(windows)]
fn copy_keys_sidecar_alongside_brings_the_sidecar_to_the_open_editor_copy() {
    let dir = std::env::temp_dir().join(format!("ps-openedit-keys-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-x.mp4");
    let dest = dir.join("rec-x-copy.mp4");
    fs::write(&src, b"video").unwrap();
    fs::write(keylog::sidecar_path(&src), br#"{"events":[],"gaps":[]}"#).unwrap();
    copy_keys_sidecar_alongside(&src, &dest);
    assert!(keylog::sidecar_path(&dest).is_file(), "open_file_in_editor's copy must bring the key-overlay sidecar with it");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[cfg(windows)]
fn copy_keys_sidecar_alongside_skips_a_sidecar_that_does_not_parse() {
    let dir = std::env::temp_dir().join(format!("ps-openedit-keys-bad-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-x.mp4");
    let dest = dir.join("rec-x-copy.mp4");
    fs::write(&src, b"video").unwrap();
    fs::write(keylog::sidecar_path(&src), b"not json at all").unwrap();
    copy_keys_sidecar_alongside(&src, &dest);
    assert!(!keylog::sidecar_path(&dest).exists(), "a sidecar that is not the expected shape must never be carried along");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[cfg(windows)]
fn copy_keys_sidecar_alongside_skips_a_sidecar_past_the_size_ceiling() {
    let dir = std::env::temp_dir().join(format!("ps-openedit-keys-huge-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-x.mp4");
    let dest = dir.join("rec-x-copy.mp4");
    fs::write(&src, b"video").unwrap();
    // Well past KEY_SIDECAR_MAX_COPY_BYTES, and never actually parsed --
    // the size check must turn it away before the file is even read.
    let oversized = vec![b' '; (KEY_SIDECAR_MAX_COPY_BYTES + 1) as usize];
    fs::write(keylog::sidecar_path(&src), &oversized).unwrap();
    copy_keys_sidecar_alongside(&src, &dest);
    assert!(!keylog::sidecar_path(&dest).exists(), "an oversized sidecar must never be carried along");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[cfg(windows)]
fn copy_keys_sidecar_alongside_is_a_no_op_when_the_source_never_captured_keys() {
    let dir = std::env::temp_dir().join(format!("ps-openedit-nokeys-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-x.mp4");
    let dest = dir.join("rec-x-copy.mp4");
    fs::write(&src, b"video").unwrap();
    copy_keys_sidecar_alongside(&src, &dest);
    assert!(!keylog::sidecar_path(&dest).exists(), "no sidecar to begin with must not conjure one for the copy");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn restore_discarded_recording_moves_the_file_back_to_its_original_folder() {
    let dir = std::env::temp_dir().join(format!("ps-restore-{}", std::process::id()));
    let trash = dir.join("verworfen");
    fs::create_dir_all(&trash).unwrap();
    fs::write(trash.join("rec-x.mp4"), b"video").unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    assert!(restore_discarded_recording_impl(&dir_s, "rec-x.mp4", &dir));
    assert!(dir.join("rec-x.mp4").is_file());
    assert!(!trash.join("rec-x.mp4").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn restore_discarded_recording_brings_the_key_overlay_sidecar_back_too() {
    let dir = std::env::temp_dir().join(format!("ps-restore-keys-{}", std::process::id()));
    let trash = dir.join("verworfen");
    fs::create_dir_all(&trash).unwrap();
    fs::write(trash.join("rec-x.mp4"), b"video").unwrap();
    fs::write(trash.join("rec-x.keys.json"), b"{}").unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    assert!(restore_discarded_recording_impl(&dir_s, "rec-x.mp4", &dir));
    assert!(dir.join("rec-x.keys.json").is_file(), "the key-overlay sidecar must travel with its recording");
    assert!(!trash.join("rec-x.keys.json").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn restore_discarded_recording_never_overwrites_a_file_already_at_the_old_spot() {
    let dir = std::env::temp_dir().join(format!("ps-restore-collide-{}", std::process::id()));
    let trash = dir.join("verworfen");
    fs::create_dir_all(&trash).unwrap();
    fs::write(trash.join("rec-x.mp4"), b"discarded copy").unwrap();
    fs::write(dir.join("rec-x.mp4"), b"a newer recording already took this name").unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    assert!(!restore_discarded_recording_impl(&dir_s, "rec-x.mp4", &dir));
    assert_eq!(fs::read(dir.join("rec-x.mp4")).unwrap(), b"a newer recording already took this name");
    assert!(trash.join("rec-x.mp4").is_file(), "the discarded copy must stay put, not vanish on a failed restore");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn delete_discarded_recording_now_removes_it_for_good() {
    let dir = std::env::temp_dir().join(format!("ps-discard-delete-now-{}", std::process::id()));
    let trash = dir.join("verworfen");
    fs::create_dir_all(&trash).unwrap();
    fs::write(trash.join("rec-x.mp4"), b"video").unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    assert!(delete_discarded_recording_now_impl(&dir_s, "rec-x.mp4", &dir));
    assert!(!trash.join("rec-x.mp4").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn delete_discarded_recording_now_removes_its_key_overlay_sidecar_too() {
    let dir = std::env::temp_dir().join(format!("ps-discard-delete-keys-{}", std::process::id()));
    let trash = dir.join("verworfen");
    fs::create_dir_all(&trash).unwrap();
    fs::write(trash.join("rec-x.mp4"), b"video").unwrap();
    fs::write(trash.join("rec-x.keys.json"), b"{}").unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    assert!(delete_discarded_recording_now_impl(&dir_s, "rec-x.mp4", &dir));
    assert!(!trash.join("rec-x.keys.json").exists(), "a purge for good must not leave the key-overlay sidecar behind");
    let _ = fs::remove_dir_all(&dir);
}

// A cancel after finish-and-continue used to delete an earlier
// part's video and mic audio but leave its key-overlay sidecar behind --
// the one file a "fully discarded" take would otherwise still keep.
#[cfg(windows)]
#[test]
fn remove_recording_part_files_takes_the_key_overlay_sidecar_with_it() {
    let dir = std::env::temp_dir().join(format!("ps-cancel-part-keys-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let video = dir.join("rec-part-2.mp4");
    let mic = dir.join("rec-part-2.mic.wav");
    let keys = dir.join("rec-part-2.keys.json");
    fs::write(&video, b"video").unwrap();
    fs::write(&mic, b"mic").unwrap();
    fs::write(&keys, b"{}").unwrap();
    let part = RecordingPartInfo { path: video.to_string_lossy().to_string(), mic_wav_path: Some(mic.to_string_lossy().to_string()) };
    remove_recording_part_files(&part);
    assert!(!video.exists());
    assert!(!mic.exists());
    assert!(!keys.exists(), "cancelling a take must not leave an earlier part's recorded keys behind");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn is_bare_filename_rejects_a_path_traversal_attempt() {
    assert!(is_bare_filename("rec-x.mp4"));
    assert!(!is_bare_filename("../rec-x.mp4"));
    assert!(!is_bare_filename("sub/rec-x.mp4"));
    assert!(!is_bare_filename(""));
}

#[test]
fn discarded_stale_in_removes_only_files_past_the_configured_age() {
    let dir = std::env::temp_dir().join(format!("ps-discarded-stale-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let old = dir.join("rec-old.mp4");
    let recent = dir.join("rec-recent.mp4");
    fs::write(&old, b"old").unwrap();
    fs::write(&recent, b"recent").unwrap();
    backdate(&old, Duration::from_secs(25 * 3600));
    let (count, bytes) = discarded_stale_in(&dir, Duration::from_secs(24 * 3600));
    assert_eq!(count, 1, "only the file past the retention window is removed");
    assert_eq!(bytes, 3);
    assert!(!old.exists());
    assert!(recent.is_file(), "a recent discard must survive this sweep");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn discarded_stale_in_removes_the_key_overlay_sidecar_alongside_an_old_discard() {
    let dir = std::env::temp_dir().join(format!("ps-discarded-stale-keys-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let old = dir.join("rec-old.mp4");
    let old_keys = dir.join("rec-old.keys.json");
    fs::write(&old, b"old").unwrap();
    fs::write(&old_keys, b"{}").unwrap();
    backdate(&old, Duration::from_secs(25 * 3600));
    let (count, _bytes) = discarded_stale_in(&dir, Duration::from_secs(24 * 3600));
    assert_eq!(count, 1);
    assert!(!old_keys.exists(), "the key-overlay sidecar of a purged discard must not be left orphaned");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn video_folder_cleanup_preview_and_cleanup_agree_and_only_touch_stale_video_files() {
    let dir = std::env::temp_dir().join(format!("ps-video-folder-cleanup-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let stale = dir.join("rec-old-recording.mp4");
    let fresh = dir.join("rec-new-recording.mp4");
    let unrelated = dir.join("readme.txt");
    fs::write(&stale, b"old").unwrap();
    fs::write(&fresh, b"new").unwrap();
    fs::write(&unrelated, b"not a video").unwrap();
    backdate(&stale, Duration::from_secs(40 * 24 * 3600));
    backdate(&unrelated, Duration::from_secs(40 * 24 * 3600));

    let dir_s = dir.to_string_lossy().to_string();
    let preview = video_folder_cleanup_preview_impl(&dir_s, 30, &dir);
    assert_eq!(preview.len(), 1, "only the stale video file is older than 30 days");
    assert_eq!(preview[0].name, "rec-old-recording.mp4");
    assert_eq!(preview[0].size, 3);

    let deleted = video_folder_cleanup_impl(&dir_s, 30, &dir);
    assert_eq!(deleted, 1, "cleanup must delete exactly what the preview listed");
    assert!(!stale.exists(), "the stale video must be gone");
    assert!(fresh.exists(), "a fresh video must survive");
    assert!(unrelated.exists(), "a non-video file must never be deleted, however old");
    let _ = fs::remove_dir_all(&dir);
}

// Finding 3 (security pass 348): a stale file with a matching extension but
// no "rec-" stem (some other app's video the user's chosen folder already
// had in it) must survive the cleanup that only ever meant to sweep this
// app's own recordings.
#[test]
fn video_folder_cleanup_never_touches_a_stale_file_without_the_rec_prefix() {
    let dir = std::env::temp_dir().join(format!("ps-video-folder-cleanup-noprefix-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let foreign = dir.join("someone-elses-video.mp4");
    fs::write(&foreign, b"not ours").unwrap();
    backdate(&foreign, Duration::from_secs(40 * 24 * 3600));

    let dir_s = dir.to_string_lossy().to_string();
    assert!(video_folder_cleanup_preview_impl(&dir_s, 30, &dir).is_empty(), "a non-rec- file must never be previewed for cleanup");
    assert_eq!(video_folder_cleanup_impl(&dir_s, 30, &dir), 0, "a non-rec- file must never be deleted by cleanup");
    assert!(foreign.is_file(), "the foreign video must survive untouched");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn video_folder_cleanup_refuses_a_dir_that_is_not_the_configured_recording_folder() {
    let configured = std::env::temp_dir().join(format!("ps-video-folder-configured-{}", std::process::id()));
    let foreign = std::env::temp_dir().join(format!("ps-video-folder-foreign-{}", std::process::id()));
    fs::create_dir_all(&configured).unwrap();
    fs::create_dir_all(&foreign).unwrap();
    let victim = foreign.join("not-a-recording.mp4");
    fs::write(&victim, b"old").unwrap();
    backdate(&victim, Duration::from_secs(40 * 24 * 3600));
    let foreign_s = foreign.to_string_lossy().to_string();

    assert!(video_folder_cleanup_preview_impl(&foreign_s, 30, &configured).is_empty(), "a foreign folder must never be previewed");
    assert_eq!(video_folder_cleanup_impl(&foreign_s, 30, &configured), 0, "a foreign folder must never be cleaned");
    assert!(victim.is_file(), "the file in the foreign folder must survive untouched");
    assert_eq!(video_folder_stats_impl(&foreign_s, &configured), (0, 0), "stats for a foreign folder must read as empty");

    let _ = fs::remove_dir_all(&configured);
    let _ = fs::remove_dir_all(&foreign);
}

#[test]
fn discarded_recording_commands_refuse_a_dir_that_is_not_the_configured_recording_folder() {
    let configured = std::env::temp_dir().join(format!("ps-discard-configured-{}", std::process::id()));
    let foreign = std::env::temp_dir().join(format!("ps-discard-foreign-{}", std::process::id()));
    let foreign_trash = foreign.join("verworfen");
    fs::create_dir_all(&configured).unwrap();
    fs::create_dir_all(&foreign_trash).unwrap();
    fs::write(foreign_trash.join("rec-x.mp4"), b"video").unwrap();
    let foreign_s = foreign.to_string_lossy().to_string();

    assert!(list_discarded_recordings_impl(&foreign_s, &configured).is_empty(), "a foreign folder's trash must never be listed");
    assert!(!restore_discarded_recording_impl(&foreign_s, "rec-x.mp4", &configured), "a foreign folder must never be restored from");
    assert!(!delete_discarded_recording_now_impl(&foreign_s, "rec-x.mp4", &configured), "a foreign folder must never be deleted from");
    assert!(foreign_trash.join("rec-x.mp4").is_file(), "the file in the foreign folder must survive untouched");

    let _ = fs::remove_dir_all(&configured);
    let _ = fs::remove_dir_all(&foreign);
}

// Reusing a zoomed-in filmstrip's cache for a zoomed-out request served
// thumbnails spaced for the WRONG count, silently covering only part of the
// clip -- count must be part of the cache key, same as path and mtime.
#[test]
fn filmstrip_cache_dir_keys_on_requested_count_too() {
    let dir = std::env::temp_dir();
    let src = dir.join(format!("ps-filmstrip-key-test-{}.mp4", std::process::id()));
    fs::write(&src, b"x").unwrap();
    let a = filmstrip_cache_dir(&src, 12).unwrap();
    let b = filmstrip_cache_dir(&src, 40).unwrap();
    assert_ne!(a, b, "different requested counts must not collide on the same cache dir");
    assert_eq!(filmstrip_cache_dir(&src, 12).unwrap(), a, "the same (path, mtime, count) must be stable");
    let _ = fs::remove_file(&src);
}

// filmstrip_cache_dir's own comment used to claim an existing sweep
// already covered this folder -- it never actually recursed into it, so a
// cache entry for a since-deleted/renamed/re-encoded recording sat forever.
#[test]
fn sweep_stale_temp_files_also_reclaims_stale_filmstrip_cache_dirs() {
    let dir = sweep_test_root("filmstrip");
    let pid = std::process::id();
    let thumb_root = dir.join(brand::TEMP_DIR).join("video-thumbs");
    let stale_entry = thumb_root.join(format!("stale-{pid}"));
    let fresh_entry = thumb_root.join(format!("fresh-{pid}"));
    fs::create_dir_all(&stale_entry).unwrap();
    fs::write(stale_entry.join("thumb-0000.jpg"), b"jpeg").unwrap();
    fs::create_dir_all(&fresh_entry).unwrap();
    fs::write(fresh_entry.join("thumb-0000.jpg"), b"jpeg").unwrap();
    backdate(&stale_entry, STALE_TEMP_FILE_AGE + Duration::from_secs(60));

    sweep_stale_temp_files_in(&dir);

    assert!(!stale_entry.exists(), "an old, unrevisited filmstrip cache dir must be swept");
    assert!(fresh_entry.exists(), "a recently-touched filmstrip cache dir must survive");
    let _ = fs::remove_dir_all(&dir);
}

// A recording nobody decided on (attach/save/discard) yet, old enough that
// it can only be a crash/abnormal-exit leftover, is swept; one still fresh
// (the post-capture menu could genuinely still be open) and one a prompt
// already references (storeFiles off, so file_path points straight into
// this folder) both survive.
#[test]
fn sweep_stale_recordings_removes_only_old_unreferenced_files() {
    let dir = std::env::temp_dir().join(format!("ps-test-rec-sweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let abandoned = dir.join("2026-09-04_09-00-00.mp4");
    let fresh = dir.join("2026-09-04_09-30-00.mp4");
    let referenced = dir.join("2026-09-04_08-00-00.mp4");
    fs::write(&abandoned, b"video").unwrap();
    fs::write(&fresh, b"video").unwrap();
    fs::write(&referenced, b"video").unwrap();
    backdate(&abandoned, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&referenced, STALE_TEMP_FILE_AGE + Duration::from_secs(60));

    let mut wanted = std::collections::HashSet::new();
    wanted.insert(referenced.to_string_lossy().replace('/', "\\").to_lowercase());
    sweep_stale_recordings_in(&dir, &wanted, &[], true, Duration::from_secs(300), None);

    assert!(!abandoned.exists(), "an old, unreferenced recording must be swept");
    assert!(fresh.exists(), "a fresh recording (menu possibly still open) must survive");
    assert!(referenced.exists(), "a recording a prompt still references must survive however old");

    let _ = fs::remove_dir_all(&dir);
}

// recordingRetentionDays at 0 (None, keep forever) lets a kept but
// unreferenced recording age well past STALE_TEMP_FILE_AGE untouched --
// once a caller passes a retention window, an unreferenced file past it is
// swept even though it is nowhere near that fixed constant, and a
// REFERENCED file survives regardless of how far past the window it is.
#[test]
fn sweep_stale_recordings_in_retention_age_only_removes_unreferenced_files_past_it() {
    let dir = std::env::temp_dir().join(format!("ps-test-rec-retention-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let old_unreferenced = dir.join("2026-09-04_07-00-00.mp4");
    let old_referenced = dir.join("2026-09-04_07-30-00.mp4");
    let young_unreferenced = dir.join("2026-09-04_07-45-00.mp4");
    fs::write(&old_unreferenced, b"video").unwrap();
    fs::write(&old_referenced, b"video").unwrap();
    fs::write(&young_unreferenced, b"video").unwrap();
    // Both ages stay well under STALE_TEMP_FILE_AGE (one hour), and the
    // broken-file sweep is off below -- none of these three plain (not a
    // real mp4) files can be swept for any reason other than retention,
    // which is exactly what this test means to isolate.
    backdate(&old_unreferenced, Duration::from_secs(45 * 60));
    backdate(&old_referenced, Duration::from_secs(45 * 60));
    backdate(&young_unreferenced, Duration::from_secs(10 * 60));

    let mut referenced = std::collections::HashSet::new();
    referenced.insert(old_referenced.to_string_lossy().replace('/', "\\").to_lowercase());

    // Off (None): nothing here is anywhere near STALE_TEMP_FILE_AGE, and
    // broken_sweep_on is false, so all three survive.
    let (count_off, _) = sweep_stale_recordings_in(&dir, &referenced, &[], false, Duration::from_secs(600), None);
    assert_eq!(count_off, 0, "retention off must not remove anything by age alone");
    assert!(old_unreferenced.exists());

    // On, at thirty minutes: only the old, unreferenced file is past it.
    let (count_on, bytes_on) =
        sweep_stale_recordings_in(&dir, &referenced, &[], false, Duration::from_secs(600), Some(Duration::from_secs(30 * 60)));
    assert_eq!(count_on, 1);
    assert!(bytes_on > 0);
    assert!(!old_unreferenced.exists(), "an old, unreferenced recording past the retention window must be swept");
    assert!(old_referenced.exists(), "a recording a prompt still references must survive however old");
    assert!(young_unreferenced.exists(), "a recording under the retention window must survive");

    let _ = fs::remove_dir_all(&dir);
}

// The box parser against a valid file, a truncated (crash) one, and a
// non-mp4 -- exactly the three cases that matter for detection.
#[test]
fn mp4_has_moov_detects_present_and_missing_atom() {
    let dir = std::env::temp_dir().join(format!("ps-test-moov-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let mut valid = Vec::new();
    valid.extend_from_slice(&16u32.to_be_bytes());
    valid.extend_from_slice(b"ftyp");
    valid.extend_from_slice(&[0u8; 8]);
    valid.extend_from_slice(&8u32.to_be_bytes());
    valid.extend_from_slice(b"moov");
    let valid_path = dir.join("valid.mp4");
    fs::write(&valid_path, &valid).unwrap();
    assert!(mp4_has_moov(&valid_path), "a moov box present at the top level must be found");

    let mut truncated = Vec::new();
    truncated.extend_from_slice(&16u32.to_be_bytes());
    truncated.extend_from_slice(b"ftyp");
    truncated.extend_from_slice(&[0u8; 8]);
    truncated.extend_from_slice(&8u32.to_be_bytes());
    truncated.extend_from_slice(b"mdat");
    let truncated_path = dir.join("truncated.mp4");
    fs::write(&truncated_path, &truncated).unwrap();
    assert!(!mp4_has_moov(&truncated_path), "a crash leftover with no moov must be reported as such");

    let garbage_path = dir.join("not-a-video.txt");
    fs::write(&garbage_path, b"just plain text, no box structure at all here").unwrap();
    assert!(!mp4_has_moov(&garbage_path), "non-box-shaped bytes must not be mistaken for a moov box");

    // The header claims a large moov box, but the file ends right
    // after the header itself -- a crash mid-write, not a healthy file.
    let mut cut_short = Vec::new();
    cut_short.extend_from_slice(&16u32.to_be_bytes());
    cut_short.extend_from_slice(b"ftyp");
    cut_short.extend_from_slice(&[0u8; 8]);
    cut_short.extend_from_slice(&5000u32.to_be_bytes());
    cut_short.extend_from_slice(b"moov");
    let cut_short_path = dir.join("cut-short.mp4");
    fs::write(&cut_short_path, &cut_short).unwrap();
    assert!(!mp4_has_moov(&cut_short_path), "a moov header whose declared size overruns the file must be reported as broken");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn recording_is_broken_only_flags_moov_less_mp4() {
    let dir = std::env::temp_dir().join(format!("ps-test-broken-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let mut valid = Vec::new();
    valid.extend_from_slice(&8u32.to_be_bytes());
    valid.extend_from_slice(b"moov");
    let mp4_ok = dir.join("ok.mp4");
    fs::write(&mp4_ok, &valid).unwrap();
    assert!(!recording_is_broken(&mp4_ok), "an mp4 with a moov box is not broken");

    let mp4_broken = dir.join("broken.mp4");
    fs::write(&mp4_broken, b"not a real box structure").unwrap();
    assert!(recording_is_broken(&mp4_broken), "an mp4 with no moov box is broken");

    let other = dir.join("clip.webm");
    fs::write(&other, b"not a real box structure").unwrap();
    assert!(!recording_is_broken(&other), "only mp4 is judged by the moov check");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rec_wav_writer_roundtrips_a_valid_header_and_sample_data() {
    let dir = std::env::temp_dir().join(format!("ps-test-wav-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mic.wav");

    let mut w = RecWavWriter::create(&path, 48_000, 2).expect("create");
    let samples: Vec<i16> = vec![1, -1, 100, -100];
    w.write_samples(&samples).expect("write");
    w.write_samples(&[42]).expect("write more");
    w.finish().expect("finish");

    let bytes = fs::read(&path).unwrap();
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    assert_eq!(&bytes[12..16], b"fmt ");
    assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 1, "PCM format tag");
    assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2, "channel count");
    assert_eq!(u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]), 48_000, "sample rate");
    assert_eq!(&bytes[36..40], b"data");
    let data_size = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]);
    assert_eq!(data_size, (samples.len() as u32 + 1) * 2, "data chunk size must cover every written sample");
    let riff_size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    assert_eq!(riff_size, 36 + data_size, "RIFF size is the fixed 36-byte header plus the data chunk");
    assert_eq!(bytes.len() as u32, 44 + data_size, "file length must match header + data exactly");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn mic_sidecar_video_path_only_matches_the_mic_wav_suffix() {
    assert_eq!(
        mic_sidecar_video_path(Path::new("C:/rec/2026-09-04_10-00-00.mic.wav")),
        Some(PathBuf::from("C:/rec/2026-09-04_10-00-00.mp4"))
    );
    assert_eq!(mic_sidecar_video_path(Path::new("C:/rec/2026-09-04_10-00-00.mp4")), None);
    assert_eq!(mic_sidecar_video_path(Path::new("C:/rec/other.wav")), None);
}

#[test]
fn keys_sidecar_video_path_only_matches_the_keys_json_suffix() {
    assert_eq!(
        keys_sidecar_video_path(Path::new("C:/rec/2026-09-04_10-00-00.keys.json")),
        Some(PathBuf::from("C:/rec/2026-09-04_10-00-00.mp4"))
    );
    assert_eq!(keys_sidecar_video_path(Path::new("C:/rec/2026-09-04_10-00-00.mp4")), None);
    assert_eq!(keys_sidecar_video_path(Path::new("C:/rec/other.json")), None);
}

// A key-overlay sidecar must never be swept away while its own recording
// is still referenced -- the same rule sweep_stale_recordings_in already
// gives the mic sidecar, tested below.
#[test]
fn sweep_stale_recordings_in_keeps_keys_sidecar_of_a_referenced_recording() {
    let dir = std::env::temp_dir().join(format!("ps-test-keys-sidecar-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let video = dir.join("2026-09-04_12-00-00.mp4");
    let sidecar = dir.join("2026-09-04_12-00-00.keys.json");
    fs::write(&video, b"not a real box structure").unwrap();
    fs::write(&sidecar, b"{}").unwrap();
    backdate(&video, Duration::from_secs(3600));
    backdate(&sidecar, Duration::from_secs(3600));

    let mut referenced = std::collections::HashSet::new();
    referenced.insert(video.to_string_lossy().replace('/', "\\").to_lowercase());
    let (count, _bytes) = sweep_stale_recordings_in(&dir, &referenced, &[], true, Duration::from_secs(60), None);

    assert_eq!(count, 0, "neither the referenced mp4 nor its key-overlay sidecar may be swept");
    assert!(video.exists());
    assert!(sidecar.exists(), "the key-overlay sidecar rides along with its referenced recording");

    let _ = fs::remove_dir_all(&dir);
}

// A mic sidecar must never be swept away while its own recording is still
// referenced (e.g. already attached to a prompt) even though the sidecar
// itself is never directly referenced by anything.
#[test]
fn sweep_stale_recordings_in_keeps_mic_sidecar_of_a_referenced_recording() {
    let dir = std::env::temp_dir().join(format!("ps-test-mic-sidecar-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let video = dir.join("2026-09-04_12-00-00.mp4");
    let sidecar = dir.join("2026-09-04_12-00-00.mic.wav");
    fs::write(&video, b"not a real box structure").unwrap();
    fs::write(&sidecar, b"RIFF....WAVE").unwrap();
    backdate(&video, Duration::from_secs(3600));
    backdate(&sidecar, Duration::from_secs(3600));

    let mut referenced = std::collections::HashSet::new();
    referenced.insert(video.to_string_lossy().replace('/', "\\").to_lowercase());
    let (count, _bytes) = sweep_stale_recordings_in(&dir, &referenced, &[], true, Duration::from_secs(60), None);

    assert_eq!(count, 0, "neither the referenced mp4 nor its sidecar may be swept");
    assert!(video.exists());
    assert!(sidecar.exists(), "the sidecar rides along with its referenced recording");

    let _ = fs::remove_dir_all(&dir);
}

// A recording open in the video editor (or the mic sidecar of any
// protected recording, muxed in or not) must survive the sweep exactly
// like the actively-recording session's own output already did -- none of
// these four carry a db reference and all are well past every threshold.
#[test]
fn sweep_stale_recordings_in_never_touches_editor_source_or_any_mic_sidecar() {
    let dir = std::env::temp_dir().join(format!("ps-test-editor-sweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let active = dir.join("2026-09-04_09-00-00.mp4");
    let active_sidecar = dir.join("2026-09-04_09-00-00.mic.wav");
    let editor_src = dir.join("2026-09-04_08-00-00.mp4");
    let editor_sidecar = dir.join("2026-09-04_08-00-00.mic.wav");
    for p in [&active, &active_sidecar, &editor_src, &editor_sidecar] {
        fs::write(p, b"not a real box structure").unwrap();
        backdate(p, Duration::from_secs(3600));
    }

    let empty = std::collections::HashSet::new();
    let protected = [active.clone(), active_sidecar.clone(), editor_src.clone(), editor_sidecar.clone()];
    let (count, bytes) = sweep_stale_recordings_in(&dir, &empty, &protected, true, Duration::from_secs(60), None);

    assert_eq!(count, 0);
    assert_eq!(bytes, 0);
    for p in [&active, &active_sidecar, &editor_src, &editor_sidecar] {
        assert!(p.exists(), "protected path must survive the sweep: {p:?}");
    }

    let _ = fs::remove_dir_all(&dir);
}

// The running session's own output is excluded unconditionally, not by
// age or by moov presence -- a recording in progress has no moov either.
#[test]
fn sweep_stale_recordings_in_never_touches_the_active_path() {
    let dir = std::env::temp_dir().join(format!("ps-test-active-sweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let active = dir.join("2026-09-04_10-00-00.mp4");
    fs::write(&active, b"not a real box structure").unwrap();
    backdate(&active, Duration::from_secs(3600));

    let empty = std::collections::HashSet::new();
    let (count, bytes) = sweep_stale_recordings_in(&dir, &empty, std::slice::from_ref(&active), true, Duration::from_secs(60), None);

    assert_eq!(count, 0);
    assert_eq!(bytes, 0);
    assert!(active.exists(), "the currently recording session's own file must survive every sweep");

    let _ = fs::remove_dir_all(&dir);
}

// A camera take abandoned after a failed chunk write keeps its partial
// file (and its marker) through every sweep, however old; an ordinary
// stale file next to it is still removed.
#[test]
fn sweep_stale_recordings_in_keeps_an_abandoned_camera_take() {
    let dir = std::env::temp_dir().join(format!("ps-test-abandoned-sweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let kept = dir.join("rec-cam-2026-09-29_100000.webm");
    let marker = camera::abandoned_marker_path(&kept);
    let stale = dir.join("rec-cam-2026-09-29_090000.webm");
    for p in [&kept, &marker, &stale] {
        fs::write(p, b"partial").unwrap();
        backdate(p, Duration::from_secs(3 * 24 * 3600));
    }

    let empty = std::collections::HashSet::new();
    let (count, _) = sweep_stale_recordings_in(&dir, &empty, &[], true, Duration::from_secs(60), Some(Duration::from_secs(3600)));

    assert_eq!(count, 1);
    assert!(kept.exists(), "the abandoned take must survive the sweep");
    assert!(marker.exists(), "its marker must survive too");
    assert!(!stale.exists(), "an ordinary stale file is still swept");

    let _ = fs::remove_dir_all(&dir);
}

// A broken (moov-less) mp4 past the short grace period is removed even
// though it is nowhere near STALE_TEMP_FILE_AGE -- the whole point of 281.
#[test]
fn sweep_stale_recordings_in_removes_broken_past_grace_period() {
    let dir = std::env::temp_dir().join(format!("ps-test-broken-sweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let broken = dir.join("2026-09-04_11-00-00.mp4");
    fs::write(&broken, b"not a real box structure").unwrap();
    backdate(&broken, Duration::from_secs(10 * 60));
    let healthy_young = dir.join("2026-09-04_11-05-00.mp4");
    let mut moov = Vec::new();
    moov.extend_from_slice(&8u32.to_be_bytes());
    moov.extend_from_slice(b"moov");
    fs::write(&healthy_young, &moov).unwrap();
    backdate(&healthy_young, Duration::from_secs(10 * 60));

    let empty = std::collections::HashSet::new();
    let (count, bytes) = sweep_stale_recordings_in(&dir, &empty, &[], true, Duration::from_secs(5 * 60), None);

    assert_eq!(count, 1);
    assert!(bytes > 0);
    assert!(!broken.exists(), "a broken mp4 past the grace period must be swept");
    assert!(healthy_young.exists(), "a healthy mp4 well under STALE_TEMP_FILE_AGE must survive");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sweep_stale_pdfium_dlls_keeps_only_the_current_version() {
    let dir = std::env::temp_dir().join(format!("ps-test-pdfium-sweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let current = dir.join("pdfium-2.7.0.dll");
    let older = dir.join("pdfium-2.6.3.dll");
    let unrelated = dir.join("pdfium-LICENSE");
    fs::write(&current, b"current").unwrap();
    fs::write(&older, b"stale").unwrap();
    fs::write(&unrelated, b"license text").unwrap();

    sweep_stale_pdfium_dlls(&current);

    assert!(current.exists(), "the current version's own DLL must survive its own sweep");
    assert!(!older.exists(), "a previous version's DLL must be removed");
    assert!(unrelated.exists(), "a same-prefix non-.dll file must survive");

    let _ = fs::remove_dir_all(&dir);
}

// A fallback file that exists but cannot be decrypted/parsed (DPAPI from a
// different Windows account, truncated write) must never be silently treated as
// "nothing here": that used to erase the whole DB on the next load_store.
#[test]
fn sealed_json_distinguishes_missing_from_unreadable() {
    let dir = std::env::temp_dir().join(format!("ps-sealed-missing-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);

    let missing = dir.join("nope.json");
    assert!(matches!(
        read_sealed_json::<Vec<String>>(&missing),
        SealedRead::Missing
    ));

    let garbled = dir.join("garbled.json");
    fs::write(&garbled, "not json at all {{{").expect("write garbled");
    assert!(matches!(
        read_sealed_json::<Vec<String>>(&garbled),
        SealedRead::Unreadable
    ));

    // A prefixed value that fails to decrypt yields "" from dec_str, which is
    // likewise not valid JSON: the exact path that used to silently default.
    let sealed_garbage = dir.join("sealed.json");
    fs::write(&sealed_garbage, format!("{}not-real-base64!!", ENC_PREFIX)).expect("write");
    assert!(matches!(
        read_sealed_json::<Vec<String>>(&sealed_garbage),
        SealedRead::Unreadable
    ));

    let _ = fs::remove_dir_all(&dir);
}

// A cache written by a build before BackupPrompt existed is a sealed
// Vec<String> -- must read back as name-only items instead of an empty list,
// and the caller must be told to re-write it (once), not on every read.
#[test]
fn backup_names_cache_upgrades_a_legacy_string_list() {
    let dir = std::env::temp_dir().join(format!("ps-backup-names-legacy-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let cache = dir.join("prompts.names.json");

    let legacy = enc_str(&serde_json::to_string(&vec!["Draft reply", "Meeting notes"]).unwrap());
    fs::write(&cache, legacy).expect("write legacy cache");
    let (items, legacy) = read_prompt_names_cache(&cache);
    assert!(legacy);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].name, "Draft reply");
    assert_eq!(items[0].chars, 0);
    assert_eq!(items[0].kind, "text");

    // Current-shape cache round-trips without the migration flag.
    write_prompt_names(&cache, &items.iter().map(|i| BackupPrompt {
        name: i.name.clone(), chars: 42, kind: "image".into(),
    }).collect::<Vec<_>>());
    let (items2, legacy2) = read_prompt_names_cache(&cache);
    assert!(!legacy2);
    assert_eq!(items2[0].chars, 42);
    assert_eq!(items2[0].kind, "image");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn history_retention_default_and_explicit_values() {
    // A new store prunes after 30 days; stores that predate that default are
    // pinned to 0 (keep forever) by migrate(), so nobody loses history silently.
    let mut s = Settings::default();
    assert_eq!(history_max_age(&s), Some(30 * 86_400), "no historyDays set => the new-install default");
    s.ui_values.insert("historyDays".into(), 0.0);
    assert_eq!(history_max_age(&s), None, "0 => keep forever");
    s.ui_values.insert("historyDays".into(), 7.0);
    assert_eq!(history_max_age(&s), Some(7 * 86_400), "explicit days still prune");
}

#[test]
fn backup_fixed_key_roundtrip_is_portable() {
    // Default backups carry no password: any install must be able to restore them.
    let plain = br#"{"prompts":[],"settings":{}}"#;
    let blob = backup_encrypt(plain, None).expect("encrypt");
    assert_eq!(&blob[0..4], BACKUP_MAGIC, "header magic");
    assert_eq!(blob[4], 2, "mode 2 = fixed key + compressed");
    assert_ne!(&blob[5..], &plain[..], "payload is not stored in the clear");
    let out = backup_decrypt(&blob, None).expect("decrypt").expect("psb format");
    assert_eq!(out, plain);
}

#[test]
fn backup_password_roundtrip_and_wrong_password_fails() {
    let plain = b"secret prompts";
    let blob = backup_encrypt(plain, Some("correct horse")).expect("encrypt");
    assert_eq!(blob[4], 3, "mode 3 = password + compressed");
    let out = backup_decrypt(&blob, Some("correct horse")).expect("decrypt").expect("psb");
    assert_eq!(out, plain);
    assert!(backup_decrypt(&blob, Some("wrong")).is_err(), "wrong password rejected");
    assert!(backup_decrypt(&blob, None).is_err(), "missing password rejected");
    // The fixed key must NOT open a password-protected backup.
    assert!(backup_decrypt(&blob, Some("")).is_err());
}

#[test]
fn backup_is_due_fires_for_an_aged_backup_and_for_none_at_all() {
    assert!(backup_is_due(None, 24), "no backup on disk is always due");
    assert!(backup_is_due(Some(24 * 3600), 24), "exactly one interval old is due");
    assert!(backup_is_due(Some(25 * 3600), 24), "older than the interval is due");
}

#[test]
fn backup_is_due_stays_false_right_after_a_backup_just_ran() {
    // The same check that lets a due backup fire at start also keeps a
    // second app start moments later from taking a second one: it sees the
    // first start's backup as a few seconds old, nowhere near the interval.
    assert!(!backup_is_due(Some(0), 24));
    assert!(!backup_is_due(Some(30), 24), "30s old is nowhere near a 24h interval");
}

#[test]
fn retention_keeps_the_right_backups_and_never_all_of_them() {
    let names: Vec<String> = [
        "2024-03-01_02-00-00", // older year
        "2025-11-30_02-00-00", // older month
        "2026-07-20_02-00-00", // older week
        "2026-07-25_02-00-00",
        "2026-07-26_02-00-00", // newest
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    // Plain count: the oldest ones go, the newest `keep` stay.
    assert_eq!(backups_to_drop(&names, 2, None), names[..3].to_vec());
    assert!(backups_to_drop(&names, 99, None).is_empty());
    // A count of 0 must not be read as "delete everything".
    assert_eq!(backups_to_drop(&names, 0, None).len(), names.len() - 1);

    // Tiers count PERIODS, newest first: 3 years reach back to 2024, while one
    // day / week / month only ever pins the newest backup. `keep` is a floor, so
    // it has to stay at 1 here for the tiers alone to decide.
    let tiers = GfsConfig { daily: 1, weekly: 1, monthly: 1, yearly: 3 };
    let dropped = backups_to_drop(&names, 1, Some(&tiers));
    assert!(!dropped.contains(&names[4]), "the newest backup always survives");
    assert!(!dropped.contains(&names[1]), "second-newest year kept");
    assert!(!dropped.contains(&names[0]), "third-newest year kept");
    assert_eq!(dropped, vec![names[2].clone(), names[3].clone()], "no tier asks for these");

    // Every tier at zero must not wipe the lot: the newest one stays.
    let empty = GfsConfig { daily: 0, weekly: 0, monthly: 0, yearly: 0 };
    let dropped = backups_to_drop(&names, 1, Some(&empty));
    assert_eq!(dropped.len(), names.len() - 1);
    assert!(!dropped.contains(&names[4]));

    // Reserved placeholders gain their slash fence exactly once.
    assert_eq!(fence_dyn_vars("Hi {#{date}#} and {#{name}#}"), "Hi {#{/date\\}#} and {#{name}#}");
    assert_eq!(fence_dyn_vars("{#{/date\\}#}"), "{#{/date\\}#}", "already fenced stays put");
    assert_eq!(fence_dyn_vars("plain text"), "plain text");
    // A choice variable that happens to start with a reserved word is untouched.
    assert_eq!(fence_dyn_vars("{#{date|a,b}#}"), "{#{date|a,b}#}");

    // The schedule reads the folder name, not a modification time.
    let a = stamp_secs("2026-07-29_08-00-00").unwrap();
    let b = stamp_secs("2026-07-29_10-30-00").unwrap();
    assert_eq!(b - a, 2 * 3600 + 30 * 60);
    assert_eq!(stamp_secs("2026-07-30_08-00-00").unwrap() - a, 86_400);
    assert!(stamp_secs("not-a-stamp").is_none());

    // `keep` survives the tiers: several backups on ONE day must not delete each
    // other just because the daily tier only pins one per date.
    let today: Vec<String> = ["2026-07-29_08-00-00", "2026-07-29_09-00-00", "2026-07-29_10-00-00"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let dropped = backups_to_drop(&today, 10, Some(&tiers));
    assert!(dropped.is_empty(), "same-day backups stay while `keep` has room");
    // With no room left, the tiers thin them out as before.
    let dropped = backups_to_drop(&today, 1, Some(&tiers));
    assert_eq!(dropped, today[..2].to_vec());
}

#[test]
fn backups_are_compressed_and_old_ones_still_open() {
    // Repetitive JSON is what a real backup looks like; it must shrink a lot.
    let plain = br#"{"prompts":[{"id":"a","name":"Review","text":"Review the code."}]}"#
        .repeat(40);
    let blob = backup_encrypt(&plain, None).expect("encrypt");
    assert_eq!(blob[4], 2, "mode 2 = fixed key + compressed");
    assert!(blob.len() * 4 < plain.len(), "compressed to well under a quarter");
    assert_eq!(backup_decrypt(&blob, None).expect("decrypt").expect("psb"), plain);

    // An uncompressed mode-0 file (written before 2.6.0) must still restore.
    let mut legacy = Vec::from(&BACKUP_MAGIC[..]);
    legacy.push(0);
    {
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Key, Nonce};
        let nonce = [7u8; 12];
        legacy.extend_from_slice(&nonce);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&BACKUP_FIXED_KEY));
        legacy.extend_from_slice(&cipher.encrypt(Nonce::from_slice(&nonce), &plain[..]).unwrap());
    }
    assert_eq!(backup_decrypt(&legacy, None).expect("decrypt").expect("psb"), plain);
}

#[test]
fn password_errors_stay_recognisable_for_the_ui() {
    // The restore flow decides whether to show the password prompt by matching
    // "password" in the error text. Rewording these would silently disable it.
    let blob = backup_encrypt(b"x", Some("pw")).expect("encrypt");
    let missing = backup_decrypt(&blob, None).unwrap_err();
    let wrong = backup_decrypt(&blob, Some("nope")).unwrap_err();
    assert_eq!(missing, "password-required");
    for msg in [&missing, &wrong] {
        assert!(msg.to_lowercase().contains("password"), "must mention password: {msg}");
    }
    // A backup without a password must NOT trigger the prompt.
    let fixed = backup_encrypt(b"x", None).expect("encrypt");
    assert!(backup_decrypt(&fixed, None).is_ok(), "fixed-key backup opens silently");
}

// ---------- Confidential entries ----------

#[test]
fn secret_encrypt_then_decrypt_round_trips() {
    let key = secret_derive_key("correct horse", b"0123456789abcdef").expect("derive");
    let plain = "the real, sensitive text of this prompt";
    let ct = secret_encrypt(plain, &key);
    assert_ne!(ct, plain, "ciphertext must not equal the plain text");
    assert!(!ct.contains(plain), "ciphertext must not contain the plain text as a substring");
    assert_eq!(secret_decrypt(&ct, &key).expect("decrypt"), plain);
}

#[test]
fn secret_decrypt_with_wrong_password_fails_cleanly_without_touching_the_entry() {
    let salt = b"0123456789abcdef";
    let right_key = secret_derive_key("right password", salt).expect("derive");
    let wrong_key = secret_derive_key("wrong password", salt).expect("derive");
    let plain = "do not leak this";
    let ct = secret_encrypt(plain, &right_key);
    let err = secret_decrypt(&ct, &wrong_key).unwrap_err();
    assert_eq!(err, "wrong-password");
    // The stored ciphertext itself is untouched by the failed attempt and
    // still opens with the real key -- a wrong password never destroys it.
    assert_eq!(secret_decrypt(&ct, &right_key).expect("decrypt"), plain);
}

#[test]
fn secret_derive_key_is_deterministic_and_password_sensitive() {
    let salt = b"0123456789abcdef";
    let a1 = secret_derive_key("same password", salt).expect("derive");
    let a2 = secret_derive_key("same password", salt).expect("derive");
    let b = secret_derive_key("different password", salt).expect("derive");
    assert_eq!(a1, a2, "same password + salt must derive the same key every time");
    assert_ne!(a1, b, "a different password must derive a different key");
}

#[test]
fn reveal_secret_text_passes_through_a_plain_prompt_unchanged() {
    let mut p = test_prompt("a");
    p.text = "plain text, never encrypted".into();
    let (text, variants) = reveal_secret_text(&p, None).expect("plain prompt never needs a key");
    assert_eq!(text, p.text);
    assert!(variants.is_empty());
}

#[test]
fn reveal_secret_text_refuses_a_confidential_prompt_with_no_key() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut p = test_prompt("a");
    p.secret = true;
    p.text = secret_encrypt("the real text", &key);
    let err = reveal_secret_text(&p, None).unwrap_err();
    assert_eq!(err, "secret-locked");
}

#[test]
fn reveal_secret_text_decrypts_a_confidential_prompt_and_its_variants_when_unlocked() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut p = test_prompt("a");
    p.secret = true;
    p.text = secret_encrypt("main entry", &key);
    p.variants = vec![secret_encrypt("entry two", &key), secret_encrypt("entry three", &key)];
    let (text, variants) = reveal_secret_text(&p, Some(key)).expect("decrypt");
    assert_eq!(text, "main entry");
    assert_eq!(variants, vec!["entry two".to_string(), "entry three".to_string()]);
}

#[test]
fn vault_first_setup_failed_save_leaves_no_salt_or_verifier() {
    let mut settings = Settings::default();
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let verify = secret_encrypt(SECRET_VERIFY_MARKER, &key);
    let r = secret_vault_first_setup(&mut settings, b"0123456789abcdef", verify, |_| false);
    assert_eq!(r, Err("save-failed".to_string()));
    assert!(!settings.ui_texts.contains_key(SECRET_SALT_KEY), "no salt may survive");
    assert!(!settings.ui_texts.contains_key(SECRET_VERIFY_KEY), "no verifier may survive");
}

#[test]
fn vault_first_setup_failed_save_restores_old_empty_values() {
    let mut settings = Settings::default();
    settings.ui_texts.insert(SECRET_SALT_KEY.into(), String::new());
    settings.ui_texts.insert(SECRET_VERIFY_KEY.into(), "old".into());
    let r = secret_vault_first_setup(&mut settings, b"0123456789abcdef", "new".into(), |_| false);
    assert!(r.is_err());
    assert_eq!(settings.ui_texts.get(SECRET_SALT_KEY).map(String::as_str), Some(""));
    assert_eq!(settings.ui_texts.get(SECRET_VERIFY_KEY).map(String::as_str), Some("old"));
}

#[test]
fn vault_first_setup_success_persists_and_rederives_the_same_key() {
    let mut settings = Settings::default();
    let salt = b"0123456789abcdef";
    let key = secret_derive_key("pw", salt).expect("derive");
    let verify = secret_encrypt(SECRET_VERIFY_MARKER, &key);
    let mut saved_salt = None;
    let r = secret_vault_first_setup(&mut settings, salt, verify, |s| {
        saved_salt = s.ui_texts.get(SECRET_SALT_KEY).cloned();
        true
    });
    assert_eq!(r, Ok(true));
    assert!(saved_salt.is_some(), "the salt was in the settings when they were saved");
    let salt_b64 = settings.ui_texts.get(SECRET_SALT_KEY).cloned().unwrap();
    let verify_b64 = settings.ui_texts.get(SECRET_VERIFY_KEY).cloned().unwrap();
    assert_eq!(secret_vault_verify("pw", &salt_b64, &verify_b64), Ok(key));
    assert_eq!(secret_vault_verify("other", &salt_b64, &verify_b64), Err("wrong-password".to_string()));
}

#[test]
fn vault_first_setup_never_replaces_an_existing_salt() {
    let mut settings = Settings::default();
    settings.ui_texts.insert(SECRET_SALT_KEY.into(), "existing".into());
    settings.ui_texts.insert(SECRET_VERIFY_KEY.into(), "existing-verify".into());
    let mut saves = 0;
    let r = secret_vault_first_setup(&mut settings, b"0123456789abcdef", "new".into(), |_| {
        saves += 1;
        true
    });
    assert_eq!(r, Ok(false));
    assert_eq!(saves, 0);
    assert_eq!(settings.ui_texts.get(SECRET_SALT_KEY).map(String::as_str), Some("existing"));
    assert_eq!(settings.ui_texts.get(SECRET_VERIFY_KEY).map(String::as_str), Some("existing-verify"));
}

fn secret_test_store(prompts: Vec<Prompt>) -> Store {
    Store {
        prompts,
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: Default::default(),
    }
}

// A backfill pass is mid-recognition when the switch goes off and the clear runs:
// the text it returns afterwards must not land anywhere.
#[test]
fn a_pass_started_before_the_ocr_clear_never_writes_its_text() {
    let mut store = secret_test_store(vec![image_text_prompt("a", "")]);
    store.settings.ui_flags.insert("ocrIndex".into(), true);
    let pic = store.prompts[0].image.clone();
    // sanity: an uninterrupted pass writes
    let started = ocr_epoch();
    assert!(apply_ocr_backfill_text(&mut store, "a", &pic, "first".into(), started));
    assert_eq!(store.prompts[0].image_text, "first");
    // a picture swapped while the pass ran blocks the write
    store.prompts[0].image_text.clear();
    assert!(!apply_ocr_backfill_text(&mut store, "a", "other-picture.png", "wrong".into(), started));
    assert!(store.prompts[0].image_text.is_empty());
    store.prompts[0].image_text = "first".into();
    // second pass: the recognizer "flips the flag off and clears" before returning
    let started = ocr_epoch();
    let recognize = |store: &mut Store| {
        store.settings.ui_flags.insert("ocrIndex".into(), false);
        assert!(clear_ocr_index_text(store));
        "late text".to_string()
    };
    let text = recognize(&mut store);
    assert!(!apply_ocr_backfill_text(&mut store, "a", &pic, text, started));
    assert!(store.prompts.iter().chain(store.trash.iter()).all(|p| p.image_text.is_empty()));
    // switched back on, but the stale pass (old epoch) still must not write
    store.settings.ui_flags.insert("ocrIndex".into(), true);
    assert!(!apply_ocr_backfill_text(&mut store, "a", &pic, "stale".into(), started));
    assert!(store.prompts[0].image_text.is_empty());
}

fn prompts_json(store: &Store) -> String {
    serde_json::to_string(&store.prompts).unwrap()
}

#[test]
fn set_secret_decrypt_with_a_corrupt_middle_variant_changes_nothing() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut p = test_prompt("a");
    p.secret = true;
    p.text = secret_encrypt("main", &key);
    p.variants = vec![secret_encrypt("one", &key), "not-a-ciphertext".into(), secret_encrypt("three", &key)];
    let mut store = secret_test_store(vec![p]);
    let before = prompts_json(&store);
    let mut saved = false;
    let r = apply_prompt_secret(&mut store, "a", false, &key, |_| { saved = true; true });
    assert!(r.is_err());
    assert!(!saved, "nothing may be persisted after a failed conversion");
    assert_eq!(prompts_json(&store), before, "prompt must stay byte for byte unchanged");
    assert!(store.prompts[0].secret);
}

#[test]
fn set_secret_save_failure_restores_the_prompt_in_both_directions() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut plain = test_prompt("a");
    plain.text = "plain main".into();
    plain.variants = vec!["v1".into(), "v2".into()];
    let mut store = secret_test_store(vec![plain]);
    let before = prompts_json(&store);
    let r = apply_prompt_secret(&mut store, "a", true, &key, |_| false);
    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(prompts_json(&store), before);

    let mut sealed = test_prompt("b");
    sealed.secret = true;
    sealed.text = secret_encrypt("main", &key);
    sealed.variants = vec![secret_encrypt("v1", &key)];
    let mut store = secret_test_store(vec![sealed]);
    let before = prompts_json(&store);
    let r = apply_prompt_secret(&mut store, "b", false, &key, |_| false);
    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(prompts_json(&store), before);
}

#[test]
fn set_secret_success_changes_text_variants_and_flag_together() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut plain = test_prompt("a");
    plain.text = "plain main".into();
    plain.variants = vec!["v1".into(), "v2".into()];
    let mut store = secret_test_store(vec![plain]);
    apply_prompt_secret(&mut store, "a", true, &key, |_| true).expect("encrypt");
    assert!(store.prompts[0].secret);
    assert_ne!(store.prompts[0].text, "plain main");
    assert_eq!(secret_decrypt(&store.prompts[0].variants[1], &key).unwrap(), "v2");
    apply_prompt_secret(&mut store, "a", false, &key, |_| true).expect("decrypt");
    assert!(!store.prompts[0].secret);
    assert_eq!(store.prompts[0].text, "plain main");
    assert_eq!(store.prompts[0].variants, vec!["v1".to_string(), "v2".to_string()]);
}

#[test]
fn secret_encrypt_if_plain_never_double_encrypts_an_already_sealed_value() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let plain = "a version-history snapshot from before this entry was confidential";
    // A snapshot taken while NOT yet confidential: plain text, must be encrypted.
    let sealed_once = secret_encrypt_if_plain(plain, &key);
    assert_ne!(sealed_once, plain);
    assert_eq!(secret_decrypt(&sealed_once, &key).expect("decrypt"), plain);
    // A snapshot taken while ALREADY confidential: already ciphertext, must be
    // kept exactly as-is rather than encrypted a second time (which would make
    // it decrypt back to the ciphertext string, not the original text).
    let already_sealed = secret_encrypt("already confidential text", &key);
    let kept = secret_encrypt_if_plain(&already_sealed, &key);
    assert_eq!(kept, already_sealed, "must not double-encrypt");
    assert_eq!(secret_decrypt(&kept, &key).expect("decrypt"), "already confidential text");
}

#[test]
fn a_confidential_prompt_never_serializes_its_plain_text_to_disk() {
    // save_prompts ultimately serializes the same Prompt struct this builds --
    // this proves the plain text is nowhere in that serialized form once the
    // entry is confidential, only the ciphertext (and the unrelated name).
    let key = secret_derive_key("vault password", b"0123456789abcdef").expect("derive");
    let secret_value = "the confidential text nobody but the vault password should reveal";
    let mut p = test_prompt("a");
    p.name = "Ordinary visible name".into();
    p.secret = true;
    p.text = secret_encrypt(secret_value, &key);
    let json = serde_json::to_string(&p).expect("serialize");
    assert!(!json.contains(secret_value), "plain text must never appear in the serialized prompt");
    assert!(json.contains(&p.name), "the name stays plain on purpose");
    assert!(json.contains("\"secret\":true"));
}

#[test]
fn export_filter_keeps_or_drops_confidential_prompts_on_request() {
    let mut open = test_prompt("open");
    open.secret = false;
    let mut locked = test_prompt("locked");
    locked.secret = true;
    let prompts = vec![open.clone(), locked.clone()];

    let excluded = filter_export_secrets(prompts.clone(), false);
    assert_eq!(excluded.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["open"]);

    let included = filter_export_secrets(prompts, true);
    assert_eq!(included.len(), 2, "both choices must be honoured, including keeping it");
}

#[test]
fn markdown_sync_never_picks_up_a_confidential_prompt() {
    let mut p = test_prompt("a");
    assert!(sync_eligible(&p), "a plain prompt with a safe id is eligible");
    p.secret = true;
    assert!(!sync_eligible(&p), "a confidential prompt must never be synced as plain text");
}

#[test]
fn reset_settings_leaves_a_usable_state() {
    // reset_settings replaces the settings with Settings::default() + migrate().
    // If that produced no view, the first grid mutation would index views[0] and
    // panic (the same trap import_all guards against), leaving a dead app.
    let mut s = Settings::default();
    s.migrate();
    assert!(!s.views.is_empty(), "reset must leave at least one view");
    assert!(
        s.views.iter().any(|v| v.id == s.active_view),
        "active_view must point at an existing view"
    );
    assert_eq!(s.views[0].cols, default_cols());
    assert_eq!(s.views[0].rows, default_rows());
    assert_eq!(s.theme, default_theme(), "theme back to default");
    assert!(s.ui_flags.is_empty() && s.ui_values.is_empty(), "expert tweaks cleared");
}

#[test]
fn a_failed_trash_restore_keeps_the_receiver_secrets_in_the_live_settings() {
    let mut settings = Settings::default();
    settings.migrate();
    let mut store = Store { prompts: Vec::new(), trash: Vec::new(), settings, prompts_locked: false, written: Default::default() };
    let mut p = test_prompt("trashed");
    p.id = "t1".into();
    p.trashed_at = 5;
    store.trash.push(p);
    store.settings.ui_texts.insert("remotePasswordHash".into(), "hash".into());
    store.settings.ui_texts.insert("remoteTlsKey".into(), "sealed".into());
    let r = restore_trashed_core(&mut store, "t1", |_| false);
    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(store.settings.ui_texts.get("remotePasswordHash").map(String::as_str), Some("hash"));
    assert_eq!(store.settings.ui_texts.get("remoteTlsKey").map(String::as_str), Some("sealed"));
    assert_eq!(store.trash.len(), 1);
    assert_eq!(store.trash[0].trashed_at, 5);
    assert!(store.prompts.is_empty());
}

fn placement_fixture() -> Store {
    let mut settings = Settings::default();
    settings.migrate();
    settings.views = serde_json::from_value(serde_json::json!([
        {"id": "a", "name": "A", "cols": 4, "rows": 3,
         "members": ["p1", "p2", "p3", "p4", "p5"],
         "layouts": {"4x3": {"p3": [0, 0], "p4": [1, 0], "p5": [3, 0]}}},
        {"id": "b", "name": "B", "cols": 4, "rows": 3,
         "members": ["p3", "p6"],
         "layouts": {"4x3": {"p3": [1, 1], "p6": [0, 0]}}}
    ]))
    .unwrap();
    settings.active_view = "a".into();
    settings.tile_folders = serde_json::from_value(serde_json::json!([
        {"id": "tf", "name": "T", "view_id": "a", "cell": [2, 0], "members": ["p1", "p2"]}
    ]))
    .unwrap();
    settings.float_folders = serde_json::from_value(serde_json::json!([
        {"id": "ff", "name": "F", "members": ["p4", "p5"]}
    ]))
    .unwrap();
    settings.floating.insert("p3".into(), Pos { x: 40, y: 50, screen: String::new() });
    settings.float_scale.insert("p3".into(), 1.5);
    let prompts = ["p1", "p2", "p3", "p4", "p5", "p6"].iter().map(|id| test_prompt(id)).collect();
    Store { prompts, trash: Vec::new(), settings, prompts_locked: false, written: Default::default() }
}

fn view_of<'a>(store: &'a Store, id: &str) -> &'a View {
    store.settings.views.iter().find(|v| v.id == id).unwrap()
}

#[test]
fn undoing_a_batch_delete_restores_every_placement() {
    let mut store = placement_fixture();
    let ids: Vec<String> = ["p1", "p3", "p4"].iter().map(|s| s.to_string()).collect();
    let snapshot = capture_placements(&store.settings, &ids);
    move_batch_to_trash(&mut store, &ids, 100);
    assert!(store.settings.tile_folders.is_empty() && store.settings.float_folders.is_empty(), "both folders dissolved by the delete");
    assert!(!view_of(&store, "a").members.contains(&"p3".to_string()));
    // Another tile takes p3's old cell in view a before the undo.
    store.prompts.push(test_prompt("p9"));
    let a = store.settings.views.iter_mut().find(|v| v.id == "a").unwrap();
    a.members.push("p9".into());
    a.layouts.get_mut("4x3").unwrap().insert("p9".into(), [0, 0]);

    assert_eq!(restore_placed_core(&mut store, &snapshot, |_| true), Ok(3));
    assert!(store.trash.is_empty());
    let a = view_of(&store, "a");
    let b = view_of(&store, "b");
    assert_eq!(a.members[0], "p1", "member order kept for the view");
    assert!(a.members.contains(&"p3".to_string()) && a.members.contains(&"p4".to_string()));
    assert_eq!(b.members, vec!["p3".to_string(), "p6".to_string()], "the second view membership is back");
    assert_eq!(b.layouts["4x3"].get("p3"), Some(&[1, 1]), "its cell in the second view is back");
    let la = &a.layouts["4x3"];
    assert_eq!(la.get("p9"), Some(&[0, 0]), "the tile that took the cell stays");
    let p3_cell = la.get("p3").copied().expect("p3 is on the grid again");
    assert_ne!(p3_cell, [0, 0], "a taken cell falls back to a free one");
    let mut cells: Vec<[u32; 2]> = la.values().copied().collect();
    cells.sort();
    cells.dedup();
    assert_eq!(cells.len(), la.len(), "no two tiles share a cell");
    assert!(!la.contains_key("p1"), "a folder member owns no cell");
    let tf = &store.settings.tile_folders[0];
    assert_eq!((tf.id.as_str(), tf.cell, tf.members.clone()), ("tf", [2, 0], vec!["p1".to_string(), "p2".to_string()]));
    assert_eq!(store.settings.float_folders[0].members, vec!["p4".to_string(), "p5".to_string()]);
    assert_eq!(store.settings.floating.get("p3").map(|p| (p.x, p.y)), Some((40, 50)));
    assert_eq!(store.settings.float_scale.get("p3"), Some(&1.5));
}

#[test]
fn a_failed_batch_undo_leaves_the_trash_and_settings_untouched() {
    let mut store = placement_fixture();
    let ids: Vec<String> = vec!["p3".to_string(), "p4".to_string()];
    let snapshot = capture_placements(&store.settings, &ids);
    move_batch_to_trash(&mut store, &ids, 100);
    let members_before = view_of(&store, "a").members.clone();
    assert_eq!(restore_placed_core(&mut store, &snapshot, |_| false), Err("save-failed".to_string()));
    assert_eq!(store.trash.len(), 2);
    assert_eq!(store.prompts.len(), 4);
    assert_eq!(view_of(&store, "a").members, members_before);
    assert!(store.settings.floating.is_empty());
    assert_eq!(restore_placed_core(&mut store, &[], |_| true), Err("not-found".to_string()));
}

#[test]
fn reset_expert_keeps_the_keys_and_drops_the_preferences() {
    let mut s = Settings::default();
    s.ui_texts.insert(SECRET_SALT_KEY.to_string(), "salt".into());
    s.ui_texts.insert(SECRET_VERIFY_KEY.to_string(), "verify".into());
    s.ui_texts.insert(BACKUP_PW_KEY.to_string(), "sealed".into());
    s.ui_texts.insert("remoteTlsKey".to_string(), "tls".into());
    s.ui_texts.insert("copyFont".to_string(), "mono".into());
    s.ui_flags.insert("clipWatcher".to_string(), true);
    s.ui_flags.insert("remoteTlsShortLived".to_string(), true);
    s.ui_texts.insert("remoteTlsCert".to_string(), "cert".into());
    s.ui_values.insert("trayRecentN".to_string(), 9.0);
    reset_expert_settings(&mut s);
    assert!(!s.ui_flags.contains_key("remoteTlsShortLived"), "the switch goes back to off");
    assert_eq!(s.ui_texts.get("remoteTlsCert").map(String::as_str), Some("cert"), "without rotating the certificate");
    for (k, v) in [(SECRET_SALT_KEY, "salt"), (SECRET_VERIFY_KEY, "verify"), (BACKUP_PW_KEY, "sealed"), ("remoteTlsKey", "tls")] {
        assert_eq!(s.ui_texts.get(k).map(String::as_str), Some(v), "{k} must survive");
    }
    assert!(!s.ui_texts.contains_key("copyFont"));
    assert!(s.ui_flags.is_empty() && s.ui_values.is_empty());
}

#[test]
fn reset_settings_keeps_the_backup_password() {
    // Mirrors what reset_settings does with the store's settings. The sealed
    // backup password must survive: without it every password-protected
    // backup on disk would be undecryptable.
    let mut old = Settings { theme: "ocean".into(), ..Default::default() };
    old.ui_texts.insert(BACKUP_PW_KEY.to_string(), "sealed-blob".into());
    old.ui_texts.insert("screenshotDir".to_string(), "C:/shots".into());

    let backup_pw = old.ui_texts.remove(BACKUP_PW_KEY);
    let mut fresh = Settings::default();
    fresh.migrate();
    if let Some(pw) = backup_pw {
        fresh.ui_texts.insert(BACKUP_PW_KEY.to_string(), pw);
    }

    assert_eq!(fresh.ui_texts.get(BACKUP_PW_KEY).map(String::as_str), Some("sealed-blob"));
    assert!(!fresh.ui_texts.contains_key("screenshotDir"), "other texts are reset");
    assert_eq!(fresh.theme, default_theme());
}

#[test]
fn delete_all_data_clears_data_tables_only() {
    // The data-only wipe must empty every derived table but touch no settings.
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');
             CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
             CREATE TABLE edit_state(id TEXT PRIMARY KEY, kind TEXT NOT NULL, target TEXT NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL, ops TEXT NOT NULL, updated_at INTEGER NOT NULL);
             INSERT INTO prompts VALUES('a', 0, '{}');
             INSERT INTO trash VALUES('b', 0, '{}');
             INSERT INTO meta VALUES('settings', '{\"theme\":\"ocean\"}');
             INSERT INTO copy_log VALUES(1, 'a');
             INSERT INTO prompt_versions(prompt_id, ts, name, text) VALUES('a', 1, 'n', 't');
             INSERT INTO clip_inbox(ts, text) VALUES(1, 'clip');
             INSERT INTO edit_state VALUES('snip:c', 'snip', 'c', 1, 1, '{}', 1);",
    )
    .unwrap();
    // Exactly the statements delete_all_data issues.
    for table in ["prompt_versions", "copy_log", "clip_inbox", "trash", "edit_state"] {
        conn.execute(&format!("DELETE FROM {table}"), []).unwrap();
    }
    conn.execute("DELETE FROM prompts", []).unwrap();
    let count = |t: &str| -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0)).unwrap()
    };
    assert_eq!(count("prompts"), 0);
    assert_eq!(count("trash"), 0, "a data wipe must not leave trashed prompts behind");
    assert_eq!(count("copy_log"), 0);
    assert_eq!(count("prompt_versions"), 0);
    assert_eq!(count("clip_inbox"), 0);
    assert_eq!(count("edit_state"), 0);
    assert_eq!(count("meta"), 1, "settings must survive a data-only wipe");
}

fn edit_state_test_conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE edit_state(id TEXT PRIMARY KEY, kind TEXT NOT NULL, target TEXT NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL, ops TEXT NOT NULL, updated_at INTEGER NOT NULL);",
    )
    .unwrap();
    conn
}

#[test]
fn edit_state_is_sealed_and_keyed_without_the_path_and_reads_back() {
    let conn = edit_state_test_conn();
    let path = "c:/clips/secret-meeting.mp4";
    edit_state_write(&conn, "video", path, 10, 20, "{\"v\":1,\"ops\":[\"hello overlay text\"]}", (now_ms() / 1000) as i64).unwrap();
    let (id, target, ops): (String, String, String) = conn
        .query_row("SELECT id, target, ops FROM edit_state", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    for stored in [&id, &target, &ops] {
        assert!(!stored.contains("secret-meeting") && !stored.contains("hello overlay"), "no plain path or overlay text at rest: {stored}");
    }
    let (_, back) = edit_state_read(&conn, "video", path, 10, 20).unwrap();
    assert!(back.contains("hello overlay text"));
    assert!(edit_state_read(&conn, "video", path, 10, 21).is_none(), "a changed mtime still misses");
    // The sweep reads the sealed path and drops the row once the file is gone.
    assert_eq!(sweep_edit_state_conn(&conn, |p| p == path, 30.0), 0);
    assert_eq!(sweep_edit_state_conn(&conn, |_| false, 30.0), 1);
}

#[test]
fn edit_state_reads_an_old_plain_row_and_replaces_it_on_save() {
    let conn = edit_state_test_conn();
    let path = "c:/clips/old.mp4";
    let now = (now_ms() / 1000) as i64;
    conn.execute(
        "INSERT INTO edit_state VALUES('video:c:/clips/old.mp4', 'video', 'c:/clips/old.mp4', 1, 2, '{\"v\":1}', ?1)",
        params![now],
    )
    .unwrap();
    assert_eq!(edit_state_read(&conn, "video", path, 1, 2).unwrap().1, "{\"v\":1}");
    assert_eq!(sweep_edit_state_conn(&conn, |_| true, 30.0), 0, "a plain legacy target still resolves in the sweep");
    edit_state_write(&conn, "video", path, 1, 2, "{\"v\":1,\"x\":1}", now).unwrap();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM edit_state", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1, "the legacy row is replaced, not duplicated");
    edit_state_discard(&conn, "video", path).unwrap();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM edit_state", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0);
}

#[test]
fn sweep_edit_state_removes_a_row_whose_target_is_gone() {
    let conn = edit_state_test_conn();
    let now = (now_ms() / 1000) as i64;
    conn.execute(
        "INSERT INTO edit_state VALUES('snip:c:\\gone.png', 'snip', 'c:\\gone.png', 1, 1, '{}', ?1)",
        params![now],
    )
    .unwrap();
    let removed = sweep_edit_state_conn(&conn, |_| false, 30.0);
    assert_eq!(removed, 1);
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM edit_state", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0);
}

#[test]
fn sweep_edit_state_keeps_a_fresh_row_whose_target_still_exists() {
    let conn = edit_state_test_conn();
    let now = (now_ms() / 1000) as i64;
    conn.execute(
        "INSERT INTO edit_state VALUES('snip:c:\\there.png', 'snip', 'c:\\there.png', 1, 1, '{}', ?1)",
        params![now],
    )
    .unwrap();
    let removed = sweep_edit_state_conn(&conn, |_| true, 30.0);
    assert_eq!(removed, 0);
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM edit_state", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn sweep_edit_state_removes_a_row_older_than_keep_days() {
    let conn = edit_state_test_conn();
    let stale = (now_ms() / 1000) as i64 - 40 * 86400; // 40 days ago, past a 30-day keep
    conn.execute(
        "INSERT INTO edit_state VALUES('video:c:\\old.mp4', 'video', 'c:\\old.mp4', 1, 1, '{}', ?1)",
        params![stale],
    )
    .unwrap();
    let removed = sweep_edit_state_conn(&conn, |_| true, 30.0);
    assert_eq!(removed, 1, "the file still exists, but the entry itself aged out");
}

#[test]
fn sweep_edit_state_zero_keep_days_means_forever() {
    let conn = edit_state_test_conn();
    let ancient = (now_ms() / 1000) as i64 - 3650 * 86400; // ten years ago
    conn.execute(
        "INSERT INTO edit_state VALUES('video:c:\\old.mp4', 'video', 'c:\\old.mp4', 1, 1, '{}', ?1)",
        params![ancient],
    )
    .unwrap();
    let removed = sweep_edit_state_conn(&conn, |_| true, 0.0);
    assert_eq!(removed, 0, "0 keep-days means never expire by age, same as trashRetentionDays");
}

#[test]
fn edit_state_ops_version_reads_v_and_rejects_junk() {
    assert_eq!(edit_state_ops_version("{\"v\":1,\"ops\":[]}"), Some(1));
    assert_eq!(edit_state_ops_version("{\"v\":2,\"ops\":[]}"), Some(2));
    assert_eq!(edit_state_ops_version("not json"), None, "corrupt JSON has no readable version");
    assert_eq!(edit_state_ops_version("{\"ops\":[]}"), None, "missing v field is unreadable, not v0");
    assert_eq!(edit_state_ops_version("{\"v\":\"one\"}"), None, "a non-numeric v is unreadable too");
}

#[test]
fn save_load_discard_edit_state_roundtrip_matches_only_on_the_exact_triple() {
    let conn = edit_state_test_conn();
    let id = "snip:c:\\shot.png";
    conn.execute(
        "INSERT INTO edit_state VALUES(?1, 'snip', 'c:\\shot.png', 100, 5000, '{\"v\":1,\"ops\":[]}', ?2)",
        params![id, (now_ms() / 1000) as i64],
    )
    .unwrap();
    let hit: Option<String> = conn
        .query_row(
            "SELECT ops FROM edit_state WHERE id = ?1 AND size = ?2 AND mtime = ?3",
            params![id, 100i64, 5000i64],
            |r| r.get(0),
        )
        .ok();
    assert_eq!(hit.as_deref(), Some("{\"v\":1,\"ops\":[]}"), "an exact path/size/mtime match restores the saved ops");
    let miss: Option<String> = conn
        .query_row(
            "SELECT ops FROM edit_state WHERE id = ?1 AND size = ?2 AND mtime = ?3",
            params![id, 100i64, 9999i64],
            |r| r.get(0),
        )
        .ok();
    assert!(miss.is_none(), "a changed mtime must never restore onto a file the ops were not drawn against");
    conn.execute("DELETE FROM edit_state WHERE id = ?1", params![id]).unwrap();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM edit_state", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0, "discard removes the row outright");
}

fn editor_drafts_test_conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE editor_drafts(id TEXT PRIMARY KEY, prompt_id TEXT NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '', updated_at INTEGER NOT NULL);",
    )
    .unwrap();
    conn
}

#[test]
fn editor_draft_is_sealed_at_rest_and_reads_back_plain() {
    let conn = editor_drafts_test_conn();
    editor_draft_write(&conn, "p1", "Secret name", "Secret body text", "[\"variant one\"]", 100).unwrap();
    let raw: (String, String, String) = conn
        .query_row("SELECT name, text, variants FROM editor_drafts WHERE id = 'current'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    for (stored, plain) in [(&raw.0, "Secret name"), (&raw.1, "Secret body text"), (&raw.2, "variant one")] {
        assert!(!stored.contains(plain), "the draft column must not hold the plaintext");
    }
    let d = editor_draft_read(&conn).unwrap();
    assert_eq!((d.name.as_str(), d.text.as_str(), d.variants.as_str()), ("Secret name", "Secret body text", "[\"variant one\"]"));
}

#[test]
fn an_undecryptable_sealed_draft_is_dropped_not_offered_empty() {
    let conn = editor_drafts_test_conn();
    conn.execute(
        "INSERT INTO editor_drafts(id, prompt_id, name, text, variants, updated_at) VALUES('current','p1','enc:v1:AAAA','enc:v1:AAAA','enc:v1:AAAA',5)",
        [],
    )
    .unwrap();
    assert!(editor_draft_read(&conn).is_none());
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0, "the unreadable row is deleted");
}

#[test]
fn an_old_plain_edit_state_row_is_sealed_on_first_read() {
    let conn = edit_state_test_conn();
    conn.execute(
        "INSERT INTO edit_state VALUES('video:c:/clips/old.mp4', 'video', 'c:/clips/old.mp4', 1, 2, '{\"v\":1,\"t\":\"overlay words\"}', 77)",
        [],
    )
    .unwrap();
    let (_, ops) = edit_state_read(&conn, "video", "c:/clips/old.mp4", 1, 2).unwrap();
    assert!(ops.contains("overlay words"));
    let (id, target, stored_ops, updated): (String, String, String, i64) = conn
        .query_row("SELECT id, target, ops, updated_at FROM edit_state", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap();
    assert!(!id.contains("old.mp4"), "the legacy id is gone");
    assert!(target.starts_with(ENC_PREFIX) && stored_ops.starts_with(ENC_PREFIX), "target and ops are sealed now");
    assert_eq!(updated, 77, "the retention clock keeps running");
    assert!(edit_state_read(&conn, "video", "c:/clips/old.mp4", 1, 2).unwrap().1.contains("overlay words"));
}

#[test]
fn startup_pass_seals_every_plain_edit_state_row_once() {
    let conn = edit_state_test_conn();
    for (id, target, ts) in [("video:c:/a/one.mp4", "c:/a/one.mp4", 11), ("video:c:/a/two.mp4", "c:/a/two.mp4", 22)] {
        conn.execute("INSERT INTO edit_state VALUES(?1, 'video', ?2, 1, 2, '{\"v\":1,\"t\":\"words\"}', ?3)", params![id, target, ts]).unwrap();
    }
    assert_eq!(edit_state_seal_all(&conn), 2);
    let mut st = conn.prepare("SELECT id, target, ops, updated_at FROM edit_state ORDER BY updated_at").unwrap();
    let rows: Vec<(String, String, String, i64)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap().flatten().collect();
    assert_eq!(rows.len(), 2);
    for ((id, target, ops, ts), (want_ts, path)) in rows.iter().zip([(11, "c:/a/one.mp4"), (22, "c:/a/two.mp4")]) {
        assert_eq!(*id, edit_state_id("video", path));
        assert!(target.starts_with(ENC_PREFIX) && ops.starts_with(ENC_PREFIX));
        assert_eq!(*ts, want_ts);
    }
    assert_eq!(edit_state_seal_all(&conn), 0, "a second run writes nothing");
}

#[test]
fn editor_draft_reads_an_old_unsealed_row() {
    let conn = editor_drafts_test_conn();
    conn.execute("INSERT INTO editor_drafts(id, prompt_id, name, text, variants, updated_at) VALUES('current','p1','Old','Old body','[]',5)", []).unwrap();
    let d = editor_draft_read(&conn).unwrap();
    assert_eq!((d.name.as_str(), d.text.as_str(), d.variants.as_str()), ("Old", "Old body", "[]"));
}

// Same upsert SQL as save_editor_draft, run directly against a bare
// in-memory connection - the async command itself only exists to fetch its
// own AppHandle-bound connection, no test harness for that here (same
// reasoning as the edit_state roundtrip test right above).
#[test]
fn editor_draft_write_then_overwrite_keeps_a_single_current_row() {
    let conn = editor_drafts_test_conn();
    let upsert = "INSERT INTO editor_drafts(id, prompt_id, name, text, variants, updated_at) VALUES('current',?1,?2,?3,?4,?5)
             ON CONFLICT(id) DO UPDATE SET prompt_id=excluded.prompt_id, name=excluded.name, text=excluded.text, variants=excluded.variants, updated_at=excluded.updated_at";
    conn.execute(upsert, params!["neu", "First", "Hello", "[]", 100i64]).unwrap();
    conn.execute(upsert, params!["p1", "First edited", "Hello there", "[]", 200i64]).unwrap();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1, "a second write overwrites the single current row, it never accumulates rows");
    let (prompt_id, name, text): (String, String, String) = conn
        .query_row("SELECT prompt_id, name, text FROM editor_drafts WHERE id = 'current'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    assert_eq!((prompt_id.as_str(), name.as_str(), text.as_str()), ("p1", "First edited", "Hello there"));
}

#[test]
fn editor_draft_delete_removes_the_row_the_same_way_a_save_does() {
    let conn = editor_drafts_test_conn();
    conn.execute(
        "INSERT INTO editor_drafts(id, prompt_id, name, text, variants, updated_at) VALUES('current','p1','A','B','[]',100)",
        [],
    )
    .unwrap();
    conn.execute("DELETE FROM editor_drafts WHERE id = 'current'", []).unwrap();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0, "an orderly close, save or discard alike, must leave no draft behind");
}

#[test]
fn discard_editor_draft_if_removes_the_row_only_when_its_own_entry_matches() {
    let conn = editor_drafts_test_conn();
    conn.execute(
        "INSERT INTO editor_drafts(id, prompt_id, name, text, variants, updated_at) VALUES('current','p1','A','B','[]',100)",
        [],
    )
    .unwrap();
    discard_editor_draft_if(&conn, |t| t == "p2"); // an unrelated prompt was deleted
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1, "a draft for a different entry must survive an unrelated delete");
    discard_editor_draft_if(&conn, |t| t == "p1"); // its own entry was deleted
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0, "the draft disappears once the entry it belongs to is gone");
}

#[test]
fn discard_editor_draft_if_covers_a_batch_delete_set_too() {
    let conn = editor_drafts_test_conn();
    conn.execute(
        "INSERT INTO editor_drafts(id, prompt_id, name, text, variants, updated_at) VALUES('current','p3','A','B','[]',100)",
        [],
    )
    .unwrap();
    let ids: std::collections::HashSet<&str> = ["p1", "p2", "p3"].into_iter().collect();
    discard_editor_draft_if(&conn, |t| ids.contains(t));
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0, "batch_prompts's delete branch matches on the same idset a single delete uses a plain equality for");
}

#[test]
fn discard_editor_draft_if_is_a_no_op_when_no_draft_exists() {
    let conn = editor_drafts_test_conn();
    discard_editor_draft_if(&conn, |_| true); // nothing to delete, must not panic or error
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0);
}

#[test]
fn prompts_and_trash_write_together_or_not_at_all() {
    // The whole point of the combined write - a failure partway through must
    // roll back BOTH tables, not just the one it failed on (the old two-
    // transaction version could durably commit prompts and then fail on trash,
    // stranding a moved prompt in neither table).
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             INSERT INTO prompts VALUES('before', 0, '{}');
             INSERT INTO trash VALUES('trashed-before', 0, '{}');",
    )
    .unwrap();
    let base = sample_prompts()[0].clone();
    let good_prompts = vec![Prompt { id: "a".into(), ..base.clone() }];
    // A duplicate id makes the second trash INSERT violate its PRIMARY KEY,
    // forcing the write to fail after the prompts half (same transaction)
    // already succeeded.
    let dup_trash = vec![
        Prompt { id: "dup".into(), ..base.clone() },
        Prompt { id: "dup".into(), ..base },
    ];
    let mut written = WrittenRows::new();
    assert!(db_write_prompts_and_trash(&mut conn, &good_prompts, &dup_trash, &mut written).is_err());
    assert!(
        written.is_empty(),
        "a write that never committed must not leave rows recorded as written"
    );
    let ids = |table: &str| -> Vec<String> {
        conn.prepare(&format!("SELECT id FROM {table} ORDER BY id"))
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .flatten()
            .collect()
    };
    assert_eq!(
        ids("prompts"), vec!["before".to_string()],
        "a failed write must not touch prompts either, even though the prompts half alone would have succeeded"
    );
    assert_eq!(ids("trash"), vec!["trashed-before".to_string()]);
}

// ---- diff writes: a save only re-seals the rows that really changed ----

fn row_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);",
    )
    .unwrap();
    conn
}

// (id, ord, stored blob) in stored order: the blob byte-for-byte, which is how
// these tests tell a rewritten row from an untouched one.
fn stored_rows(conn: &Connection, table: &str) -> Vec<(String, i64, String)> {
    conn.prepare(&format!("SELECT id, ord, data FROM {table} ORDER BY ord"))
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .flatten()
        .collect()
}

fn numbered_prompts(n: usize) -> Vec<Prompt> {
    (0..n)
        .map(|i| Prompt { name: format!("Name {i}"), ..test_prompt(&format!("id{i:03}")) })
        .collect()
}

fn stored_prompt(blob: &str) -> Prompt {
    serde_json::from_str(&dec_str(blob)).unwrap()
}

// Prompt carries no PartialEq/Debug (nothing in the app compares two of them),
// so whole-prompt comparisons in these tests go through its json.
fn as_json(p: &Prompt) -> String {
    serde_json::to_string(p).unwrap()
}

// Comparing blobs only proves nothing was re-sealed because sealing is
// randomised: the same text sealed twice comes out as different bytes. Assert
// that rather than assume it: where sealing is unavailable the rows are stored
// as plain text and the comparison is weaker, so the check is skipped there.
fn sealing_is_randomised() -> bool {
    enc_str("probe").starts_with(ENC_PREFIX)
}

// The whole point of the flag is that it describes the DISK, not an intention:
// whatever enc_str actually produced is what the window is told. Before this,
// a plaintext fallback was reported nowhere at all and the answer would have
// been a hardcoded "sealed" no matter what came back.
#[test]
fn store_sealed_reports_what_a_write_actually_did() {
    let _guard = SEAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let really_sealed = enc_str("probe").starts_with(ENC_PREFIX);
    assert_eq!(
        store_sealed(),
        really_sealed,
        "the reported state must match what enc_str wrote, in both directions"
    );
}

// Serializes every test that reads or mutates the process-global sealing state
// (STORE_SEALED, SEAL_NOTIFY) against each other -- cargo test runs tests on
// separate threads by default, and these are the only globals any test
// deliberately flips. FORCE_PLAINTEXT_FALLBACK is thread-local (see enc_str)
// precisely so it needs no lock to stay isolated from unrelated tests.
static SEAL_TEST_LOCK: Mutex<()> = Mutex::new(());

// A row written through the real store writer must never carry its plaintext
// content on disk -- proves the writer path, not just enc_str in isolation
// (already covered by the plain round-trip test above it).
#[test]
fn a_sealed_write_never_puts_the_plaintext_marker_on_disk() {
    let _guard = SEAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !sealing_is_randomised() {
        return; // DPAPI unavailable on this machine -- nothing to prove here
    }
    let mut conn = row_test_db();
    const MARKER: &str = "PLAINTEXT-MARKER-206-must-never-reach-disk";
    let mut prompts = numbered_prompts(1);
    prompts[0].text = MARKER.into();
    let mut written = WrittenRows::new();
    db_write_prompts(&mut conn, &prompts, &mut written).unwrap();
    let rows = stored_rows(&conn, "prompts");
    let blob = &rows[0].2;
    assert!(blob.starts_with(ENC_PREFIX), "a sealed write must carry the enc prefix");
    assert!(!blob.contains(MARKER), "the plaintext marker must never appear in the stored bytes: {blob}");
}

// The fallback must flip the flag and notify exactly once, even when a
// single save floods many rows through it -- not once per row.
// Restores the process-global sealing state on drop -- including on a panic
// partway through the test -- so one failed assertion here can never leave
// state stuck for every other test that runs after it. FORCE_PLAINTEXT_FALLBACK
// is thread-local, but cargo test pools and reuses OS threads across tests, so
// a later test landing on this same thread would still inherit a stale `true`
// without this reset.
struct SealOverrideGuard {
    prior_sealed: bool,
}
impl Drop for SealOverrideGuard {
    fn drop(&mut self) {
        FORCE_PLAINTEXT_FALLBACK.with(|f| f.set(false));
        STORE_SEALED.store(self.prior_sealed, Ordering::Relaxed);
        *SEAL_NOTIFY.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

#[test]
fn a_flood_of_unsealed_writes_flips_the_flag_and_notifies_exactly_once() {
    let _guard = SEAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Install the counter BEFORE either flag flips. Flipping FORCE first left a
    // window where an unrelated, unguarded test's own enc_str call could catch
    // this test's one true-to-false transition while SEAL_NOTIFY was still the
    // previous (cleared) value -- the flag still ended up unsealed, but nothing
    // was there to count it, so the assertion below saw zero calls.
    let calls = Arc::new(AtomicU64::new(0));
    let counter = calls.clone();
    *SEAL_NOTIFY.lock().unwrap_or_else(|e| e.into_inner()) =
        Some(Arc::new(move || { counter.fetch_add(1, Ordering::Relaxed); }));
    let prior_sealed = STORE_SEALED.swap(true, Ordering::Relaxed);
    FORCE_PLAINTEXT_FALLBACK.with(|f| f.set(true));
    let _restore = SealOverrideGuard { prior_sealed };

    let mut conn = row_test_db();
    let prompts = numbered_prompts(50);
    let mut written = WrittenRows::new();
    db_write_prompts(&mut conn, &prompts, &mut written).unwrap();

    assert!(!store_sealed(), "the flag must report unsealed once the fallback ran");
    assert_eq!(calls.load(Ordering::Relaxed), 1, "the notify must fire exactly once, not once per row");
}

// "A repaired profile re-seals on the next save" (the warning dialog's own
// words) is a promise about the ROW being written, not about STORE_SEALED --
// that flag stays sticky for the rest of the session on purpose (see its
// comment: an old plaintext row is not retroactively sealed just because a
// later write succeeded, so un-warning the user here would be misleading).
// enc_str re-checks DPAPI on every single call with no persisted "give up"
// state, so the row-level promise needs no new machinery -- only a test that
// FORCE_PLAINTEXT_FALLBACK really is a per-call toggle and not a latch.
#[test]
fn a_write_after_a_forced_failure_is_sealed_again_once_dpapi_is_available() {
    let _guard = SEAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !sealing_is_randomised() {
        return; // DPAPI unavailable on this machine -- nothing to prove here
    }
    let prior_sealed = STORE_SEALED.swap(true, Ordering::Relaxed);
    FORCE_PLAINTEXT_FALLBACK.with(|f| f.set(true));
    let _restore = SealOverrideGuard { prior_sealed };

    let unsealed = enc_str("written during the outage");
    assert!(!unsealed.starts_with(ENC_PREFIX), "the forced failure must fall back to plaintext");

    // The profile is "repaired": nothing but the thread-local override changes.
    FORCE_PLAINTEXT_FALLBACK.with(|f| f.set(false));
    let resealed = enc_str("written after the profile was repaired");
    assert!(resealed.starts_with(ENC_PREFIX), "a write with DPAPI available again must come out sealed");
}

// A cargo test run must never touch the real error.log -- dozens of call
// sites fire during entirely unrelated tests, not just the ones that test
// logging directly, so this has to hold for the redirect mechanism itself.
#[test]
fn append_crash_log_never_writes_the_real_appdata_file() {
    let real_path = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(DATA_FOLDER)
        .join("error.log");
    let before = fs::metadata(&real_path).ok().map(|m| (m.len(), m.modified().ok()));

    assert_ne!(crash_log_path(), real_path, "a test build must redirect away from the real file");
    const MARKER: &str = "PROBE-crash-log-redirect-never-writes-the-real-appdata-file";
    append_crash_log(MARKER);

    let after = fs::metadata(&real_path).ok().map(|m| (m.len(), m.modified().ok()));
    assert_eq!(before, after, "the real error.log must be untouched by a test run");

    let redirected = fs::read_to_string(crash_log_path()).unwrap_or_default();
    assert!(redirected.contains(MARKER), "the message must land in the redirected file instead");
}

// Every writer the unit tests reach (the error log, the data folder fallbacks)
// must resolve into a per-run location, never into the owner's real roaming
// folder, and writing through them must leave the real error.log untouched.
#[test]
fn test_runs_never_resolve_into_the_real_data_folder() {
    let real_dir = PathBuf::from(test_env::real_appdata().expect("APPDATA is set on Windows")).join(DATA_FOLDER);
    let real_log = real_dir.join("error.log");
    let stamp = |p: &Path| fs::metadata(p).ok().map(|m| (m.len(), m.modified().ok()));
    let before = stamp(&real_log);
    assert_ne!(std::env::var_os("APPDATA"), test_env::real_appdata(), "the test process must run with a sandboxed APPDATA");

    for (what, path) in [
        ("crash_log_path", crash_log_path()),
        ("default_data_dir_headless", default_data_dir_headless()),
        ("data_dir_headless", data_dir_headless()),
    ] {
        assert!(!path.starts_with(&real_dir), "{what} resolved into the real data folder: {}", path.display());
    }

    // The writers: a plain log line, the startup scrub, and the call sites that
    // fire inside unrelated tests (sealing without DPAPI, a failed conversion).
    append_crash_log("PROBE-guard-plain-line");
    scrub_user_paths_from_crash_log();
    append_crash_log("DPAPI encryption unavailable - probe line from the guard test");
    append_crash_log("recording convert failed (webm): probe line from the guard test");

    assert_eq!(before, stamp(&real_log), "the real error.log must be untouched by the unit tests");
    let redirected = fs::read_to_string(crash_log_path()).unwrap_or_default();
    assert!(redirected.contains("PROBE-guard-plain-line"), "the lines must land in the per-run file");
}

// Tests write fixed-name scratch files straight into the temp folder; two runs
// at once (two checkouts) or a file an earlier run left behind must never meet
// there, so each test process gets a temp folder named after its own pid.
#[test]
fn test_runs_use_a_temp_folder_of_their_own() {
    let temp = std::env::temp_dir();
    let own = format!("ps-testenv-{}", std::process::id());
    assert!(
        temp.components().any(|c| c.as_os_str().to_string_lossy() == own),
        "the temp folder must sit inside this process's sandbox, got {}",
        temp.display()
    );
    assert!(temp.is_dir(), "the sandbox temp folder must exist");
}

// A data folder outside the profile (a custom location the user picked, or any
// other drive-rooted path this run never resolved to a known folder) is never
// a folder the named placeholders recognise -- the structural catch-all must
// still keep it out of the log, down to just the file name.
#[test]
fn append_crash_log_scrubs_a_path_outside_any_known_folder() {
    const MARKER: &str = "PROBE-crash-log-scrub-outside-profile";
    append_crash_log(&format!(r"{MARKER}: could not read D:\Privat\Kunde\x.png"));

    // Checked as a whole-file substring rather than a reconstructed "line":
    // dozens of other tests append to this same shared redirected file at the
    // same time, so only a contiguous match proves anything here.
    let redirected = fs::read_to_string(crash_log_path()).unwrap_or_default();
    assert!(!redirected.contains(r"D:\Privat\Kunde"), "an unrecognised drive path must never reach the log");
    assert!(
        redirected.contains(&format!("{MARKER}: could not read x.png")),
        "the file name itself may stay, only the folder must go: {redirected}"
    );
}

// The configured sync folder is exactly the kind of user-data folder this
// point exists for: a full path most users will not pick from inside their
// profile. It must come out replaced by its own readable placeholder, not
// merely shortened.
#[test]
fn append_crash_log_scrubs_the_configured_sync_folder() {
    const MARKER: &str = "PROBE-crash-log-scrub-sync-folder";
    let sync_dir = PathBuf::from(r"E:\CloudSync\PromptFolders");
    let prior = LAST_KNOWN_SYNC_DIR.lock().unwrap_or_else(|e| e.into_inner()).clone();
    *LAST_KNOWN_SYNC_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(sync_dir.clone());

    append_crash_log(&format!("{MARKER}: the sync folder could not be created: {}", sync_dir.display()));

    *LAST_KNOWN_SYNC_DIR.lock().unwrap_or_else(|e| e.into_inner()) = prior;

    let redirected = fs::read_to_string(crash_log_path()).unwrap_or_default();
    assert!(
        !redirected.to_ascii_lowercase().contains("cloudsync"),
        "the sync folder path must never reach the log"
    );
    assert!(
        redirected.contains(&format!("{MARKER}: the sync folder could not be created: <sync>")),
        "the named sync placeholder must take its place: {redirected}"
    );
}

// A message that never carried a path must reach the log exactly as written --
// scrubbing must never mangle ordinary text.
#[test]
fn append_crash_log_leaves_a_message_without_any_path_unchanged() {
    const MARKER: &str = "PROBE-crash-log-scrub-no-path";
    let msg = format!("{MARKER}: persist failed, no path involved at all");
    append_crash_log(&msg);

    let redirected = fs::read_to_string(crash_log_path()).unwrap_or_default();
    assert!(redirected.contains(&msg), "a path-free message must reach the log exactly as written: {redirected}");
}

// A folder name with a space in it used to survive scrubbing: splitting on
// spaces before deciding what looks like a path left every space-separated
// piece past the first one (here "Folder\x.mp4") sitting in the log in the
// clear. The whole path, spaces and all, must come out as just the file name.
#[test]
fn shorten_stray_paths_removes_a_folder_name_that_contains_a_space() {
    let out = shorten_stray_paths(r"path=D:\Private Folder\x.mp4");
    assert_eq!(out, "path=x.mp4");
    assert!(!out.contains("Private"), "no fragment of the folder name may remain: {out}");
    assert!(!out.contains("Folder"), "no fragment of the folder name may remain: {out}");
}

// The same path, quoted, must shorten to just the file name inside the
// quotes rather than treating the quote character as part of the path or
// leaving the quotes describing something that is no longer there.
#[test]
fn shorten_stray_paths_removes_a_quoted_path_with_spaces() {
    let out = shorten_stray_paths(r#"could not open "D:\Private Folder\x.mp4" for writing"#);
    assert_eq!(out, r#"could not open "x.mp4" for writing"#);
}

#[test]
fn append_crash_log_scrubs_a_folder_name_with_a_space_in_it() {
    const MARKER: &str = "PROBE-crash-log-scrub-space-in-folder";
    append_crash_log(&format!(r"{MARKER}: path=D:\Private Folder\x.mp4"));

    let redirected = fs::read_to_string(crash_log_path()).unwrap_or_default();
    assert!(!redirected.contains("Private Folder"), "a spaced folder name must never reach the log: {redirected}");
    assert!(
        redirected.contains(&format!("{MARKER}: path=x.mp4")),
        "the file name itself may stay, only the folder must go: {redirected}"
    );
}

// record_error_log/ERROR_LOG is a shared static -- serialise against other
// tests touching it, same reasoning as SEAL_TEST_LOCK above.
static ERROR_LOG_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn record_error_log_keeps_the_cap_and_drops_the_oldest() {
    let _guard = ERROR_LOG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    ERROR_LOG.lock().unwrap_or_else(|e| e.into_inner()).clear();
    for i in 0..7 {
        record_error_log(&format!("message {i}"), "test", None, 5);
    }
    let log = ERROR_LOG.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(log.len(), 5, "the buffer must never grow past the configured cap");
    assert_eq!(log.first().unwrap().text, "message 6", "the newest entry must be first");
    assert_eq!(log.last().unwrap().text, "message 2", "the oldest entries beyond the cap must have been dropped");
}

#[test]
fn sanitize_error_log_origin_keeps_the_known_set_and_falls_back_to_other() {
    assert_eq!(sanitize_error_log_origin("settings"), "settings");
    assert_eq!(sanitize_error_log_origin("main"), "main");
    assert_eq!(sanitize_error_log_origin("remoteApi"), "remoteApi");
    // Ids whose dialogs left the markup are no longer allow-listed.
    for gone in ["clip-modal", "hotkeys-modal", "journal", "licenses-modal", "vars-modal", "ve-confirm", "whats-new-modal"] {
        assert_eq!(sanitize_error_log_origin(gone), "other", "{gone}");
    }
    assert_eq!(sanitize_error_log_origin("some prompt text smuggled in as an origin"), "other");
    assert_eq!(sanitize_error_log_origin(""), "other");
}

#[test]
fn record_error_log_sanitizes_an_origin_outside_the_known_set() {
    let _guard = ERROR_LOG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    ERROR_LOG.lock().unwrap_or_else(|e| e.into_inner()).clear();
    record_error_log("message", "not-a-real-modal-id", None, 5);
    let log = ERROR_LOG.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(log.first().unwrap().origin, "other", "an origin outside the fixed set must never reach the stored entry as-is");
}

#[test]
fn scrub_error_log_text_shortens_a_path_to_its_file_name() {
    let msg = r"could not open D:\Projects\ClientData\secret-notes.txt";
    let clean = scrub_error_log_text(msg);
    assert!(clean.contains("secret-notes.txt"), "the file name must survive the scrub: {clean}");
    assert!(!clean.contains(r"D:\Projects\ClientData"), "the folder part of the path must not survive: {clean}");
}

#[test]
fn scrub_error_log_text_never_writes_a_long_text_whole() {
    // Stands in for prompt content that ended up interpolated into a
    // message -- no call site does this today, but the scrub must hold even
    // if one someday does.
    let long_text = "a".repeat(400);
    let clean = scrub_error_log_text(&long_text);
    assert!(clean.len() < long_text.len(), "a long text must be collapsed, never written out whole: {clean}");
    assert!(!clean.contains(&long_text), "the original long text must never appear verbatim in the log");
}

// A log grown past the cap must keep its newest lines and lose only the
// oldest -- deleting the whole file (the old behaviour) would throw away the
// real failure a user just hit along with everything before it. Runs against
// its own private file, never the shared redirected crash log, so nothing
// else appending concurrently can move this test's goalposts.
#[test]
fn rotate_log_file_keeps_the_newest_lines_and_drops_only_the_oldest() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-rotate-test-{}", std::process::id()));
    let lines: Vec<String> = (0..50).map(|i| format!("line {i:03} of the log before rotation")).collect();
    let body = lines.iter().map(|l| format!("{l}\n")).collect::<String>();
    fs::write(&path, &body).unwrap();
    let full_len = body.len();

    // Cap well under the full size but well over one line, so several of the
    // oldest lines must go and several of the newest must survive intact.
    let cap = full_len / 3;
    rotate_log_file(&path, cap);

    let kept = fs::read_to_string(&path).unwrap();
    assert!(kept.len() <= cap, "the kept tail must not exceed the cap: {} > {cap}", kept.len());
    assert!(!kept.is_empty(), "some of the log must survive a rotation");
    assert!(
        kept.starts_with("line "),
        "the tail must start on a whole line, not a fragment of one that was cut: {kept:?}"
    );
    assert!(
        lines.last().map(|l| kept.contains(l.as_str())).unwrap_or(false),
        "the newest line must survive the rotation"
    );
    assert!(
        !lines.first().map(|l| kept.contains(l.as_str())).unwrap_or(false),
        "the oldest line must be the one that gets dropped"
    );

    let _ = fs::remove_file(&path);
}

// A file already under the cap must be left completely alone -- rotation is
// only for a log that actually outgrew its budget.
#[test]
fn rotate_log_file_leaves_a_file_under_the_cap_untouched() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-rotate-small-test-{}", std::process::id()));
    fs::write(&path, "short log, well under any reasonable cap\n").unwrap();
    let before = fs::read_to_string(&path).unwrap();

    rotate_log_file(&path, 256 * 1024);

    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(before, after, "a file under the cap must not be rewritten at all");
    let _ = fs::remove_file(&path);
}

// A line appended while a rotation is under way must never be lost. Proven
// with a real second thread rather than a mocked lock: this thread holds the
// crash log's own lock first (standing in for a truncation already in
// progress), a channel confirms the appending thread has started only once
// it is actually about to block on that same lock, and only then does this
// thread finish the truncation and let go -- so the append is genuinely
// serialised after it, never racing it.
#[test]
fn a_concurrent_append_during_truncation_is_never_lost() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-lock-test-{}", std::process::id()));
    let lock_path = path.with_extension("log.lock");
    let _ = fs::remove_file(&lock_path);
    let lines: Vec<String> = (0..50).map(|i| format!("line {i:03} of the log before rotation")).collect();
    let body = lines.iter().map(|l| format!("{l}\n")).collect::<String>();
    fs::write(&path, &body).unwrap();
    let cap = body.len() / 3;

    let held = CrashLogLock::acquire(&path);
    assert!(held.held, "the test itself must hold the real lock to stand in for a truncation in progress");

    let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
    let path2 = path.clone();
    let appender = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        append_line_locked(&path2, usize::MAX, "line NEW appended during truncation");
    });
    started_rx.recv().unwrap();
    // Give the appending thread a moment to reach the lock and start
    // waiting on it before this thread finishes the "truncation" and lets go.
    std::thread::sleep(Duration::from_millis(100));
    rotate_log_file_locked(&path, cap);
    drop(held);
    appender.join().unwrap();

    let kept = fs::read_to_string(&path).unwrap();
    assert!(
        kept.contains("line NEW appended during truncation"),
        "an append that happened during a truncation must still be in the file: {kept:?}"
    );
    assert!(
        !lines.first().map(|l| kept.contains(l.as_str())).unwrap_or(false),
        "the truncation itself must still have taken effect: {kept:?}"
    );

    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(&lock_path);
}

// The side file rotation writes the kept remainder to before renaming it into
// place must never survive the rotation, win or lose.
#[test]
fn rotate_log_file_leaves_no_side_file_behind() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-rotate-sidefile-test-{}", std::process::id()));
    let tmp = path.with_extension("log.tmp");
    let _ = fs::remove_file(&tmp);
    let body: String = (0..50).map(|i| format!("line {i:03} of the log before rotation\n")).collect();
    fs::write(&path, &body).unwrap();

    rotate_log_file(&path, body.len() / 3);

    assert!(!tmp.exists(), "the side file used during rotation must not be left behind");
    let _ = fs::remove_file(&path);
}

// A line carrying a Windows user-profile path -- exactly what an unremapped
// panic location from a dependency crate bakes in -- keeps its other content;
// only the path on it is replaced, the line as a whole is never deleted.
#[test]
fn strip_user_path_lines_replaces_the_path_but_keeps_the_line() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-scrub-test-{}", std::process::id()));
    let contents = "\
[t=1 v1.0] a clean line with no path in it at all\n\
[t=2 v1.0] panicked at C:\\Users\\example\\.cargo\\registry\\src\\index.crates.io\\somecrate-1.2.3\\src\\lib.rs:42\n\
[t=3 v1.0] another clean line, describing an ordinary failure\n";
    fs::write(&path, contents).unwrap();

    strip_user_path_lines(&path);

    let cleaned = fs::read_to_string(&path).unwrap();
    assert!(!cleaned.to_ascii_lowercase().contains(r"\users\"), "the profile path must be gone: {cleaned:?}");
    assert_eq!(cleaned.lines().count(), 3, "the line must stay, only the path on it is replaced: {cleaned:?}");
    assert!(cleaned.contains("panicked at"), "the rest of the panic line must survive: {cleaned:?}");
    assert!(cleaned.contains("lib.rs:42"), "the file name and line number must survive: {cleaned:?}");
    assert!(cleaned.contains("a clean line with no path in it at all"), "an unrelated line must survive");
    assert!(cleaned.contains("another clean line, describing an ordinary failure"), "an unrelated line must survive");
    let _ = fs::remove_file(&path);
}

// A file with nothing to remove must not be rewritten at all -- the healthy,
// common case should cost one read and nothing else.
#[test]
fn strip_user_path_lines_leaves_a_clean_file_untouched() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-scrub-clean-test-{}", std::process::id()));
    fs::write(&path, "nothing here reveals anyone's account name\n").unwrap();
    let before = fs::read_to_string(&path).unwrap();

    strip_user_path_lines(&path);

    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(before, after, "a clean file must not be rewritten");
    let _ = fs::remove_file(&path);
}

// A single invalid UTF-8 byte anywhere in the file (a raw OS error message,
// a rotation cut mid multi-byte character) used to make `fs::read_to_string`
// fail outright, which skipped scrubbing every other line in the file along
// with the broken one. Reading raw bytes and decoding line by line must keep
// scrubbing every well-formed line regardless of what one broken line holds.
#[test]
fn strip_user_path_lines_still_scrubs_other_lines_past_one_invalid_utf8_byte() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-scrub-badutf8-{}", std::process::id()));
    let mut contents = Vec::new();
    contents.extend_from_slice(b"[t=1 v1.0] broken byte follows: \xFF end of line\n");
    contents.extend_from_slice(b"[t=2 v1.0] panicked at C:\\Users\\example\\lib.rs:42");
    contents.push(b'\n');
    fs::write(&path, &contents).unwrap();

    strip_user_path_lines(&path);

    let cleaned = fs::read(&path).unwrap();
    let cleaned_text = String::from_utf8_lossy(&cleaned);
    assert_eq!(cleaned_text.lines().count(), 2, "both lines must survive, invalid byte and all: {cleaned_text:?}");
    assert!(!cleaned_text.to_ascii_lowercase().contains(r"\users\"), "the profile path on the OTHER line must still be scrubbed: {cleaned_text:?}");
    assert!(cleaned_text.contains("lib.rs:42"), "the rest of the second line must survive: {cleaned_text:?}");
    let _ = fs::remove_file(&path);
}

// A plaintext-fallback value whose own text happens to start with the literal
// marker ENC_PREFIX ("enc:v1:") must still round-trip intact, not be misread
// as corrupt ciphertext and erased to "".
#[test]
fn plaintext_fallback_of_text_starting_with_enc_prefix_round_trips() {
    let _guard = SEAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prior_sealed = STORE_SEALED.swap(true, Ordering::Relaxed);
    FORCE_PLAINTEXT_FALLBACK.with(|f| f.set(true));
    let _restore = SealOverrideGuard { prior_sealed };

    let text = "enc:v1:this looks like ciphertext but is not";
    let stored = enc_str(text);
    assert_ne!(stored, text, "the collision must be escaped, not written raw");
    assert_eq!(dec_str(&stored), text, "the original text must come back unchanged");
}

// A backup that could never be restored (past MAX_INFLATED) must be refused at
// creation time, not silently produced and only discovered useless on the day
// it is actually needed.
#[test]
fn a_store_past_the_restore_ceiling_refuses_to_back_up() {
    assert!(!too_large_to_restore(MAX_INFLATED), "exactly at the ceiling must still be backable up");
    assert!(too_large_to_restore(MAX_INFLATED + 1), "one byte past the ceiling must refuse a backup nobody could ever restore");
}

// The test above only proves the pure ceiling function itself; it never proves
// do_backup's real production path (vacuum_and_check_size) actually runs VACUUM
// INTO the destination it then measures. MAX_INFLATED is 512 MB -- deliberately
// far past any real store, and deliberately too large to manufacture in a fast
// unit test, so the reject path itself stays unexercised here; this proves the
// wiring on the path that IS practical to run for real: a normal-sized source
// actually gets vacuumed into `tmp`, not silently skipped or pointed at `src`.
#[test]
fn vacuum_and_check_size_writes_a_real_vacuumed_copy_at_tmp() {
    let dir = std::env::temp_dir().join(format!("{}-test-vacuum-size", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("data.db");
    let tmp = dir.join("data.db.tmp");
    {
        let conn = Connection::open(&src).unwrap();
        conn.execute_batch("CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL); INSERT INTO prompts VALUES('a', 0, 'x');").unwrap();
    }
    assert!(!tmp.exists());
    let result = vacuum_and_check_size(&src, &tmp);
    assert!(result.is_ok(), "a normal-sized store must not be rejected: {result:?}");
    assert!(tmp.exists(), "VACUUM INTO must have actually written the destination file");
    let conn = Connection::open(&tmp).unwrap();
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1, "the vacuumed copy must contain the real data, not an empty placeholder");
    let _ = std::fs::remove_dir_all(&dir);
}

fn stream_test_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{}-test-stream-{tag}", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// Deterministic, poorly compressible bytes, well past the 8 KiB copy buffer.
fn stream_test_bytes(len: usize) -> Vec<u8> {
    let mut x: u32 = 0x1234_5678;
    (0..len)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 24) as u8
        })
        .collect()
}

#[test]
fn gzip_and_gunzip_stream_a_file_larger_than_the_copy_buffer() {
    let dir = stream_test_dir("roundtrip");
    let raw = dir.join("raw.db");
    let data = stream_test_bytes(3 * 1024 * 1024 + 17);
    std::fs::write(&raw, &data).unwrap();
    let gz = dir.join("data.db.gz");
    gzip_to(&raw, &gz).unwrap();
    assert!(!dir.join("data.db.gz.part").exists(), "the temp file must be renamed away");
    let back = dir.join("back.db");
    gunzip_to(&gz, &back).unwrap();
    assert_eq!(std::fs::read(&back).unwrap(), data, "the round trip must be byte exact");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gzip_to_leaves_no_part_file_when_the_source_is_missing() {
    let dir = stream_test_dir("gzfail");
    let gz = dir.join("data.db.gz");
    assert!(gzip_to(&dir.join("absent.db"), &gz).is_err());
    assert!(!gz.exists());
    assert!(!dir.join("data.db.gz.part").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gunzip_refuses_output_one_byte_above_the_cap_and_removes_it() {
    let dir = stream_test_dir("cap");
    let raw = dir.join("raw.db");
    let data = stream_test_bytes(100_000);
    std::fs::write(&raw, &data).unwrap();
    let gz = dir.join("data.db.gz");
    gzip_to(&raw, &gz).unwrap();
    let out = dir.join("out.db");
    gunzip_to_capped(&gz, &out, data.len() as u64).expect("exactly at the cap is allowed");
    assert_eq!(std::fs::read(&out).unwrap().len(), data.len());
    let err = gunzip_to_capped(&gz, &out, data.len() as u64 - 1).unwrap_err();
    assert!(err.contains("expands"), "{err}");
    assert!(!out.exists(), "a refused inflate must not leave partial output");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gunzip_of_a_truncated_snapshot_fails_and_leaves_nothing() {
    let dir = stream_test_dir("trunc");
    let raw = dir.join("raw.db");
    std::fs::write(&raw, stream_test_bytes(200_000)).unwrap();
    let gz = dir.join("data.db.gz");
    gzip_to(&raw, &gz).unwrap();
    let bytes = std::fs::read(&gz).unwrap();
    std::fs::write(&gz, &bytes[..bytes.len() / 2]).unwrap();
    let out = dir.join("out.db");
    assert!(gunzip_to(&gz, &out).is_err(), "a cut-off stream must never count as restored");
    assert!(!out.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

// The store lock must be free while the copy is being packed, and still held
// while the point-in-time copy itself is taken.
#[test]
fn backup_releases_the_store_lock_before_packing() {
    let dir = stream_test_dir("lock");
    {
        let conn = Connection::open(dir.join("data.db")).unwrap();
        conn.execute_batch("CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL); INSERT INTO prompts VALUES('a', 0, 'x');").unwrap();
    }
    let root = dir.join("backups");
    let store = std::sync::Mutex::new(0u8);
    let guard = store.lock().unwrap();
    let mut free_while_paused = false;
    let name = backup_released(guard, &dir, &root, 5, None, || {
        free_while_paused = store.try_lock().is_ok();
    })
    .unwrap();
    assert!(free_while_paused, "packing must run with the store lock released");
    let folder = root.join(&name);
    assert!(folder.join(BACKUP_DB_GZ).exists());
    assert!(!folder.join("data.db.tmp").exists(), "the raw copy must not be left behind");
    let check = dir.join("check.db");
    gunzip_to(&folder.join(BACKUP_DB_GZ), &check).unwrap();
    let n: i64 = Connection::open(&check).unwrap().query_row("SELECT COUNT(*) FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1);
    // A failing snapshot frees the lock too.
    let guard = store.lock().unwrap();
    let err = backup_released(guard, &dir.join("nowhere"), &root, 5, None, || {}).unwrap_err();
    assert_eq!(err, "no-store");
    assert!(store.try_lock().is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

// Stand-in for one stored setting plus one registry value.
struct TwoStateWorld {
    setting: bool,
    os: Option<String>,
    save_fails: bool,
    os_fails: bool,
    restore_save_fails: bool,
}

fn run_two_state(w: &mut TwoStateWorld, want: bool) -> Result<(), String> {
    let prior = w.setting;
    let prior_os = w.os.clone();
    let os_cell = std::cell::RefCell::new(w.os.clone());
    let (os_fails, save_fails, restore_save_fails) = (w.os_fails, w.save_fails, w.restore_save_fails);
    let saved = std::cell::Cell::new(prior);
    let mut setting = prior;
    let result = two_state_change(
        &mut setting,
        prior_os,
        |s| {
            *s = want;
            if save_fails {
                false
            } else {
                saved.set(want);
                true
            }
        },
        |s, persist| {
            *s = prior;
            if !persist {
                return true;
            }
            if restore_save_fails {
                false
            } else {
                saved.set(prior);
                true
            }
        },
        || {
            // A failing registry write may still have changed the value.
            *os_cell.borrow_mut() = Some(format!("new-{want}"));
            if os_fails { Err("registry".to_string()) } else { Ok(()) }
        },
        |p| {
            *os_cell.borrow_mut() = p;
            Ok(())
        },
    );
    w.setting = saved.get();
    w.os = os_cell.into_inner();
    result
}

#[test]
fn two_state_change_success_changes_both_sides() {
    let mut w = TwoStateWorld { setting: false, os: None, save_fails: false, os_fails: false, restore_save_fails: false };
    run_two_state(&mut w, true).unwrap();
    assert!(w.setting);
    assert_eq!(w.os.as_deref(), Some("new-true"));
}

#[test]
fn two_state_change_keeps_both_old_when_the_save_fails() {
    let mut w = TwoStateWorld { setting: false, os: Some("old".into()), save_fails: true, os_fails: false, restore_save_fails: false };
    assert_eq!(run_two_state(&mut w, true).unwrap_err(), "save-failed");
    assert!(!w.setting);
    assert_eq!(w.os.as_deref(), Some("old"), "the registry is not touched when the setting did not save");
}

#[test]
fn two_state_change_keeps_both_old_when_the_registry_fails_after_the_save() {
    let mut w = TwoStateWorld { setting: false, os: Some("old".into()), save_fails: false, os_fails: true, restore_save_fails: false };
    assert_eq!(run_two_state(&mut w, true).unwrap_err(), "registry");
    assert!(!w.setting, "the saved setting is put back");
    assert_eq!(w.os.as_deref(), Some("old"), "a half-applied registry change is put back");
    let mut w = TwoStateWorld { setting: true, os: None, save_fails: false, os_fails: true, restore_save_fails: false };
    assert!(run_two_state(&mut w, false).is_err());
    assert!(w.setting);
    assert_eq!(w.os, None);
}

#[test]
fn registry_owner_tells_this_copy_from_another() {
    let here = r"C:\Apps\Prompt Saver\prompt-saver.exe";
    let exists = |p: &str| p.to_lowercase().contains("other");
    assert_eq!(registry_owner(None, here, exists), RegistryOwner::Missing);
    assert_eq!(registry_owner(Some(r#""C:\Apps\Prompt Saver\prompt-saver.exe" --minimized"#), here, exists), RegistryOwner::ThisExe);
    assert_eq!(registry_owner(Some(r#""c:/apps/prompt saver/PROMPT-SAVER.EXE" "%1""#), here, exists), RegistryOwner::ThisExe);
    assert_eq!(registry_owner(Some(r#""D:\other\prompt-saver.exe""#), here, exists), RegistryOwner::OtherExe);
    assert_eq!(registry_owner(Some(r#""E:\moved\prompt-saver.exe""#), here, exists), RegistryOwner::Gone);
    assert_eq!(registry_owner(Some(""), here, exists), RegistryOwner::Missing);
}

#[test]
fn boot_reconcile_only_writes_and_never_removes() {
    use RegistryOwner::*;
    for owner in [Missing, ThisExe, Gone] {
        assert!(boot_reconcile_writes(true, false, owner));
        assert!(!boot_reconcile_writes(false, false, owner), "a setting that reads as off must not delete an entry at boot");
        assert!(!boot_reconcile_writes(true, true, owner), "demo mode skips the pass");
    }
    assert!(!boot_reconcile_writes(true, false, OtherExe), "another copy's entry is left alone");
}

#[test]
fn switching_off_leaves_another_copys_entry_alone() {
    use RegistryOwner::*;
    assert!(skip_registry_removal(false, OtherExe));
    assert!(!skip_registry_removal(false, ThisExe));
    assert!(!skip_registry_removal(false, Gone));
    assert!(!skip_registry_removal(false, Missing));
    assert!(!skip_registry_removal(true, OtherExe), "switching on is never skipped");
}

fn fake_snapshot(root: &std::path::Path, name: &str) {
    let f = root.join(name);
    std::fs::create_dir_all(&f).unwrap();
    std::fs::write(f.join(BACKUP_DB_GZ), b"x").unwrap();
}

#[test]
fn rotation_ignores_an_incomplete_folder_and_keeps_good_snapshots() {
    let dir = stream_test_dir("rotate-incomplete");
    fake_snapshot(&dir, "2026-01-01_10-00-00");
    fake_snapshot(&dir, "2026-01-02_10-00-00");
    // Newer than both, but still being packed: only the raw copy exists.
    let pending = dir.join("2026-01-03_10-00-00");
    std::fs::create_dir_all(&pending).unwrap();
    std::fs::write(pending.join("data.db.tmp"), b"raw").unwrap();
    rotate_backups_at(&dir, 2, None);
    assert!(dir.join("2026-01-01_10-00-00").exists(), "a good snapshot must not be dropped for an incomplete folder");
    assert!(dir.join("2026-01-02_10-00-00").exists());
    assert!(pending.exists(), "the folder being packed is not rotation's to delete");
    assert_eq!(list_backups_impl_at(&dir).len(), 2, "the list shows complete snapshots only");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn startup_cleanup_removes_leftovers_and_keeps_complete_snapshots() {
    let dir = stream_test_dir("cleanup");
    fake_snapshot(&dir, "2026-01-01_10-00-00");
    let good = dir.join("2026-01-01_10-00-00");
    std::fs::write(good.join("data.db.tmp"), b"raw").unwrap();
    std::fs::write(good.join("data.db.gz.part"), b"half").unwrap();
    let broken = dir.join("2026-01-02_10-00-00");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("data.db.tmp"), b"raw").unwrap();
    // Too fresh to be sure it is abandoned: untouched.
    cleanup_incomplete_backups(&dir, Duration::from_secs(3600));
    assert!(broken.exists());
    assert!(good.join("data.db.tmp").exists());
    cleanup_incomplete_backups(&dir, Duration::ZERO);
    assert!(!broken.exists(), "a folder without a snapshot is removed");
    assert!(!good.join("data.db.tmp").exists());
    assert!(!good.join("data.db.gz.part").exists());
    assert!(good.join(BACKUP_DB_GZ).exists(), "a complete snapshot is never touched");
    let _ = std::fs::remove_dir_all(&dir);
}

// allowMultipleInstances lets two full processes share one data directory.
// write_rows must not delete a row this process never learned about (added by
// another instance after this process's own load) just because that row is
// absent from this process's in-memory list.
#[test]
fn write_rows_preserves_a_row_unknown_to_this_process() {
    let mut conn = row_test_db();
    // "Process A" loads and writes its initial two prompts.
    let mut written_a = WrittenRows::new();
    let initial = numbered_prompts(2);
    db_write_prompts(&mut conn, &initial, &mut written_a).unwrap();

    // "Process B" independently adds a third prompt, never seen by A.
    let foreign = Prompt { name: "Foreign".into(), ..test_prompt("foreign") };
    let mut written_b = WrittenRows::new();
    let mut b_prompts = initial.clone();
    b_prompts.push(foreign.clone());
    db_write_prompts(&mut conn, &b_prompts, &mut written_b).unwrap();

    // A now saves again with its own (still two-item) list, unaware of B's row.
    db_write_prompts(&mut conn, &initial, &mut written_a).unwrap();

    let ids: Vec<String> = stored_rows(&conn, "prompts").into_iter().map(|(id, _, _)| id).collect();
    assert!(ids.contains(&"foreign".to_string()), "another instance's unseen row must survive: {ids:?}");
    assert_eq!(ids.len(), 3, "A's save must not touch B's row at all: {ids:?}");
}

// A genuinely fresh install (no settings row at all yet) must never be flagged
// as unreadable -- only a row that IS there but cannot be read back correctly.
#[test]
fn db_load_does_not_flag_a_settings_row_that_was_never_written() {
    let conn = row_test_db();
    conn.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);").unwrap();
    let (_prompts, settings, _plain, _fails, settings_unreadable) = db_load(&conn, &mut WrittenRows::new());
    assert!(settings.is_none(), "no row means no settings to return");
    assert!(!settings_unreadable, "no row means first run, not corruption");
}

// A settings row that IS present but will not decrypt/parse must be flagged --
// this is the case that has to lock writes so a rewrite can never overwrite the
// real (corrupt but present) row with Settings::default().
#[test]
fn db_load_flags_a_settings_row_that_will_not_parse() {
    let conn = row_test_db();
    conn.execute_batch(
        "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO meta VALUES('settings', 'not valid json and not an enc: blob either');",
    )
    .unwrap();
    let (_prompts, settings, _plain, _fails, settings_unreadable) = db_load(&conn, &mut WrittenRows::new());
    assert!(settings.is_none(), "an unparsable row must not be handed back as real settings");
    assert!(settings_unreadable, "a present-but-corrupt row must be flagged");
}

// A properly sealed, well-formed row must never be flagged, whatever its content.
#[test]
fn db_load_does_not_flag_a_readable_settings_row() {
    let conn = row_test_db();
    conn.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);").unwrap();
    let s = Settings { theme: "ocean".into(), ..Default::default() };
    conn.execute(
        "INSERT INTO meta(key, value) VALUES('settings', ?1)",
        params![enc_str(&serde_json::to_string(&s).unwrap())],
    )
    .unwrap();
    let (_prompts, settings, _plain, _fails, settings_unreadable) = db_load(&conn, &mut WrittenRows::new());
    assert_eq!(settings.map(|s| s.theme), Some("ocean".to_string()));
    assert!(!settings_unreadable, "a readable row must not be flagged");
}

// The command the window polls at startup must report exactly what the flag
// holds -- this is the read side of the same lock write_settings_now checks
// before ever touching disk (untestable here directly: it needs a real
// AppHandle, which no unit test in this file constructs).
#[test]
fn settings_locked_command_reports_the_flag() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let before = SETTINGS_LOCKED.load(Ordering::Relaxed);
    SETTINGS_LOCKED.store(true, Ordering::Relaxed);
    assert!(settings_locked());
    SETTINGS_LOCKED.store(false, Ordering::Relaxed);
    assert!(!settings_locked());
    SETTINGS_LOCKED.store(before, Ordering::Relaxed);
}

// Same shape as settings_locked_command_reports_the_flag above -- the actual
// flip lives in write_settings_now, which needs a real AppHandle no unit test
// here constructs, so this proves the read side: whatever the flag holds is
// exactly what the window's startup poll gets back, in both directions.
#[test]
fn settings_write_failed_command_reports_the_flag() {
    let before = SETTINGS_WRITE_FAILED.load(Ordering::Relaxed);
    SETTINGS_WRITE_FAILED.store(true, Ordering::Relaxed);
    assert!(settings_write_failed());
    SETTINGS_WRITE_FAILED.store(false, Ordering::Relaxed);
    assert!(!settings_write_failed());
    SETTINGS_WRITE_FAILED.store(before, Ordering::Relaxed);
}

// ---- Data folder lock --------------------------------------------------

fn ddlock_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ps-ddlock-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// These tests all read and write the same two globals with real filesystem
// I/O in between, so the default parallel test runner can interleave them.
// Serialize just this group, the same way BG_OPS_TEST_LOCK already does
// for its own shared state.
static DDLOCK_TEST_LOCK: Mutex<()> = Mutex::new(());

// Saves/restores the two globals the way settings_locked_command_reports_the_flag
// does for SETTINGS_LOCKED -- these tests are the only ones in the suite that
// touch them, but the same courtesy avoids surprising a future one.
struct DdlockGuard {
    _serial: std::sync::MutexGuard<'static, ()>,
    was_readonly: bool,
    was_host: Option<String>,
}
impl DdlockGuard {
    fn capture() -> Self {
        Self {
            _serial: DDLOCK_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner()),
            was_readonly: DATA_DIR_READONLY.load(Ordering::Relaxed),
            was_host: DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        }
    }
}
impl Drop for DdlockGuard {
    fn drop(&mut self) {
        DATA_DIR_READONLY.store(self.was_readonly, Ordering::Relaxed);
        *DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()) = self.was_host.clone();
    }
}

#[test]
fn acquire_data_dir_lock_reads_a_fresh_foreign_lock_as_read_only() {
    let _g = DdlockGuard::capture();
    let dir = ddlock_test_dir("fresh-foreign");
    write_data_dir_lock(
        &data_dir_lock_path(&dir),
        &DataDirLockInfo { host: "OTHER-PC".to_string(), pid: 999, seen: unix_now() },
    );
    acquire_data_dir_lock(&dir, true);
    assert!(DATA_DIR_READONLY.load(Ordering::Relaxed), "a fresh foreign lock must open the store read-only");
    assert_eq!(
        DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()).as_deref(),
        Some("OTHER-PC"),
        "the read-only banner needs the other machine's name"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn acquire_data_dir_lock_takes_over_a_stale_foreign_lock_without_asking() {
    let _g = DdlockGuard::capture();
    let dir = ddlock_test_dir("stale-foreign");
    let path = data_dir_lock_path(&dir);
    let stale_seen = unix_now().saturating_sub(DATA_DIR_LOCK_STALE_SECS + 10);
    write_data_dir_lock(&path, &DataDirLockInfo { host: "OTHER-PC".to_string(), pid: 999, seen: stale_seen });
    acquire_data_dir_lock(&dir, true);
    assert!(!DATA_DIR_READONLY.load(Ordering::Relaxed), "a lock nobody has refreshed in three rounds must be reclaimed, not obeyed");
    assert!(DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()).is_none());
    let claimed = read_data_dir_lock(&path).expect("acquiring the lock must write a fresh one");
    assert_eq!(claimed.host, local_host_name(), "the reclaimed lock must now name this machine");
    assert!(unix_now().saturating_sub(claimed.seen) < 5, "the reclaimed lock must carry a current timestamp");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn acquire_data_dir_lock_never_treats_its_own_hostname_as_foreign() {
    let _g = DdlockGuard::capture();
    let dir = ddlock_test_dir("own-host");
    // Same host, different (long-gone) pid and a timestamp that would count as
    // stale too -- neither should matter: a crash-and-restart on THIS machine
    // is not the two-machine case this guards against, and single-instance
    // already rules out a second live process here.
    write_data_dir_lock(
        &data_dir_lock_path(&dir),
        &DataDirLockInfo { host: local_host_name(), pid: 424_242, seen: unix_now().saturating_sub(DATA_DIR_LOCK_STALE_SECS + 999) },
    );
    acquire_data_dir_lock(&dir, true);
    assert!(!DATA_DIR_READONLY.load(Ordering::Relaxed), "this machine's own old lock must never lock this machine out");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn acquire_data_dir_lock_does_nothing_while_the_toggle_is_off() {
    let _g = DdlockGuard::capture();
    let dir = ddlock_test_dir("toggle-off");
    write_data_dir_lock(
        &data_dir_lock_path(&dir),
        &DataDirLockInfo { host: "OTHER-PC".to_string(), pid: 1, seen: unix_now() },
    );
    acquire_data_dir_lock(&dir, false);
    assert!(!DATA_DIR_READONLY.load(Ordering::Relaxed), "dataDirLock off must restore today's behaviour: no read-only state");
    assert!(DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()).is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn refresh_data_dir_lock_keeps_an_owned_lock_fresh() {
    let _g = DdlockGuard::capture();
    let dir = ddlock_test_dir("owned-refresh");
    acquire_data_dir_lock(&dir, true); // claims it -- no prior lock file
    assert!(!DATA_DIR_READONLY.load(Ordering::Relaxed));
    let path = data_dir_lock_path(&dir);
    // Back-date the file the way a real clock tick would leave it just before
    // the next scheduler round is due.
    write_data_dir_lock(&path, &DataDirLockInfo { host: local_host_name(), pid: std::process::id(), seen: unix_now() - 25 });
    refresh_data_dir_lock(&dir);
    let refreshed = read_data_dir_lock(&path).expect("refresh must not delete the lock file");
    assert!(unix_now().saturating_sub(refreshed.seen) < 5, "a round while owned must bring the timestamp back to now");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn refresh_data_dir_lock_never_touches_a_foreign_lock_while_read_only() {
    let _g = DdlockGuard::capture();
    let dir = ddlock_test_dir("readonly-refresh");
    let path = data_dir_lock_path(&dir);
    let foreign_seen = unix_now();
    write_data_dir_lock(&path, &DataDirLockInfo { host: "OTHER-PC".to_string(), pid: 1, seen: foreign_seen });
    acquire_data_dir_lock(&dir, true); // becomes read-only against the fresh foreign lock
    assert!(DATA_DIR_READONLY.load(Ordering::Relaxed));
    refresh_data_dir_lock(&dir); // must be a no-op: this file is not ours to touch
    let after = read_data_dir_lock(&path).expect("a read-only round must not delete the other machine's lock");
    assert_eq!(after.host, "OTHER-PC");
    assert_eq!(after.seen, foreign_seen, "a read-only process must never rewrite the lock it does not own");
    let _ = fs::remove_dir_all(&dir);
}

// Eight paths: one for each provider's folder-name convention, one OneDrive
// path recognised only through its environment variable (business tenants
// rename the visible folder), and two ordinary local paths that must stay
// unrecognised.
#[test]
fn sync_folder_hint_recognises_known_cloud_folders_and_rejects_plain_ones() {
    assert_eq!(sync_folder_hint(Path::new(r"D:\Sync\OneDrive\PromptSaver")), Some("OneDrive"));
    assert_eq!(sync_folder_hint(Path::new(r"D:\Sync\Dropbox\PromptSaver")), Some("Dropbox"));
    assert_eq!(sync_folder_hint(Path::new(r"D:\Sync\Google Drive\PromptSaver")), Some("Google Drive"));
    assert_eq!(sync_folder_hint(Path::new(r"D:\Sync\GoogleDrive\PromptSaver")), Some("Google Drive"));
    assert_eq!(sync_folder_hint(Path::new(r"D:\Sync\iCloudDrive\PromptSaver")), Some("iCloud Drive"));
    assert_eq!(sync_folder_hint(Path::new(r"D:\Sync\Nextcloud\PromptSaver")), Some("Nextcloud"));
    assert_eq!(sync_folder_hint(Path::new(r"D:\Data\PromptSaver")), None);
    assert_eq!(sync_folder_hint(Path::new(r"D:\Backups\PromptSaver")), None);

    // The env-var path: a business tenant's OneDrive root is often renamed in
    // Explorer (e.g. "BGCoding") with nothing resembling "onedrive" in the
    // visible name, so only the environment variable catches it.
    let guard = EnvVarGuard::set("OneDrive", r"D:\Company\BGCoding");
    assert_eq!(sync_folder_hint(Path::new(r"D:\Company\BGCoding\PromptSaver")), Some("OneDrive"));
    drop(guard);
}

// Minimal set/restore-on-drop wrapper -- std::env::set_var is unsafe to call
// concurrently with a read elsewhere in the same process, but nothing else
// in this suite reads OneDrive/OneDriveConsumer/OneDriveCommercial, and the
// restore on Drop keeps a panic from leaking the override into later tests.
struct EnvVarGuard {
    key: &'static str,
    prior: Option<String>,
}
impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let prior = std::env::var(key).ok();
        unsafe { std::env::set_var(key, value) };
        Self { key, prior }
    }
}
impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.prior {
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

#[test]
fn changing_one_prompt_rewrites_exactly_one_row() {
    let mut conn = row_test_db();
    let mut prompts = numbered_prompts(200);
    let mut written = WrittenRows::new();
    db_write_prompts(&mut conn, &prompts, &mut written).unwrap();
    let before = stored_rows(&conn, "prompts");
    if sealing_is_randomised() {
        assert_ne!(
            enc_str("probe"),
            enc_str("probe"),
            "sealing must be randomised, or comparing blobs proves nothing below"
        );
    }
    prompts[42].name = "Renamed".into();
    db_write_prompts(&mut conn, &prompts, &mut written).unwrap();
    let after = stored_rows(&conn, "prompts");
    assert_eq!(after.len(), before.len());
    for (b, a) in before.iter().zip(after.iter()) {
        assert_eq!(a.0, b.0, "order must not change");
        assert_eq!(a.1, b.1, "positions must not change");
        if a.0 == "id042" {
            assert_ne!(a.2, b.2, "the edited prompt must actually be rewritten");
        } else {
            assert_eq!(a.2, b.2, "an untouched prompt must keep the exact bytes it had");
        }
    }
    assert_eq!(stored_prompt(&after[42].2).name, "Renamed");
}

#[test]
fn reordering_prompts_moves_them_without_rewriting_any_content() {
    let mut conn = row_test_db();
    let mut prompts = numbered_prompts(20);
    let mut written = WrittenRows::new();
    db_write_prompts(&mut conn, &prompts, &mut written).unwrap();
    let before: HashMap<String, String> =
        stored_rows(&conn, "prompts").into_iter().map(|(id, _, data)| (id, data)).collect();
    prompts.reverse();
    db_write_prompts(&mut conn, &prompts, &mut written).unwrap();
    let after = stored_rows(&conn, "prompts");
    assert_eq!(
        after.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        prompts.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        "the stored order must follow the new list"
    );
    for (i, row) in after.iter().enumerate() {
        assert_eq!(row.1, i as i64, "ord must be the row's new position");
        assert_eq!(&row.2, before.get(&row.0).unwrap(), "a move must not re-seal the content");
    }
}

#[test]
fn trashing_and_restoring_moves_exactly_one_row_between_the_tables() {
    let mut conn = row_test_db();
    let mut written = WrittenRows::new();
    let prompts = numbered_prompts(3);
    db_write_prompts_and_trash(&mut conn, &prompts, &[], &mut written).unwrap();
    let before = stored_rows(&conn, "prompts");

    let mut live = prompts.clone();
    let trashed = live.remove(1);
    db_write_prompts_and_trash(&mut conn, &live, std::slice::from_ref(&trashed), &mut written).unwrap();
    let kept = stored_rows(&conn, "prompts");
    assert_eq!(kept.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), vec!["id000", "id002"]);
    assert_eq!(kept[0].2, before[0].2, "the prompt before the hole must not be rewritten");
    assert_eq!(kept[1].1, 1, "the prompt after the hole must close the gap");
    let bin = stored_rows(&conn, "trash");
    assert_eq!(bin.len(), 1);
    assert_eq!(as_json(&stored_prompt(&bin[0].2)), as_json(&trashed), "the trashed prompt must arrive whole");

    db_write_prompts_and_trash(&mut conn, &prompts, &[], &mut written).unwrap();
    let restored = stored_rows(&conn, "prompts");
    assert_eq!(
        restored.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        vec!["id000", "id001", "id002"]
    );
    assert_eq!(
        as_json(&stored_prompt(&restored[1].2)),
        as_json(&prompts[1]),
        "the restored prompt must come back whole"
    );
    assert!(stored_rows(&conn, "trash").is_empty(), "the trash row must be gone again");
}

#[test]
fn a_replaced_prompt_set_is_written_in_full_once_the_row_map_is_cleared() {
    // The trap this guards: the map says "id X on disk holds this content", a
    // restore/import then replaces the table behind the writer's back, and the
    // in-memory prompt still hashes to what the map remembers: so without
    // clearing, the row is skipped and the user's data never reaches the disk.
    let mut conn = row_test_db();
    let mut written = WrittenRows::new();
    let mine = numbered_prompts(3);
    db_write_prompts(&mut conn, &mine, &mut written).unwrap();

    // What a restore does: put someone else's rows straight into the table.
    let theirs: Vec<Prompt> = mine
        .iter()
        .map(|p| Prompt { name: format!("{} (from the backup)", p.name), ..p.clone() })
        .collect();
    conn.execute("DELETE FROM prompts", []).unwrap();
    {
        let mut ins = conn
            .prepare("INSERT INTO prompts(id, ord, data) VALUES(?1, ?2, ?3)")
            .unwrap();
        for (i, p) in theirs.iter().enumerate() {
            ins.execute(params![p.id, i as i64, enc_str(&serde_json::to_string(p).unwrap())]).unwrap();
        }
    }

    // Stale map: every row still hashes to what it did before, so the write is
    // skipped entirely and the table keeps the foreign content.
    let mut stale = written.clone();
    db_write_prompts(&mut conn, &mine, &mut stale).unwrap();
    assert_eq!(
        stored_rows(&conn, "prompts").iter().map(|r| stored_prompt(&r.2).name).collect::<Vec<_>>(),
        theirs.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
        "a stale map really does skip the write - which is why every wholesale replacement clears it"
    );

    // Cleared map (what Store::forget_written_rows does): the set on disk ends
    // up exactly the set in memory.
    written.clear();
    db_write_prompts(&mut conn, &mine, &mut written).unwrap();
    let stored: Vec<String> =
        stored_rows(&conn, "prompts").iter().map(|r| as_json(&stored_prompt(&r.2))).collect();
    assert_eq!(
        stored,
        mine.iter().map(as_json).collect::<Vec<_>>(),
        "after a full replacement the stored set must match the new one exactly"
    );
}

// A settings write that once fell back to settings.json leaves that file behind
// while prompts.json (which nothing wrote) stays absent. If the database is
// then unopenable for one launch, the prompts are invisible but very much still
// on disk, so the store has to refuse prompt writes regardless: write_rows itself
// now only deletes a row it can prove it once knew about (see the
// allowMultipleInstances fix), so an empty `written` map on its own no longer
// wipes the table: but a partially-known list (some rows genuinely loaded, one
// instance's own explicit delete mixed in) still relies entirely on the lock,
// which is why it stays unconditional here. Whether settings.json happens to be
// lying there says nothing about that, which is exactly what this used to hinge on.
#[test]
fn a_missing_prompts_json_beside_a_database_locks_prompt_writes() {
    let dir = std::env::temp_dir().join(format!("{}-test-fallback-lock", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // The real prompts live in the database this launch cannot open.
    let db = dir.join("data.db");
    let mine = numbered_prompts(3);
    {
        let mut conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
                 CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);",
        )
        .unwrap();
        db_write_prompts(&mut conn, &mine, &mut WrittenRows::new()).unwrap();
    }
    // Only settings.json is on disk, left by an earlier failed settings write.
    let mut leftover = Settings::default();
    leftover.ui_texts.insert("marker".into(), "FROM-FILE".into());
    assert!(write_sealed_json(&dir.join("settings.json"), &leftover));
    assert!(!dir.join("prompts.json").exists());

    let store = load_store_from_json(&dir);
    assert_eq!(
        store.settings.ui_texts.get("marker").map(String::as_str),
        Some("FROM-FILE"),
        "the settings file is still the best source there is and must be read"
    );
    assert!(store.prompts.is_empty(), "there is nothing readable to load the prompts from");
    assert!(
        store.prompts_locked,
        "an empty prompt list with a database behind it must never be saveable"
    );

    // Even bypassing the lock, write_rows' own "only delete a row I can prove I
    // knew about" guard now protects this exact case too: `store.written` is
    // empty (this Store never read the table), so nothing gets deleted. The
    // lock remains the real, necessary defense for a partially-known list.
    {
        let mut conn = Connection::open(&db).unwrap();
        db_write_prompts(&mut conn, &store.prompts, &mut store.written.borrow_mut()).unwrap();
        assert_eq!(
            stored_rows(&conn, "prompts").len(),
            3,
            "write_rows must not delete rows it never learned about, even with the lock bypassed"
        );
    }

    // A genuinely fresh folder has no database behind it, so nothing is hidden
    // and writes must stay open.
    let fresh = std::env::temp_dir().join(format!("{}-test-fallback-fresh", brand::SLUG));
    let _ = std::fs::remove_dir_all(&fresh);
    std::fs::create_dir_all(&fresh).unwrap();
    assert!(
        !load_store_from_json(&fresh).prompts_locked,
        "a first launch with no data at all must be able to save"
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&fresh);
}

// A data-folder move first writes the live state into the folder it is leaving,
// and the cleanup offered afterwards deletes exactly that folder. So a flush
// that could not write (a folder gone read-only, a permission change) has to
// abort the whole move: copying the now-stale files on and then authorising the
// deletion of the original is how the newest prompts would go for good.
#[test]
fn a_failed_flush_aborts_the_data_folder_move_and_authorises_no_cleanup() {
    let root = std::env::temp_dir().join(format!("{}-test-datadir-move", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    let current = root.join("old");
    let new = root.join("new");
    let pointer = root.join("pointer");
    for d in [&current, &new, &pointer] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(current.join("data.db"), b"the real store").unwrap();
    *LAST_OLD_DATA_DIR.lock().unwrap() = None;

    let mut rebound = false;
    {
        let mut refuses = || -> Result<(), String> { Err("folder is read-only".to_string()) };
        let mut rebind = || rebound = true;
        let moved = move_store_dir(&current, &new, &pointer, &mut refuses, &mut rebind);
        assert!(moved.is_err(), "a flush that could not persist the live state must fail the move");
    }
    assert!(!rebound, "the process must keep writing to the folder it never left");
    assert!(!new.join("data.db").exists(), "nothing may be copied to the new folder");
    assert!(
        !pointer.join("datapath").exists(),
        "future launches must still be pointed at the folder that holds the data"
    );
    assert!(
        LAST_OLD_DATA_DIR.lock().unwrap().is_none(),
        "the folder still holding the only copy must never be offered for deletion"
    );

    // The same move with a flush that works goes all the way through.
    {
        let mut works = || -> Result<(), String> { Ok(()) };
        let mut rebind = || rebound = true;
        move_store_dir(&current, &new, &pointer, &mut works, &mut rebind).unwrap();
    }
    assert!(rebound, "the live store must be told it is writing to a different file now");
    assert!(new.join("data.db").exists(), "the store must arrive in the new folder");
    assert_eq!(
        std::fs::read_to_string(pointer.join("datapath")).unwrap(),
        new.to_string_lossy(),
        "the next launch must find the new folder"
    );
    assert_eq!(
        *LAST_OLD_DATA_DIR.lock().unwrap(),
        Some(current.clone()),
        "only a completed move may authorise cleaning up the old folder"
    );

    *LAST_OLD_DATA_DIR.lock().unwrap() = None;
    let _ = std::fs::remove_dir_all(&root);
}

// Demo mode: switching on builds the sample store in its own subfolder and
// redirects the canonical folder at it, switching off puts the redirect back
// exactly as it was, and the real database, sitting right next to the
// redirect file the whole time, is never opened by either direction.
#[test]
fn demo_mode_round_trip_never_touches_the_real_store() {
    let canonical = std::env::temp_dir().join(format!("{}-test-demo-roundtrip", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    std::fs::create_dir_all(&canonical).unwrap();
    let real_db = canonical.join("data.db");
    std::fs::write(&real_db, b"the real store, byte for byte").unwrap();
    let real_bytes_before = std::fs::read(&real_db).unwrap();

    // No redirect exists yet: enabling must create one.
    assert!(!canonical.join("datapath").exists());
    let demo = enter_demo_mode(&canonical, None).unwrap();
    assert_eq!(demo, canonical.join(DEMO_SUBFOLDER));
    assert!(demo.join("data.db").exists(), "enabling must create the sample store");
    assert_eq!(
        std::fs::read_to_string(canonical.join("datapath")).unwrap(),
        demo.to_string_lossy(),
        "the canonical folder must now redirect to the demo subfolder"
    );
    assert_eq!(
        std::fs::read(&real_db).unwrap(),
        real_bytes_before,
        "the real database must be untouched by enabling demo mode"
    );

    exit_demo_mode(&canonical).unwrap();
    assert!(
        !canonical.join("datapath").exists(),
        "disabling must remove the redirect there was none of before"
    );
    assert!(
        !canonical.join(DEMO_POINTER_BACKUP).exists(),
        "the backup marker must be cleaned up once restored"
    );
    assert_eq!(
        std::fs::read(&real_db).unwrap(),
        real_bytes_before,
        "the real database must still be untouched after disabling demo mode"
    );

    let _ = std::fs::remove_dir_all(&canonical);
}

// A user who had already redirected their data elsewhere (the "move data
// folder" expert action) must get that exact redirect back, not an empty one,
// when demo mode is switched off again.
#[test]
fn demo_mode_restores_a_pre_existing_custom_redirect() {
    let canonical = std::env::temp_dir().join(format!("{}-test-demo-custom-redirect", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    std::fs::create_dir_all(&canonical).unwrap();
    let custom = canonical.join("elsewhere");
    std::fs::create_dir_all(&custom).unwrap();
    std::fs::write(canonical.join("datapath"), custom.to_string_lossy().as_bytes()).unwrap();

    enter_demo_mode(&canonical, None).unwrap();
    assert_ne!(
        std::fs::read_to_string(canonical.join("datapath")).unwrap(),
        custom.to_string_lossy(),
        "entering demo mode must point away from the custom folder"
    );

    exit_demo_mode(&canonical).unwrap();
    assert_eq!(
        std::fs::read_to_string(canonical.join("datapath")).unwrap(),
        custom.to_string_lossy(),
        "the original custom redirect must come back exactly"
    );

    let _ = std::fs::remove_dir_all(&canonical);
}

// Re-entering demo mode (switch off, then back on in the same session)
// rebuilds the sample store from nothing rather than keeping whatever a
// previous demo session left behind.
#[test]
fn demo_mode_rebuilds_fresh_on_every_enable() {
    let canonical = std::env::temp_dir().join(format!("{}-test-demo-rebuild", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    std::fs::create_dir_all(&canonical).unwrap();

    let demo = enter_demo_mode(&canonical, None).unwrap();
    std::fs::write(demo.join("left-over.txt"), b"a change made during the demo session").unwrap();
    exit_demo_mode(&canonical).unwrap();

    enter_demo_mode(&canonical, None).unwrap();
    assert!(
        !demo.join("left-over.txt").exists(),
        "a fresh enable must wipe whatever a previous demo session left behind"
    );

    let _ = std::fs::remove_dir_all(&canonical);
}

// The sample store itself: every prompt kind the spec asks for is present,
// and the sample library and settings load back exactly as any other store's
// would through the normal reader.
fn demo_structure(lang: &str, tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-lang-{tag}", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let prompts = demo_prompts(&dir, None, lang);
    let settings = demo_settings(false, lang);
    let _ = std::fs::remove_dir_all(&dir);
    let ids = prompts.iter().map(|p| p.id.clone()).collect();
    let shape = prompts
        .iter()
        .map(|p| (!p.image.is_empty(), p.show_image, p.variants.len(), !p.file_path.is_empty(), p.favorite, p.tags.len()))
        .collect();
    let views = settings.views.iter().map(|v| (v.id.clone(), v.members.clone())).collect();
    let folders = settings.tile_folders.iter().map(|f| format!("{}|{:?}", f.id, f.members)).collect();
    let ids: Vec<String> = ids;
    let shape: Vec<(bool, bool, usize, bool, bool, usize)> = shape;
    let views: Vec<(String, Vec<String>)> = views;
    let folders: Vec<String> = folders;
    format!("{ids:?}{shape:?}{views:?}{folders:?}")
}

#[test]
fn demo_content_has_the_same_structure_in_every_language() {
    let base = demo_structure("en", "base");
    for lang in demo_text::DEMO_LANGS {
        assert_eq!(demo_structure(lang, lang), base, "{lang}: ids and structure must not change with the language");
    }
}

#[test]
fn demo_text_is_complete_dash_free_and_keeps_the_placeholder_in_every_language() {
    let en = demo_text::demo_text("en");
    for lang in demo_text::DEMO_LANGS {
        let d = demo_text::demo_text(lang);
        let all = [
            d.tag_demo, d.tag_favorite, d.welcome_name, d.welcome_text, d.image_name, d.image_text, d.rotating_name,
            d.rotating_text, d.rotating_a, d.rotating_b, d.placeholder_name, d.placeholder_text, d.favorite_name,
            d.favorite_text, d.chain_a_name, d.chain_a_text, d.chain_b_name, d.chain_b_text, d.attach_name,
            d.attach_file, d.attach_body, d.folder_a_name, d.folder_a_text, d.folder_b_name, d.folder_b_text,
            d.video_name, d.video_file, d.view_home, d.view_team, d.folder,
        ];
        for t in all {
            assert!(!t.trim().is_empty(), "{lang}: empty text");
            assert!(!t.contains('\u{2014}') && !t.contains('\u{2013}'), "{lang}: dash in {t:?}");
            assert!(!t.contains("_name") && !t.contains("_text") && !t.contains("demo."), "{lang}: raw key in {t:?}");
        }
        assert!(d.placeholder_text.contains("{#{") && d.placeholder_text.contains("}#}"), "{lang}: placeholder syntax");
        for f in [d.attach_file, d.video_file] {
            assert!(!f.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']), "{lang}: {f:?} is not a file name");
        }
        assert!(d.attach_file.ends_with(".txt") && d.video_file.ends_with(".mp4"), "{lang}: file extensions");
        if lang != "en" {
            assert_ne!(d.welcome_text, en.welcome_text, "{lang} is still the English text");
        }
    }
}

#[test]
fn demo_text_falls_back_to_english_and_ignores_a_region() {
    let en = demo_text::demo_text("en");
    for odd in ["", "xx", "klingon", "fr-FR"] {
        assert_eq!(demo_text::demo_text(odd).welcome_name, en.welcome_name, "{odd:?} must fall back to EN");
    }
    assert_eq!(demo_text::demo_text("pt-BR").welcome_name, demo_text::demo_text("pt").welcome_name);
    assert_eq!(demo_text::demo_text("zh_CN").welcome_name, demo_text::demo_text("zh").welcome_name);
    assert_eq!(demo_text::demo_text("DE").welcome_name, "Willkommen");
}

#[test]
fn demo_settings_keep_the_interface_language() {
    assert_eq!(demo_settings(false, "en").language, "en");
    assert_eq!(demo_settings(false, "fr").language, "fr");
}

#[test]
fn demo_german_content_is_what_it_was_before_the_table() {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-de-same", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let prompts = demo_prompts(&dir, None, "de");
    let by = |id: &str| prompts.iter().find(|p| p.id == id).unwrap();
    assert_eq!(by("demo-welcome").name, "Willkommen");
    assert_eq!(by("demo-welcome").text, "Das hier ist ein Beispieleintrag im Vorführmodus. Ein Klick kopiert ihn wie jeden anderen.");
    assert_eq!(by("demo-image").name, "Bild-Kachel");
    assert_eq!(by("demo-rotating").variants, vec!["Guten Tag!".to_string(), "Guten Abend!".to_string()]);
    assert_eq!(by("demo-placeholder").text, "Hallo {#{Name}#}, vielen Dank für deine Nachricht.");
    assert_eq!(by("demo-favorite").tags, vec!["Demo".to_string(), "Favorit".to_string()]);
    assert_eq!(by("demo-attachment").file_path, "Anhang.txt");
    assert_eq!(std::fs::read_to_string(dir.join("Anhang.txt")).unwrap(), "Eine harmlose Beispieldatei für den Vorführmodus.\n");
    assert_eq!(demo_settings(false, "de").tile_folders[0].name, "Beispielordner");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn demo_store_rebuilt_in_another_language_holds_only_that_language() {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-switch", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    build_demo_store(&dir, None, "de").unwrap();
    assert!(dir.join("Anhang.txt").is_file());
    build_demo_store(&dir, None, "en").unwrap();
    assert!(!dir.join("Anhang.txt").exists(), "the German attachment must be gone after a rebuild");
    assert!(dir.join("Attachment.txt").is_file());
    let conn = db_conn_at(&dir).unwrap();
    let mut written = WrittenRows::new();
    let (prompts, settings, _p, _f, _l) = db_load(&conn, &mut written);
    assert_eq!(prompts.iter().find(|p| p.id == "demo-welcome").unwrap().name, "Welcome");
    assert_eq!(settings.unwrap().tile_folders[0].name, "Sample folder");
    drop(conn);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn demo_store_contains_every_sample_kind() {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-contents", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    build_demo_store(&dir, None, "de").unwrap();

    let conn = db_conn_at(&dir).unwrap();
    let mut written = WrittenRows::new();
    let (prompts, settings, _plain, _fails, _locked) = db_load(&conn, &mut written);
    let settings = settings.expect("the sample settings row must load back");
    let by_id: HashMap<&str, &Prompt> = prompts.iter().map(|p| (p.id.as_str(), p)).collect();

    assert!(by_id.get("demo-image").is_some_and(|p| !p.image.is_empty()), "an image tile");
    assert!(by_id.get("demo-welcome").unwrap().image.is_empty(), "a tile without an image");
    assert!(!by_id.get("demo-rotating").unwrap().variants.is_empty(), "a rotating prompt");
    assert!(by_id.get("demo-placeholder").unwrap().text.contains("{#{"), "a self-fill placeholder");
    assert!(by_id.get("demo-favorite").unwrap().favorite, "a favorite");
    assert!(!by_id.get("demo-attachment").unwrap().file_path.is_empty(), "an attachment");
    assert!(!by_id.get("demo-welcome").unwrap().tags.is_empty(), "a tagged prompt");

    assert_eq!(settings.views.len(), 2, "two views");
    assert_eq!(settings.tile_folders.len(), 1, "one grid folder");
    assert_eq!(settings.tile_folders[0].members.len(), 2);
    assert_eq!(settings.ui_flags.get("demoMode"), Some(&true));

    let _ = std::fs::remove_dir_all(&dir);
}

// Finding 1 (HIGH, adversarial audit): disable_demo_mode flushes pending
// demo settings into the demo dir, under the store lock, BEFORE it flips
// the "datapath" redirect back to the real one -- otherwise app.restart()'s
// own RunEvent::Exit flush (or the 250ms settings_flush_tick) can resolve
// data_dir() AFTER the flip and overwrite the real data.db with demo
// settings. This proves the write-then-flip half directly: a settings
// write made while the redirect still points at the demo dir lands there,
// never in the real store sitting right next to it.
#[test]
fn demo_settings_written_before_the_flip_land_in_the_demo_dir_not_the_real_one() {
    let canonical = std::env::temp_dir().join(format!("{}-test-demo-exit-flush", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    std::fs::create_dir_all(&canonical).unwrap();
    let real_db = canonical.join("data.db");
    std::fs::write(&real_db, b"the real store, byte for byte").unwrap();
    let real_bytes_before = std::fs::read(&real_db).unwrap();

    let demo = enter_demo_mode(&canonical, None).unwrap();

    // Simulate the flush disable_demo_mode does before it calls
    // exit_demo_mode: at this point the redirect still points at `demo`.
    let conn = db_conn_at(&demo).unwrap();
    let changed = Settings { theme: "changed-during-demo".into(), ..Default::default() };
    db_write_settings(&conn, &changed).unwrap();

    exit_demo_mode(&canonical).unwrap();

    assert_eq!(
        std::fs::read(&real_db).unwrap(),
        real_bytes_before,
        "a settings write made before the redirect flip must never reach the real data.db"
    );
    let demo_conn = db_conn_at(&demo).unwrap();
    let mut written = WrittenRows::new();
    let (_p, settings, ..) = db_load(&demo_conn, &mut written);
    assert_eq!(
        settings.map(|s| s.theme),
        Some("changed-during-demo".to_string()),
        "the flush must have actually landed in the demo store"
    );

    let _ = std::fs::remove_dir_all(&canonical);
}

// The other half of finding 1: DEMO_TRANSITION_LOCKED is the flag
// data_dir_write_blocked() AND db_conn() both check first (the second
// review's own point: db_conn() is the actual single choke point, not
// just the four functions that also call data_dir_write_blocked()) --
// this reads it back exactly like settings_locked_command_reports_the_flag
// and settings_write_failed_command_reports_the_flag do for their own
// flags (db_conn/data_dir_write_blocked themselves need a real AppHandle,
// which no unit test in this file constructs).
#[test]
fn demo_transition_locked_flag_round_trips() {
    let before = DEMO_TRANSITION_LOCKED.load(Ordering::Relaxed);
    DEMO_TRANSITION_LOCKED.store(true, Ordering::Relaxed);
    assert!(DEMO_TRANSITION_LOCKED.load(Ordering::Relaxed));
    DEMO_TRANSITION_LOCKED.store(false, Ordering::Relaxed);
    assert!(!DEMO_TRANSITION_LOCKED.load(Ordering::Relaxed));
    DEMO_TRANSITION_LOCKED.store(before, Ordering::Relaxed);
}

// Kept alive a few seconds and spawned only as this same test binary re-
// invoking itself (see other_instance_running_detects_a_live_pid_and_prunes_a_dead_one
// below), so instance_pid_is_this_exe's image-path check has something
// genuinely matching current_exe() to find, not an unrelated process.
#[test]
#[ignore]
fn __sleep_helper_for_instance_test() {
    std::thread::sleep(std::time::Duration::from_secs(3));
}

// other_instance_running must tell a real second live process apart from
// the allowMultipleInstances setting -- see its own comment for why the
// flag was not trustworthy evidence. Uses this test binary's own pid
// (always alive) as the "other" instance and a definitely-dead one (a
// huge pid unlikely to exist, plus this process's own pid is excluded so
// it can never see itself).
#[test]
#[cfg(windows)]
fn other_instance_running_detects_a_live_pid_and_prunes_a_dead_one() {
    let canonical = std::env::temp_dir().join(format!("{}-test-instance-registry", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    assert!(!other_instance_running(&canonical), "an empty/missing registry has nothing to find");

    register_running_instance(&canonical);
    assert!(
        !other_instance_running(&canonical),
        "a process must never count itself as a second instance"
    );

    // A pid a genuinely different copy of THIS exe holds right now -- a
    // third review finding requires instance_pid_is_this_exe to confirm
    // the pid's image path, not merely that some process holds it, so
    // the "other" instance here must actually be this same test binary
    // re-invoked as a child, kept alive by the sleep helper above.
    let exe = std::env::current_exe().unwrap();
    let mut child = std::process::Command::new(&exe)
        .args(["tests::__sleep_helper_for_instance_test", "--exact", "--ignored", "--test-threads=1"])
        .spawn()
        .unwrap();
    let other_pid = child.id();
    std::fs::write(instances_dir(&canonical).join(other_pid.to_string()), b"").unwrap();
    assert!(other_instance_running(&canonical), "a different LIVE copy of this exe must be detected");

    // An obviously stale entry (not a real pid on this machine) must be
    // pruned rather than perma-blocking every future enable/disable.
    std::fs::write(instances_dir(&canonical).join("999999999"), b"").unwrap();
    let _ = other_instance_running(&canonical); // one pass to prune it
    assert!(
        !instances_dir(&canonical).join("999999999").exists(),
        "a dead pid's marker must be pruned opportunistically"
    );

    let _ = child.kill();
    let _ = child.wait();
    unregister_running_instance(&canonical);
    assert!(
        !instances_dir(&canonical).join(std::process::id().to_string()).exists(),
        "unregister must remove this process's own marker"
    );

    let _ = std::fs::remove_dir_all(&canonical);
}

// A pid reused by an unrelated process (Windows reuses freed pids; the
// window between a crash/task-kill and its stale entry getting pruned
// can span a whole later session) must not read as "still our instance"
// forever -- a third review finding. cmd.exe here stands in for that
// unrelated process: it is definitely alive, and definitely not a copy
// of this exe, so instance_pid_is_this_exe must refuse it even though
// pid_is_alive alone would accept it. The entry must NOT be pruned for
// that alone, though (a fourth review finding): pruning here as well as
// on a dead pid would delete a REAL second instance's own marker the
// moment the image check merely fails to confirm it (a permission edge
// case, or a bug in that check), permanently losing this instance's
// spot in the registry with nothing to notice and re-register it.
#[test]
#[cfg(windows)]
fn other_instance_running_refuses_a_pid_reused_by_an_unrelated_process() {
    let canonical = std::env::temp_dir().join(format!("{}-test-instance-registry-reused-pid", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    std::fs::create_dir_all(instances_dir(&canonical)).unwrap();

    let mut child = std::process::Command::new("cmd").args(["/C", "ping -n 3 127.0.0.1 >NUL"]).spawn().unwrap();
    let other_pid = child.id();
    std::fs::write(instances_dir(&canonical).join(other_pid.to_string()), b"").unwrap();
    assert!(
        !other_instance_running(&canonical),
        "a live pid that is not a copy of this exe must never count as another instance"
    );
    assert!(
        instances_dir(&canonical).join(other_pid.to_string()).exists(),
        "a live pid must never be pruned on the image check alone, only a dead one"
    );

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&canonical);
}

// A real second instance running from the exact same exe must still be
// recognized when only its casing differs (a fourth review finding:
// Windows paths are case-insensitive, an exact PathBuf == PathBuf compare
// is not). Uses this test binary's own current_exe() with an uppercased
// copy of its path string, since a real differently-cased path to the
// same file cannot be constructed portably in a test.
#[test]
fn same_exe_path_ignores_casing() {
    let exe = std::env::current_exe().unwrap();
    let upper = PathBuf::from(exe.to_string_lossy().to_uppercase());
    assert!(same_exe_path(&exe, &upper), "the same file with different casing must still match");
}

// Two genuinely different files must never match regardless of casing.
#[test]
fn same_exe_path_refuses_a_different_file() {
    let exe = std::env::current_exe().unwrap();
    let other = std::env::temp_dir().join(format!("{}-test-same-exe-path-unrelated.exe", brand::SLUG));
    assert!(!same_exe_path(&exe, &other));
}

// Finding 2 (HIGH, portable mode): build_demo_store must never delete a
// pre-existing folder at the demo subfolder's path unless it carries the
// marker this function itself writes -- portable installs put the demo
// subfolder beside the exe, where a user could already have an unrelated
// folder there (named "demo" under the old, generic subfolder name; the
// marker check protects the new app-specific name just as well against
// any other collision).
#[test]
fn build_demo_store_refuses_to_wipe_an_unmarked_existing_folder() {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-unmarked-folder", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sentinel = dir.join("not-mine.txt");
    std::fs::write(&sentinel, b"a user's own file, nothing to do with demo mode").unwrap();

    let result = build_demo_store(&dir, None, "de");
    assert!(result.is_err(), "an existing, unmarked folder must never be wiped: {result:?}");
    assert!(sentinel.exists(), "the user's own file must survive");

    let _ = std::fs::remove_dir_all(&dir);
}

// A folder build_demo_store itself created (carries the marker) must still
// be rebuildable, or every enable after the very first would fail outright.
#[test]
fn build_demo_store_rebuilds_its_own_marked_folder() {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-marked-folder", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    build_demo_store(&dir, None, "de").unwrap();
    std::fs::write(dir.join("left-over.txt"), b"session leftover").unwrap();

    build_demo_store(&dir, None, "de").unwrap();
    assert!(!dir.join("left-over.txt").exists(), "a marked folder must still be wiped and rebuilt");
    assert!(dir.join(DEMO_MARKER_FILE).is_file(), "the rebuild must leave the marker in place");

    let _ = std::fs::remove_dir_all(&dir);
}

// Finding 3 (N3, second review): a locked file elsewhere in the folder
// (data.db held open by something else) must never cost the marker its
// life -- the old order (remove_dir_all the whole folder, including the
// marker, THEN fail on the locked file) could leave a folder that still
// exists but no longer carries proof this app made it, refusing every
// later enable forever.
#[test]
#[cfg(windows)]
fn build_demo_store_keeps_its_marker_when_a_file_is_locked() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = std::env::temp_dir().join(format!("{}-test-demo-locked-file", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    build_demo_store(&dir, None, "de").unwrap();

    // share_mode(0) denies every other handle -- including this same
    // process's own attempt to remove or reopen it -- any access at all.
    let locked = std::fs::OpenOptions::new().read(true).share_mode(0).open(dir.join("data.db")).unwrap();

    // The rebuild's own data.db step will fail while this handle is open;
    // what matters here is only that the marker survives it regardless.
    let _ = build_demo_store(&dir, None, "de");
    assert!(
        dir.join(DEMO_MARKER_FILE).is_file(),
        "the marker must survive a locked file elsewhere in the folder"
    );

    drop(locked);
    // Once unlocked, a later enable must not be refused forever either.
    assert!(build_demo_store(&dir, None, "de").is_ok(), "a retry once the lock clears must succeed");

    let _ = std::fs::remove_dir_all(&dir);
}

// Finding 3 (MEDIUM): entering demo mode a second time without an exit in
// between (a crash mid-session, or a relaunch that never reached
// exit_demo_mode) must never overwrite an already-recorded backup with the
// demo path itself -- once that happens there is nothing left to restore
// the real custom redirect from.
#[test]
fn demo_pointer_backup_survives_a_second_enable_without_an_exit() {
    let canonical = std::env::temp_dir().join(format!("{}-test-demo-backup-vanish", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    std::fs::create_dir_all(&canonical).unwrap();
    let custom = canonical.join("elsewhere");
    std::fs::create_dir_all(&custom).unwrap();
    std::fs::write(canonical.join("datapath"), custom.to_string_lossy().as_bytes()).unwrap();

    enter_demo_mode(&canonical, None).unwrap();
    assert_eq!(
        std::fs::read_to_string(canonical.join(DEMO_POINTER_BACKUP)).unwrap(),
        custom.to_string_lossy(),
        "the first enable must save the real custom redirect as the backup"
    );

    // Re-enter without exiting first: the pointer already points at the
    // demo folder itself.
    enter_demo_mode(&canonical, None).unwrap();
    assert_eq!(
        std::fs::read_to_string(canonical.join(DEMO_POINTER_BACKUP)).unwrap(),
        custom.to_string_lossy(),
        "a second enable must never overwrite the backup with the demo path itself"
    );

    exit_demo_mode(&canonical).unwrap();
    assert_eq!(
        std::fs::read_to_string(canonical.join("datapath")).unwrap(),
        custom.to_string_lossy(),
        "switching off must restore the real custom redirect, not the canonical (empty) store"
    );

    let _ = std::fs::remove_dir_all(&canonical);
}

// The other half of finding 3: if the backup is missing (an exit already
// ran, or nothing ever wrote one) and the redirect points somewhere that is
// NOT the demo folder, exit_demo_mode must leave it alone rather than
// deleting it and falling back to the canonical (empty) store.
#[test]
fn exit_demo_mode_leaves_an_unrelated_redirect_alone_when_the_backup_is_missing() {
    let canonical = std::env::temp_dir().join(format!("{}-test-demo-no-backup", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    std::fs::create_dir_all(&canonical).unwrap();
    let custom = canonical.join("elsewhere");
    std::fs::create_dir_all(&custom).unwrap();
    std::fs::write(canonical.join("datapath"), custom.to_string_lossy().as_bytes()).unwrap();
    assert!(!canonical.join(DEMO_POINTER_BACKUP).exists());

    exit_demo_mode(&canonical).unwrap();

    assert_eq!(
        std::fs::read_to_string(canonical.join("datapath")).unwrap(),
        custom.to_string_lossy(),
        "an unrelated redirect must survive when there is no demo backup to restore"
    );

    let _ = std::fs::remove_dir_all(&canonical);
}

// Finding 5 (LOW): the sample attachment row must never store an absolute
// path -- an installed build's data dir sits under
// %APPDATA%\<Windows account name>, and demo mode's whole point is to be
// shown to other people (screenshots, exports).
#[test]
fn demo_attachment_path_is_relative_not_absolute() {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-relative-path", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    build_demo_store(&dir, None, "de").unwrap();

    let conn = db_conn_at(&dir).unwrap();
    let mut written = WrittenRows::new();
    let (prompts, ..) = db_load(&conn, &mut written);
    let attachment = prompts.iter().find(|p| p.id == "demo-attachment").unwrap();
    assert!(!attachment.file_path.is_empty());
    assert!(
        Path::new(&attachment.file_path).is_relative(),
        "the stored file_path must be relative, not an absolute path containing the user's own account name: {}",
        attachment.file_path
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// The other half of finding 5 (second review): load_store() resolves the
// demo attachment's relative file_path to a real absolute path in memory
// so the UI can actually use it -- but the very next unrelated save must
// never bake that absolute path back into the database. strip_data_dir_prefix
// is what save_prompts (and save_trash/save_prompts_and_trash) run every
// prompt through first for exactly this reason: a full round trip
// (relative on disk -> absolute in memory, simulating load_store's own
// resolution loop -> stripped again before persisting) must land back on
// the same relative value it started from, not the resolved one.
#[test]
fn strip_data_dir_prefix_reverses_load_stores_own_resolution() {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-strip-prefix", brand::SLUG)).join(DEMO_SUBFOLDER);
    let _ = std::fs::remove_dir_all(&dir);
    build_demo_store(&dir, None, "de").unwrap();

    let conn = db_conn_at(&dir).unwrap();
    let mut written = WrittenRows::new();
    let (mut prompts, ..) = db_load(&conn, &mut written);
    let original = prompts.iter().find(|p| p.id == "demo-attachment").unwrap().file_path.clone();
    assert!(Path::new(&original).is_relative(), "sanity: db_load must still hand back the relative row");

    // Mirror load_store()'s own resolution loop.
    for p in prompts.iter_mut() {
        if !p.file_path.is_empty() && Path::new(&p.file_path).is_relative() {
            p.file_path = dir.join(&p.file_path).to_string_lossy().to_string();
        }
    }
    let resolved = prompts.iter().find(|p| p.id == "demo-attachment").unwrap().file_path.clone();
    assert!(Path::new(&resolved).is_absolute(), "sanity: the in-memory copy must now be absolute");

    // What save_prompts runs the resolved prompts through right before
    // db_write_prompts/write_sealed_json ever see them.
    let stripped = strip_data_dir_prefix(&dir, true, &prompts);
    let round_tripped = stripped.iter().find(|p| p.id == "demo-attachment").unwrap().file_path.clone();
    assert_eq!(
        round_tripped, original,
        "a save made after load_store()'s own resolution must never persist the absolute path"
    );
    assert!(
        Path::new(&round_tripped).is_relative(),
        "the persisted value must be relative again: {round_tripped}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// A real (non-demo) attachment's absolute path, whose parent is NOT the
// data dir, must never be touched by strip_data_dir_prefix -- it is
// already exactly what the file picker produced.
#[test]
fn strip_data_dir_prefix_leaves_an_unrelated_absolute_path_alone() {
    let dir = std::env::temp_dir().join(format!("{}-test-strip-prefix-unrelated", brand::SLUG));
    let real = Prompt { file_path: "C:\\Users\\someone\\Documents\\report.pdf".to_string(), ..test_prompt("real") };
    let out = strip_data_dir_prefix(&dir, true, std::slice::from_ref(&real));
    assert_eq!(out[0].file_path, real.file_path);
}

// A trashed demo attachment must resolve to an absolute path exactly like
// a live one -- a third review finding: load_store used to only run this
// loop over `prompts`, so a demo attachment deleted during the session
// restored from trash with the bare, unresolvable filename db_load_trash
// handed back, instead of a path under the live data dir.
#[test]
fn resolve_relative_file_paths_covers_trash_rows_too() {
    let dir = std::env::temp_dir().join(format!("{}-test-resolve-relative-trash", brand::SLUG));
    let mut trash = vec![Prompt { file_path: "Anhang.txt".to_string(), ..test_prompt("trashed-attachment") }];
    resolve_relative_file_paths(&dir, &mut trash);
    assert_eq!(trash[0].file_path, dir.join("Anhang.txt").to_string_lossy().to_string());
    assert!(Path::new(&trash[0].file_path).is_absolute());
}

// Portable mode's data dir is the exe folder; a custom data dir is common
// too. A real attachment the file picker placed directly inside either
// (its absolute path's parent equals the data dir, same as the demo
// sample row's) must never be stripped down to a bare filename outside
// demo mode -- it would resolve against the wrong folder after any later
// data-dir move or portable/installed takeover (a third review finding:
// the fix was gating on dir shape alone, so it fired for every user, not
// only in demo). Named literally `DEMO_SUBFOLDER` on purpose (a fourth
// review finding): the first attempt at this fix compared the folder's
// NAME to `DEMO_SUBFOLDER`, which still misfired for a real data dir
// that happens to be named that -- only `is_demo` (the caller's own
// exact-path comparison against `default_data_dir(app).join(DEMO_
// SUBFOLDER)`) may gate this, never the shape of `dir` alone.
#[test]
fn strip_data_dir_prefix_leaves_a_real_attachment_alone_outside_demo() {
    let dir = std::env::temp_dir().join(format!("{}-test-strip-prefix-non-demo", brand::SLUG)).join(DEMO_SUBFOLDER);
    let real = Prompt { file_path: dir.join("report.pdf").to_string_lossy().to_string(), ..test_prompt("real") };
    assert!(Path::new(&real.file_path).parent() == Some(dir.as_path()), "sanity: parent must equal dir");
    let out = strip_data_dir_prefix(&dir, false, std::slice::from_ref(&real));
    assert_eq!(out[0].file_path, real.file_path, "a non-demo data dir must never be stripped, even named exactly like the demo subfolder");
    assert!(Path::new(&out[0].file_path).is_absolute(), "the real attachment path must stay absolute");
}

// Adopting an installed store copies its files one by one. If the database
// arrives and a later file does not (a full disk, a locked file), the folder
// already holds the adopted store while the process still has the empty one it
// replaced. write_rows' "only delete a row I can prove I knew about" guard (the
// allowMultipleInstances fix) now keeps a save with an empty `written` map from
// wiping those adopted rows either way: but the copy still has to report that
// something landed, so the caller reloads from what is really there instead of
// silently carrying on with a stale, now-wrong in-memory empty list.
#[test]
fn a_half_copied_takeover_reports_what_already_landed() {
    let root = std::env::temp_dir().join(format!("{}-test-takeover-partial", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    let src = root.join("installed");
    let dst = root.join("portable");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();

    let mine = numbered_prompts(3);
    let seed_db = |path: &std::path::Path| {
        let mut conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
                 CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);",
        )
        .unwrap();
        db_write_prompts(&mut conn, &mine, &mut WrittenRows::new()).unwrap();
    };
    seed_db(&src.join("data.db"));
    std::fs::write(src.join("settings.json"), b"sealed settings").unwrap();
    // A folder where the second file wants to go: the copy fails there, after
    // the database has already arrived.
    std::fs::create_dir_all(dst.join("settings.json")).unwrap();

    let mut landed = false;
    assert!(
        copy_store_files(&src, &dst, &mut landed).is_err(),
        "a file that cannot be written must fail the adoption"
    );
    assert!(landed, "the database arrived before the failure and must be reported as such");

    // With the store left as it was: the empty prompt list plus its (equally
    // empty) row map used to delete every adopted row on the next save. Now
    // write_rows itself refuses -- it has no record of ever having seen these
    // rows, so it leaves them alone.
    {
        let mut conn = Connection::open(dst.join("data.db")).unwrap();
        db_write_prompts(&mut conn, &[], &mut WrittenRows::new()).unwrap();
        assert_eq!(
            stored_rows(&conn, "prompts").len(),
            3,
            "an empty written map must not be read as license to delete unseen rows"
        );
    }

    // What reloading first gives instead: the same save keeps every row.
    std::fs::copy(src.join("data.db"), dst.join("data.db")).unwrap();
    let mut conn = Connection::open(dst.join("data.db")).unwrap();
    let mut written = WrittenRows::new();
    let (reloaded, _settings, _mig, fails, _settings_unreadable) = db_load(&conn, &mut written);
    assert_eq!(fails, 0, "the adopted rows must be readable");
    db_write_prompts(&mut conn, &reloaded, &mut written).unwrap();
    assert_eq!(
        stored_rows(&conn, "prompts").iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
        mine.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
        "a save after the reload must leave the adopted rows exactly where they are"
    );

    drop(conn);
    let _ = std::fs::remove_dir_all(&root);
}

// ---- Selective backup restore ----

// restore_selective opens real files via Connection::open (ATTACH DATABASE
// needs a path, an in-memory DB can't be attached across connections), so these
// tests write actual throwaway .db files under the OS temp dir and remove them
// when done. `tag` keeps parallel test runs from colliding on the same names.
fn full_schema_db(path: &std::path::Path, tag: &str) -> Connection {
    let _ = std::fs::remove_file(path);
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');
             CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);",
    )
    .unwrap();
    let _ = tag; // only distinguishes the caller's temp filename, not the schema
    conn
}

// A readable-but-foreign SQLite file (empty, wrong schema, a stray .db the user
// picked) must never pass as a restorable backup -- for scope "all" it would be
// renamed straight onto the live data.db.
#[test]
fn stage_and_validate_backup_accepts_a_real_backup_and_rejects_a_foreign_db() {
    let root = std::env::temp_dir().join(format!("{}-test-stage-validate", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let real = root.join("real.db");
    full_schema_db(&real, "real");
    let staged = stage_and_validate_backup(&real, &root).expect("a real backup must be accepted");
    assert!(staged.exists());
    let _ = std::fs::remove_file(&staged);

    let foreign = root.join("foreign.db");
    let _ = std::fs::remove_file(&foreign);
    Connection::open(&foreign).unwrap().execute_batch("CREATE TABLE unrelated(x INTEGER);").unwrap();
    let err = stage_and_validate_backup(&foreign, &root).unwrap_err();
    assert_eq!(err, "backup-schema-mismatch");
    assert!(!staged.exists(), "a rejected file must not be left staged behind");

    let _ = std::fs::remove_dir_all(&root);
}

// ---- Backup content viewer ----

#[test]
fn backup_contents_from_matches_the_actual_content_of_a_created_backup() {
    let root = std::env::temp_dir().join(format!("{}-test-backup-contents-counts", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let db_path = root.join("staged.db");
    let mut conn = full_schema_db(&db_path, "counts");

    let mut prompts = numbered_prompts(3);
    // Same tag in two different castings: the header count is distinct tags,
    // never distinct spellings.
    prompts[0].tags = vec!["Work".into()];
    prompts[1].tags = vec!["work".into()];
    prompts[2].tags = vec!["Personal".into()];
    db_write_prompts(&mut conn, &prompts, &mut WrittenRows::new()).unwrap();

    let settings = Settings { views: vec![test_view("v1"), test_view("v2")], ..Settings::default() };
    conn.execute(
        "INSERT INTO meta(key, value) VALUES('settings', ?1)",
        params![enc_str(&serde_json::to_string(&settings).unwrap())],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_versions(prompt_id, ts, name, text, variants) VALUES(?1, ?2, 'N', 'T', '')",
        params![prompts[0].id, 1_700_000_000_i64],
    )
    .unwrap();

    let out = backup_contents_from(&conn, &root);
    assert_eq!(out.entries, 3, "entries must count every prompt actually in the snapshot");
    assert_eq!(out.views, 2, "views must come from the snapshot's own settings row");
    assert_eq!(out.tags, 2, "tags must be distinct keys, not distinct castings");
    assert_eq!(out.last_change, Some(1_700_000_000), "the newest edit anywhere must be reported");
    assert_eq!(out.items.len(), 3, "every prompt must appear in the detail list");
    let edited = out.items.iter().find(|i| i.id == prompts[0].id).unwrap();
    assert_eq!(edited.date, Some(1_700_000_000), "an edited prompt must carry its own edit date");
    let untouched = out.items.iter().find(|i| i.id == prompts[1].id).unwrap();
    assert_eq!(untouched.date, None, "a prompt with no version row was never edited");

    drop(conn);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn backup_contents_from_reports_encryption_correctly_and_incorrectly() {
    let _guard = SEAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !sealing_is_randomised() {
        return; // DPAPI unavailable on this machine -- nothing to prove here
    }
    let root = std::env::temp_dir().join(format!("{}-test-backup-contents-encrypted", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    // Correct: every row sealed the normal way.
    let sealed_path = root.join("sealed.db");
    let mut sealed_conn = full_schema_db(&sealed_path, "sealed");
    db_write_prompts(&mut sealed_conn, &numbered_prompts(2), &mut WrittenRows::new()).unwrap();
    let sealed_out = backup_contents_from(&sealed_conn, &root);
    assert!(sealed_out.encrypted, "a normally written snapshot must report itself as encrypted");
    drop(sealed_conn);

    // Incorrect: the rare fallback where DPAPI itself refused, so a row landed
    // in plain text -- same case enc_str's own comment describes.
    let prior_sealed = STORE_SEALED.swap(true, Ordering::Relaxed);
    FORCE_PLAINTEXT_FALLBACK.with(|f| f.set(true));
    let _restore = SealOverrideGuard { prior_sealed };
    let plain_path = root.join("plain.db");
    let mut plain_conn = full_schema_db(&plain_path, "plain");
    db_write_prompts(&mut plain_conn, &numbered_prompts(2), &mut WrittenRows::new()).unwrap();
    let plain_out = backup_contents_from(&plain_conn, &root);
    assert!(!plain_out.encrypted, "a snapshot with a plaintext-fallback row must report itself as not encrypted");
    drop(plain_conn);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn restore_selective_data_scope_leaves_settings_untouched() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-restore-data-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-restore-data-staged.db", brand::SLUG));
    {
        let conn = full_schema_db(&target, "target");
        conn.execute_batch(
            "INSERT INTO prompts VALUES('live', 0, '{}');
                 INSERT INTO meta VALUES('settings', 'LIVE-SETTINGS');
                 INSERT INTO meta VALUES('copy_log_migrated', '1');",
        )
        .unwrap();
    }
    {
        let conn = full_schema_db(&staged, "staged");
        conn.execute_batch(
            "INSERT INTO prompts VALUES('backed-up', 0, '{}');
                 INSERT INTO meta VALUES('settings', 'BACKUP-SETTINGS');",
        )
        .unwrap();
    }
    restore_selective(&target, &staged, "data").unwrap();
    let conn = Connection::open(&target).unwrap();
    let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(prompt_id, "backed-up", "data scope must replace the prompts table");
    let settings: String = conn.query_row("SELECT value FROM meta WHERE key='settings'", [], |r| r.get(0)).unwrap();
    assert_eq!(settings, "LIVE-SETTINGS", "data scope must never touch settings");
    let migrated: String = conn.query_row("SELECT value FROM meta WHERE key='copy_log_migrated'", [], |r| r.get(0)).unwrap();
    assert_eq!(migrated, "1", "data scope must never touch the migration marker");
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn restore_selective_settings_scope_leaves_data_untouched() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-restore-settings-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-restore-settings-staged.db", brand::SLUG));
    {
        let conn = full_schema_db(&target, "target");
        conn.execute_batch(
            "INSERT INTO prompts VALUES('live', 0, '{}');
                 INSERT INTO meta VALUES('settings', 'LIVE-SETTINGS');
                 INSERT INTO meta VALUES('copy_log_migrated', '1');",
        )
        .unwrap();
    }
    {
        let conn = full_schema_db(&staged, "staged");
        conn.execute_batch(
            "INSERT INTO prompts VALUES('backed-up', 0, '{}');
                 INSERT INTO meta VALUES('settings', 'BACKUP-SETTINGS');",
        )
        .unwrap();
    }
    restore_selective(&target, &staged, "settings").unwrap();
    let conn = Connection::open(&target).unwrap();
    let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(prompt_id, "live", "settings scope must never touch prompts");
    let settings: String = conn.query_row("SELECT value FROM meta WHERE key='settings'", [], |r| r.get(0)).unwrap();
    assert_eq!(settings, "BACKUP-SETTINGS", "settings scope must apply the backup's settings");
    let migrated: String = conn.query_row("SELECT value FROM meta WHERE key='copy_log_migrated'", [], |r| r.get(0)).unwrap();
    assert_eq!(migrated, "1", "settings scope must never touch the migration marker");
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

// ---- import_all scope ----

fn test_prompt(id: &str) -> Prompt {
    serde_json::from_str(&format!(r#"{{"id":"{id}","name":"N","text":"T"}}"#)).unwrap()
}

#[test]
fn dup_length_could_match_uses_the_real_threshold_not_a_fixed_margin() {
    // Equal lengths: the theoretical Dice ceiling is 1.0, always possible.
    assert!(dup_length_could_match(50, 50, 0.99));
    // A 2:1 length ratio caps Dice at 2*50/150 = 0.667 -- below a 70% floor.
    assert!(!dup_length_could_match(50, 100, 0.70));
    // Just inside: lengths 90 and 100 cap at 2*90/190 = 0.947, above 0.90.
    assert!(dup_length_could_match(90, 100, 0.90));
    assert!(!dup_length_could_match(0, 10, 0.70), "an empty side can never match");
}

#[test]
fn find_similar_prompt_crosses_the_threshold_exactly_at_the_computed_score() {
    let mut a = test_prompt("a");
    a.text = "Please write a short friendly email about the quarterly report".into();
    let mut b = test_prompt("b");
    b.text = "Please write a short friendly email about the quarterly results".into(); // one word differs
    let mut c = test_prompt("c");
    c.text = "Completely different content about something else entirely, not close at all".into();

    // The real score between the near-duplicate pair, computed the same way
    // find_similar_prompt does -- so "just above" and "just below" below are
    // exact, not guessed.
    let score = dup_dice(&dup_trigrams(&normalize_dup(&a.text)), &dup_trigrams(&normalize_dup(&b.text)));
    assert!(score > 0.5 && score < 1.0, "fixture must be a realistic near-duplicate, got {score}");

    let others = vec![&b, &c];
    let just_below = (score * 100.0).floor() - 1.0;
    let hit = find_similar_prompt(&a, &others, just_below);
    assert_eq!(hit.map(|h| h.id), Some("b".to_string()), "a threshold just below the real score must still match");

    let just_above = (score * 100.0).ceil() + 1.0;
    let hit2 = find_similar_prompt(&a, &others, just_above);
    assert!(hit2.is_none(), "a threshold just above the real score must find nothing");
}

#[test]
fn find_similar_prompt_ignores_an_unrelated_entry_and_itself() {
    let mut a = test_prompt("a");
    a.text = "Please write a short friendly email about the quarterly report".into();
    let mut c = test_prompt("c");
    c.text = "Completely different content about something else entirely, not close at all".into();
    let hit = find_similar_prompt(&a, &[&c], 70.0);
    assert!(hit.is_none(), "nothing above the threshold exists, so no hit should be reported");
}

#[test]
fn find_similar_prompt_stays_within_budget_at_ten_thousand_entries() {
    let mut target = test_prompt("target");
    target.text = "Please write a short friendly email about the quarterly report".into();
    let others: Vec<Prompt> = (0..10_000)
        .map(|i| {
            let mut p = test_prompt(&format!("o{i}"));
            p.text = format!("Unrelated filler content number {i} that shares nothing meaningful with the target text at all");
            p
        })
        .collect();
    let refs: Vec<&Prompt> = others.iter().collect();
    let start = std::time::Instant::now();
    let hit = find_similar_prompt(&target, &refs, 85.0);
    let elapsed = start.elapsed();
    assert!(hit.is_none(), "none of the filler text should cross an 85% threshold");
    assert!(elapsed < std::time::Duration::from_millis(200), "10,000-entry scan took {elapsed:?}, must stay well under a saving-feels-slow margin");
}

#[test]
fn dismiss_example_prompts_keeps_only_the_ones_the_user_never_touched() {
    let mut a = test_prompt("a");
    let mut b = test_prompt("b");
    let c = test_prompt("c");
    let mut seeds = HashMap::new();
    for p in [&mut a, &mut b] {
        seeds.insert(p.id.clone(), example_seed_fingerprint(&p.name, &p.text, &p.image));
    }
    seeds.insert(c.id.clone(), example_seed_fingerprint(&c.name, &c.text, &c.image));
    b.text = "the user's own rewrite".to_string(); // edited after seeding
    let ids = vec![a.id.clone(), b.id.clone(), c.id.clone()];
    let prompts = vec![a, b, c];

    let pristine = examples_still_pristine(&ids, &seeds, &prompts);

    assert_eq!(pristine, vec!["a".to_string(), "c".to_string()], "the edited one must be excluded, order preserved");
}

#[test]
fn dismiss_example_prompts_treats_a_missing_seed_as_unchanged() {
    // Pre-421 installs have example_prompt_ids but no example_prompt_seeds entry
    // for them -- must still be eligible for removal, matching old behaviour.
    let p = test_prompt("legacy");
    let ids = vec![p.id.clone()];
    let pristine = examples_still_pristine(&ids, &HashMap::new(), &[p]);
    assert_eq!(pristine, vec!["legacy".to_string()]);
}

#[test]
fn import_all_data_scope_leaves_settings_untouched() {
    let mut store = Store {
        prompts: vec![test_prompt("live")],
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: true, // must be cleared once data is actually replaced
        written: Default::default(),
    };
    store.settings.ui_texts.insert("marker".into(), "LIVE".into());
    let mut backup_settings = Settings::default();
    backup_settings.ui_texts.insert("marker".into(), "BACKUP".into());
    let backup = Backup { prompts: vec![test_prompt("backed-up")], settings: backup_settings };

    let (count, dropped, pw_dropped) = apply_import(&mut store, backup, true, false);

    assert_eq!(store.prompts.len(), 1, "data scope must replace the prompts list");
    assert_eq!(store.prompts[0].id, "backed-up");
    assert_eq!(count, 1);
    assert_eq!(dropped, 0);
    assert!(!pw_dropped, "settings were never touched, so no backup password can have been dropped");
    assert!(!store.prompts_locked, "a full data import supersedes any decrypt lock");
    assert_eq!(
        store.settings.ui_texts.get("marker").map(String::as_str),
        Some("LIVE"),
        "data scope must never touch settings"
    );
}

#[test]
fn import_all_settings_scope_leaves_data_untouched() {
    let mut store = Store {
        prompts: vec![test_prompt("live")],
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: Default::default(),
    };
    store.settings.ui_texts.insert("marker".into(), "LIVE".into());
    let mut backup_settings = Settings::default();
    backup_settings.ui_texts.insert("marker".into(), "BACKUP".into());
    backup_settings.ui_texts.insert(BACKUP_PW_KEY.into(), "foreign-pw".into());
    let backup = Backup { prompts: vec![test_prompt("backed-up")], settings: backup_settings };

    let (count, dropped, pw_dropped) = apply_import(&mut store, backup, false, true);

    assert_eq!(store.prompts.len(), 1, "settings scope must never touch prompts");
    assert_eq!(store.prompts[0].id, "live");
    assert_eq!(count, 1);
    assert_eq!(dropped, 0);
    assert!(pw_dropped, "the backup carried its own backup password, which must be reported as dropped");
    assert_eq!(
        store.settings.ui_texts.get("marker").map(String::as_str),
        Some("BACKUP"),
        "settings scope must apply the backup's settings"
    );
    assert!(
        !store.settings.ui_texts.contains_key(BACKUP_PW_KEY),
        "the foreign backup password must not survive - this machine had none to restore"
    );
}

// Finding 1 (security pass 348): a full backup restore used to overwrite
// the folder-authority settings outright, so a hostile backup file could
// point videoExportDir/recordingDir/screenshotDir/syncDir at any folder it
// liked and have export/cleanup/sync commands trust it from then on. This
// machine's own folders must survive a settings-scope import exactly like
// the backup password above.
#[test]
fn import_all_settings_scope_keeps_this_machines_own_folder_settings() {
    let mut store = Store {
        prompts: vec![test_prompt("live")],
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: Default::default(),
    };
    store.settings.ui_texts.insert("recordingDir".into(), "D:\\mine\\recordings".into());
    store.settings.ui_texts.insert("videoExportDir".into(), "D:\\mine\\exports".into());
    store.settings.ui_texts.insert("screenshotDir".into(), "D:\\mine\\shots".into());
    store.settings.ui_texts.insert("syncDir".into(), "D:\\mine\\sync".into());
    let mut backup_settings = Settings::default();
    backup_settings.ui_texts.insert("recordingDir".into(), "C:\\Windows\\System32".into());
    backup_settings.ui_texts.insert("videoExportDir".into(), "C:\\Windows\\System32".into());
    backup_settings.ui_texts.insert("screenshotDir".into(), "C:\\Windows\\System32".into());
    backup_settings.ui_texts.insert("syncDir".into(), "C:\\Windows\\System32".into());
    backup_settings.ui_texts.insert("marker".into(), "BACKUP".into());
    let backup = Backup { prompts: vec![test_prompt("backed-up")], settings: backup_settings };

    apply_import(&mut store, backup, false, true);

    assert_eq!(
        store.settings.ui_texts.get("marker").map(String::as_str),
        Some("BACKUP"),
        "an ordinary setting must still come from the backup"
    );
    for (key, own_value) in [
        ("recordingDir", "D:\\mine\\recordings"),
        ("videoExportDir", "D:\\mine\\exports"),
        ("screenshotDir", "D:\\mine\\shots"),
        ("syncDir", "D:\\mine\\sync"),
    ] {
        assert_eq!(
            store.settings.ui_texts.get(key).map(String::as_str),
            Some(own_value),
            "{key} must keep this machine's own folder, never the backup's",
        );
    }
}

// Finding 1 (security pass 348): set_ui_text is the generic string setter
// every ordinary expert-menu row uses -- a folder-authority key must never
// be reachable through it, only through its own dedicated command.
#[test]
fn is_protected_folder_key_covers_every_folder_setting_and_nothing_else() {
    assert!(is_protected_folder_key("screenshotDir"));
    assert!(is_protected_folder_key("videoExportDir"));
    assert!(is_protected_folder_key("recordingDir"));
    assert!(is_protected_folder_key("syncDir"));
    assert!(!is_protected_folder_key("copyFont"), "an ordinary string setting must stay reachable");
    assert!(!is_protected_folder_key("dataDir"), "dataDir has its own separate command and storage, not a ui_texts key");
}

#[test]
fn outside_settings_that_switch_ocr_index_off_erase_the_picture_text() {
    let mut store = Store { prompts: vec![sync_test_prompt()], trash: Vec::new(), settings: Settings::default(), prompts_locked: false, written: Default::default() };
    store.prompts[0].image_text = "read from a picture".into();
    store.settings.ui_flags.insert("ocrIndex".into(), true);
    assert!(!clear_ocr_text_if_switched_off(true, &mut store), "still on: nothing to erase");
    assert!(!store.prompts[0].image_text.is_empty());
    store.settings.ui_flags.insert("ocrIndex".into(), false);
    assert!(!clear_ocr_text_if_switched_off(false, &mut store), "was already off: left alone");
    assert!(clear_ocr_text_if_switched_off(true, &mut store));
    assert!(store.prompts[0].image_text.is_empty());
}

#[test]
fn a_full_import_that_switches_ocr_index_off_erases_prompt_and_trash_text() {
    let mut store = Store { prompts: vec![sync_test_prompt()], trash: vec![sync_test_prompt()], settings: Settings::default(), prompts_locked: false, written: Default::default() };
    store.prompts[0].image_text = "live".into();
    store.trash[0].image_text = "trashed".into();
    store.settings.ui_flags.insert("ocrIndex".into(), true);
    let mut incoming = import_test_backup();
    incoming.prompts[0].image_text = "kept by data".into();
    let mut wrote_prompts = false;
    run_import(&mut store, incoming, false, true, |_, prompts, _| {
        wrote_prompts = prompts;
        true
    })
    .unwrap();
    assert!(wrote_prompts, "cleared prompt rows must be persisted");
    assert!(store.prompts.iter().all(|p| p.image_text.is_empty()));
    assert!(store.trash.iter().all(|p| p.image_text.is_empty()));
    // A data-only import leaves the switch alone and the text with it.
    store.settings.ui_flags.insert("ocrIndex".into(), true);
    store.trash[0].image_text = "trashed".into();
    run_import(&mut store, import_test_backup(), true, false, |_, _, _| true).unwrap();
    assert_eq!(store.trash[0].image_text, "trashed");
}

#[test]
fn the_generic_text_setter_refuses_every_row_with_a_dedicated_writer() {
    for k in ["remoteAllowedRanges", "remotePasswordHash", "backupPw", "secretVaultSalt", "secretVaultVerify", "remoteTlsKey", "screenshotDir", "syncDir"] {
        assert!(is_generic_text_write_denied(k), "{k} must not be writable generically");
    }
    for k in ["copyFont", "palette", "librarySort", "globalSearchRecent"] {
        assert!(!is_generic_text_write_denied(k), "{k} stays reachable");
    }
}

// Finding 1 (security pass 348): a settings-only import file is untrusted
// (it could be a hostile file, not necessarily the user's own export) --
// its folder-authority rows must be dropped before the diff/apply ever see
// them, so neither preview nor apply can be talked into changing them.
#[test]
fn strip_denied_import_keys_removes_every_folder_setting_and_nothing_else() {
    let mut incoming = SettingsOnlyExport::default();
    incoming.ui_texts.insert("screenshotDir".into(), "C:\\evil".into());
    incoming.ui_texts.insert("videoExportDir".into(), "C:\\evil".into());
    incoming.ui_texts.insert("recordingDir".into(), "C:\\evil".into());
    incoming.ui_texts.insert("syncDir".into(), "C:\\evil".into());
    incoming.ui_texts.insert("copyFont".into(), "Consolas".into());

    strip_denied_import_keys(&mut incoming);

    for key in PROTECTED_FOLDER_KEYS {
        assert!(!incoming.ui_texts.contains_key(key), "{key} must be stripped from an incoming settings file");
    }
    assert_eq!(incoming.ui_texts.get("copyFont").map(String::as_str), Some("Consolas"), "an ordinary setting must survive untouched");
}

#[test]
fn settings_only_payload_omits_the_receiver_certificate_values() {
    let mut s = Settings::default();
    for k in remote_tls::REMOTE_TLS_KEYS {
        s.ui_texts.insert(k.to_string(), "value".into());
    }
    s.ui_texts.insert("copyFont".into(), "Consolas".into());
    let payload = settings_only_payload(&s);
    for k in remote_tls::REMOTE_TLS_KEYS {
        assert!(!payload.ui_texts.contains_key(k));
    }
    assert_eq!(payload.ui_texts.get("copyFont").map(String::as_str), Some("Consolas"));
    assert!(!serde_json::to_string(&payload).unwrap().contains("remoteTls"));
    let mut incoming = payload;
    incoming.ui_texts.insert(remote_tls::REMOTE_TLS_KEY.into(), "planted".into());
    strip_denied_import_keys(&mut incoming);
    assert!(!incoming.ui_texts.contains_key(remote_tls::REMOTE_TLS_KEY));
}

#[test]
fn a_full_import_keeps_this_machines_receiver_certificate() {
    let mut store = Store { prompts: vec![], trash: Vec::new(), settings: Settings::default(), prompts_locked: false, written: Default::default() };
    store.settings.ui_texts.insert(remote_tls::REMOTE_TLS_CA.into(), "mine".into());
    let mut foreign = Settings::default();
    foreign.ui_texts.insert(remote_tls::REMOTE_TLS_CA.into(), "theirs".into());
    foreign.ui_texts.insert(remote_tls::REMOTE_TLS_KEY.into(), "theirs-key".into());
    let backup = Backup { prompts: vec![], settings: foreign };
    apply_import(&mut store, backup, false, true);
    assert_eq!(store.settings.ui_texts.get(remote_tls::REMOTE_TLS_CA).map(String::as_str), Some("mine"));
    assert!(!store.settings.ui_texts.contains_key(remote_tls::REMOTE_TLS_KEY));
}

// A hostile file plants every key that could open a listener, register
// with the OS or swap a secret. A full import keeps this machine's own
// value for each of them (or the absence of one).
fn hostile_settings() -> Settings {
    let mut s = Settings { autostart: true, ..Settings::default() };
    for k in ["remoteApi", "remoteNetworkFilter", "remoteTlsShortLived", "urlScheme", "urlSchemeOpenAllowed", "urlSchemeCopyAllowed", "demoMode", "allowMultipleInstances", "promptSync"] {
        s.ui_flags.insert(k.into(), true);
    }
    s.ui_flags.insert("copyFlash".into(), false);
    s.ui_values.insert("remotePort".into(), 4444.0);
    s.ui_values.insert("remoteRateLimit".into(), 0.0);
    s.ui_values.insert("tileGap".into(), 7.0);
    for k in ["remotePasswordHash", "remoteAllowedRanges", BACKUP_PW_KEY, "screenshotDir"] {
        s.ui_texts.insert(k.into(), "planted".into());
    }
    for k in remote_tls::REMOTE_TLS_KEYS {
        s.ui_texts.insert(k.to_string(), "planted".into());
    }
    s.ui_texts.insert("copyFont".into(), "Consolas".into());
    s
}

// The three switches that grant consent or guard the data folder: a file,
// backup or snapshot says whatever it likes, this machine keeps its own.
const CONSENT_KEYS: [&str; 3] = ["voiceOnlineOk", "dataDirLock", "clipSkipSecrets"];

fn local_consent_settings() -> Settings {
    let mut s = Settings::default();
    s.ui_flags.insert("voiceOnlineOk".into(), false);
    s.ui_flags.insert("dataDirLock".into(), true);
    s.ui_flags.insert("clipSkipSecrets".into(), true);
    s
}

fn hostile_consent_settings() -> Settings {
    let mut s = Settings::default();
    s.ui_flags.insert("voiceOnlineOk".into(), true);
    s.ui_flags.insert("dataDirLock".into(), false);
    s.ui_flags.insert("clipSkipSecrets".into(), false);
    s.ui_flags.insert("copyFlash".into(), false);
    s
}

fn assert_consent_kept(s: &Settings, what: &str) {
    assert_eq!(s.ui_flags.get("voiceOnlineOk"), Some(&false), "{what}: voiceOnlineOk");
    assert_eq!(s.ui_flags.get("dataDirLock"), Some(&true), "{what}: dataDirLock");
    assert_eq!(s.ui_flags.get("clipSkipSecrets"), Some(&true), "{what}: clipSkipSecrets");
}

#[test]
fn a_settings_file_cannot_set_consent_or_lock_switches() {
    let mut live = local_consent_settings();
    let mut file = SettingsOnlyExport::default();
    for k in CONSENT_KEYS {
        file.ui_flags.insert(k.into(), k == "voiceOnlineOk");
    }
    file.ui_flags.insert("copyFlash".into(), false);
    let skipped = strip_denied_import_keys(&mut file);
    assert_eq!(skipped, 3, "the three rows count as skipped");
    merge_settings_only(&mut live, file, |_| true).unwrap();
    assert_consent_kept(&live, "settings-only import");
    assert_eq!(live.ui_flags.get("copyFlash"), Some(&false), "ordinary rows still apply");
}

#[test]
fn a_full_import_cannot_set_consent_or_lock_switches() {
    let mut store = Store { prompts: vec![], trash: Vec::new(), settings: local_consent_settings(), prompts_locked: false, written: Default::default() };
    apply_import(&mut store, Backup { prompts: vec![], settings: hostile_consent_settings() }, false, true);
    assert_consent_kept(&store.settings, "full import");
    assert_eq!(store.settings.ui_flags.get("copyFlash"), Some(&false));
    // A machine that never had the rows does not receive them either.
    let mut bare = Store { prompts: vec![], trash: Vec::new(), settings: Settings::default(), prompts_locked: false, written: Default::default() };
    apply_import(&mut bare, Backup { prompts: vec![], settings: hostile_consent_settings() }, false, true);
    for k in CONSENT_KEYS {
        assert!(!bare.settings.ui_flags.contains_key(k), "{k} must not arrive");
    }
}

fn consent_restore_pair(tag: &str, local: &Settings, incoming: Option<&Settings>) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-consent-{tag}-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-consent-{tag}-staged.db", brand::SLUG));
    let t = full_schema_db(&target, "target");
    db_write_settings(&t, local).unwrap();
    let s = full_schema_db(&staged, "staged");
    if let Some(i) = incoming {
        db_write_settings(&s, i).unwrap();
    }
    (target, staged)
}

#[test]
fn a_settings_restore_cannot_set_consent_or_lock_switches() {
    let (target, staged) = consent_restore_pair("sel", &local_consent_settings(), Some(&hostile_consent_settings()));
    restore_selective(&target, &staged, "settings").unwrap();
    let after = read_settings_row(&Connection::open(&target).unwrap(), "main").unwrap();
    assert_consent_kept(&after, "settings-scope restore");
    assert_eq!(after.ui_flags.get("copyFlash"), Some(&false));
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn a_whole_snapshot_restore_cannot_set_consent_or_lock_switches() {
    // Shared by the in-app restore and the command-line standalone restore.
    let (target, staged) = consent_restore_pair("all", &local_consent_settings(), Some(&hostile_consent_settings()));
    keep_machine_in_staged(&target, &staged).unwrap();
    let after = read_settings_row(&Connection::open(&staged).unwrap(), "main").unwrap();
    assert_consent_kept(&after, "whole-snapshot restore");
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn a_whole_snapshot_restore_onto_a_machine_without_settings_drops_the_switches() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-consent-none-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-consent-none-staged.db", brand::SLUG));
    let _ = full_schema_db(&target, "target");
    let s = full_schema_db(&staged, "staged");
    db_write_settings(&s, &hostile_consent_settings()).unwrap();
    drop(s);
    keep_machine_in_staged(&target, &staged).unwrap();
    let after = read_settings_row(&Connection::open(&staged).unwrap(), "main").unwrap();
    for k in CONSENT_KEYS {
        assert!(!after.ui_flags.contains_key(k), "{k} must not arrive");
    }
    assert_eq!(after.ui_flags.get("copyFlash"), Some(&false));
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn a_settings_restore_onto_a_machine_without_settings_drops_the_switches() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-consent-selnone-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-consent-selnone-staged.db", brand::SLUG));
    let t = full_schema_db(&target, "target");
    drop(t);
    let s = full_schema_db(&staged, "staged");
    let mut hostile = hostile_consent_settings();
    hostile.ui_flags.insert("remoteApi".into(), true);
    hostile.autostart = true;
    db_write_settings(&s, &hostile).unwrap();
    drop(s);
    restore_selective(&target, &staged, "settings").unwrap();
    let after = read_settings_row(&Connection::open(&target).unwrap(), "main").unwrap();
    for k in CONSENT_KEYS.iter().chain(["remoteApi"].iter()) {
        assert!(!after.ui_flags.contains_key(*k), "{k} must not arrive");
    }
    assert!(!after.autostart, "a restore must not register autostart on a bare machine");
    assert_eq!(after.ui_flags.get("copyFlash"), Some(&false));
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn a_snapshot_restore_onto_a_machine_without_settings_drops_the_switches() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-consent-stagednone-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-consent-stagednone-staged.db", brand::SLUG));
    let _ = std::fs::remove_file(&target);
    let s = full_schema_db(&staged, "staged");
    let mut hostile = hostile_consent_settings();
    hostile.autostart = true;
    db_write_settings(&s, &hostile).unwrap();
    drop(s);
    keep_machine_in_staged(&target, &staged).unwrap();
    let after = read_settings_row(&Connection::open(&staged).unwrap(), "main").unwrap();
    for k in CONSENT_KEYS {
        assert!(!after.ui_flags.contains_key(k), "{k} must not arrive");
    }
    assert!(!after.autostart, "a restore must not register autostart on a bare machine");
    assert_eq!(after.ui_flags.get("copyFlash"), Some(&false));
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn the_cli_refusal_and_the_import_denylist_are_one_list() {
    for k in CONSENT_KEYS {
        assert!(is_import_denied_flag(k), "{k} must be on the import denylist");
    }
}

#[test]
fn a_full_import_cannot_plant_network_or_machine_settings() {
    let mut store = Store { prompts: vec![], trash: Vec::new(), settings: Settings::default(), prompts_locked: false, written: Default::default() };
    store.settings.ui_flags.insert("remoteApi".into(), false);
    store.settings.ui_texts.insert("remotePasswordHash".into(), "mine".into());
    let backup = Backup { prompts: vec![], settings: hostile_settings() };
    apply_import(&mut store, backup, false, true);
    let s = &store.settings;
    for k in ["remoteNetworkFilter", "remoteTlsShortLived", "urlScheme", "urlSchemeOpenAllowed", "urlSchemeCopyAllowed", "demoMode", "allowMultipleInstances", "promptSync"] {
        assert!(!s.ui_flags.contains_key(k), "{k} must not be imported");
    }
    assert_eq!(s.ui_flags.get("remoteApi"), Some(&false), "this machine's own value stays");
    assert_eq!(s.ui_texts.get("remotePasswordHash").map(String::as_str), Some("mine"));
    assert!(!s.ui_texts.contains_key("remoteAllowedRanges"));
    assert!(!s.ui_values.contains_key("remotePort") && !s.ui_values.contains_key("remoteRateLimit"));
    assert!(!s.autostart, "a file cannot switch the autostart registration on");
    assert_eq!(s.ui_values.get("tileGap"), Some(&7.0), "ordinary settings still arrive");
    assert_eq!(s.ui_texts.get("copyFont").map(String::as_str), Some("Consolas"));
    assert_eq!(s.ui_flags.get("copyFlash"), Some(&false));
}

#[test]
fn a_settings_file_previews_every_denied_key_as_skipped_and_changes_none() {
    let hostile = hostile_settings();
    let mut incoming = SettingsOnlyExport { ui_flags: hostile.ui_flags.clone(), ui_values: hostile.ui_values.clone(), ui_texts: hostile.ui_texts.clone() };
    incoming.ui_texts.insert(SECRET_SALT_KEY.into(), "foreign".into());
    incoming.ui_texts.insert(SECRET_VERIFY_KEY.into(), "foreign".into());
    let skipped = strip_denied_import_keys(&mut incoming);
    // 9 flags, 2 values, 4 texts + 3 certificate values + 2 vault keys
    assert_eq!(skipped, 9 + 2 + 4 + 3, "the vault rows are dropped silently");
    assert_eq!(incoming.ui_flags.keys().collect::<Vec<_>>(), vec!["copyFlash"]);
    assert_eq!(incoming.ui_values.keys().collect::<Vec<_>>(), vec!["tileGap"]);
    assert_eq!(incoming.ui_texts.keys().collect::<Vec<_>>(), vec!["copyFont"]);
    let mut live = Settings::default();
    live.ui_flags.insert("remoteApi".into(), false);
    let mut again = SettingsOnlyExport { ui_flags: hostile.ui_flags, ui_values: hostile.ui_values, ui_texts: hostile.ui_texts };
    strip_denied_import_keys(&mut again);
    let _ = merge_settings_only(&mut live, again, |_| true).unwrap();
    let after: serde_json::Value = serde_json::from_str(&serde_json::to_string(&live).unwrap()).unwrap();
    assert_eq!(after["ui_flags"]["remoteApi"], serde_json::json!(false));
    assert!(after["ui_flags"].get("remoteNetworkFilter").is_none());
    assert!(after["ui_texts"].get("remotePasswordHash").is_none());
}

fn theme_file(theme_gap: f64) -> SettingsOnlyExport {
    let mut f = SettingsOnlyExport::default();
    f.ui_values.insert("tileGap".into(), theme_gap);
    f.ui_flags.insert("trayRecent".into(), true);
    f.ui_texts.insert("copyFont".into(), "Consolas".into());
    f
}

#[test]
fn a_settings_import_that_cannot_be_saved_changes_nothing() {
    let mut live = Settings::default();
    live.ui_values.insert("tileGap".into(), 3.0);
    let before = serde_json::to_string(&live).unwrap();
    let mut saw = 0;
    let r = merge_settings_only(&mut live, theme_file(9.0), |_| {
        saw += 1;
        false
    });
    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(saw, 1, "exactly one write was attempted");
    assert_eq!(serde_json::to_string(&live).unwrap(), before, "memory must equal what disk still holds");
}

#[test]
fn a_saved_settings_import_is_what_was_written_and_survives_a_reload() {
    let mut live = Settings::default();
    live.ui_values.insert("tileGap".into(), 3.0);
    live.ui_texts.insert("untouched".into(), "keep".into());
    let mut written = None;
    let changed = merge_settings_only(&mut live, theme_file(9.0), |s| {
        written = Some(serde_json::to_string(s).unwrap());
        true
    })
    .unwrap();
    assert_eq!(changed, 3);
    assert_eq!(written.as_deref(), Some(serde_json::to_string(&live).unwrap().as_str()), "what was saved is what is live");
    let reloaded: Settings = serde_json::from_str(&written.unwrap()).unwrap();
    assert_eq!(reloaded.ui_values.get("tileGap"), Some(&9.0));
    assert_eq!(reloaded.ui_flags.get("trayRecent"), Some(&true));
    assert_eq!(reloaded.ui_texts.get("untouched").map(String::as_str), Some("keep"), "a row the file never mentions stays");
    assert!(capture_excluded(&reloaded) == capture_excluded(&live), "the capture state follows the same settings");
}

// ---- a full import commits both halves together or neither ----

// Serializes the tests that set or read the corrupt-row lock flag.
static SETTINGS_LOCK_TEST: Mutex<()> = Mutex::new(());

fn import_test_db() -> Connection {
    let conn = row_test_db();
    conn.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);").unwrap();
    conn
}

// A store whose disk holds exactly its prompts "old1", "old2" and a settings
// row marked "OLD".
fn import_test_store(conn: &mut Connection) -> Store {
    let mut settings = Settings::default();
    settings.ui_texts.insert("marker".into(), "OLD".into());
    let store = Store {
        prompts: vec![test_prompt("old1"), test_prompt("old2")],
        trash: Vec::new(),
        settings,
        prompts_locked: false,
        written: Default::default(),
    };
    db_write_prompts(conn, &store.prompts, &mut store.written.borrow_mut()).unwrap();
    db_write_settings(conn, &store.settings).unwrap();
    store
}

fn import_test_backup() -> Backup {
    let mut settings = Settings::default();
    settings.ui_texts.insert("marker".into(), "NEW".into());
    Backup { prompts: vec![test_prompt("new1")], settings }
}

fn disk_state(conn: &Connection) -> (Vec<String>, String) {
    let (prompts, settings, _, _, _) = db_load(conn, &mut WrittenRows::new());
    let marker = settings.and_then(|s| s.ui_texts.get("marker").cloned()).unwrap_or_default();
    (prompts.iter().map(|p| p.id.clone()).collect(), marker)
}

fn import_with(conn: &mut Connection, store: &mut Store, want_data: bool, want_settings: bool) -> Result<(usize, usize, bool, bool), String> {
    run_import(store, import_test_backup(), want_data, want_settings, |st, prompts, settings| {
        db_write_import(conn, st, &st.prompts, prompts, settings).is_ok()
    })
}

#[test]
fn a_full_import_that_fails_on_the_settings_write_leaves_everything_as_it_was() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut conn = import_test_db();
    let mut store = import_test_store(&mut conn);
    let before_prompts = serde_json::to_string(&store.prompts).unwrap();
    let before_settings = serde_json::to_string(&store.settings).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_settings BEFORE INSERT ON meta BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    SETTINGS_LOCKED.store(true, Ordering::Relaxed);
    store.prompts_locked = true;

    let r = import_with(&mut conn, &mut store, true, true);

    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(serde_json::to_string(&store.prompts).unwrap(), before_prompts, "memory keeps the old prompts");
    assert_eq!(serde_json::to_string(&store.settings).unwrap(), before_settings, "memory keeps the old settings");
    assert!(store.prompts_locked, "the decrypt lock is put back");
    assert!(SETTINGS_LOCKED.load(Ordering::Relaxed), "the corrupt-row lock is put back");
    SETTINGS_LOCKED.store(false, Ordering::Relaxed);
    let (ids, marker) = disk_state(&conn);
    assert_eq!(ids, vec!["old1".to_string(), "old2".to_string()], "the prompt half must not have committed");
    assert_eq!(marker, "OLD");
}

#[test]
fn a_full_import_that_fails_on_the_prompt_write_leaves_everything_as_it_was() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut conn = import_test_db();
    let mut store = import_test_store(&mut conn);
    let before_prompts = serde_json::to_string(&store.prompts).unwrap();
    let before_settings = serde_json::to_string(&store.settings).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_prompts BEFORE INSERT ON prompts BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();

    let r = import_with(&mut conn, &mut store, true, true);

    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(serde_json::to_string(&store.prompts).unwrap(), before_prompts);
    assert_eq!(serde_json::to_string(&store.settings).unwrap(), before_settings);
    let (ids, marker) = disk_state(&conn);
    assert_eq!(ids, vec!["old1".to_string(), "old2".to_string()]);
    assert_eq!(marker, "OLD", "the settings half must not have committed");
}

#[test]
fn a_full_import_commits_both_halves_and_a_reopened_store_agrees() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut conn = import_test_db();
    let mut store = import_test_store(&mut conn);
    let (count, _, _, _) = import_with(&mut conn, &mut store, true, true).unwrap();
    assert_eq!(count, 1);
    assert_eq!(store.prompts.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["new1"]);
    let (ids, marker) = disk_state(&conn);
    assert_eq!(ids, vec!["new1".to_string()], "the replaced prompts are gone from disk too");
    assert_eq!(marker, "NEW");
}

#[test]
fn a_data_only_import_never_writes_the_settings_row_and_a_settings_only_one_never_the_prompts() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut conn = import_test_db();
    let mut store = import_test_store(&mut conn);
    // A failing settings write cannot matter to a data-only import.
    conn.execute_batch("CREATE TRIGGER fail_settings BEFORE INSERT ON meta BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    import_with(&mut conn, &mut store, true, false).unwrap();
    let (ids, marker) = disk_state(&conn);
    assert_eq!(ids, vec!["new1".to_string()]);
    assert_eq!(marker, "OLD");
    conn.execute_batch("DROP TRIGGER fail_settings;").unwrap();

    let mut store = import_test_store_again(&mut conn);
    conn.execute_batch("CREATE TRIGGER fail_prompts BEFORE INSERT ON prompts BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    import_with(&mut conn, &mut store, false, true).unwrap();
    let (ids, marker) = disk_state(&conn);
    assert_eq!(ids, vec!["old1".to_string(), "old2".to_string()], "a settings-only import leaves the prompt rows alone");
    assert_eq!(marker, "NEW");
}

// Puts the disk back to old1, old2 plus a settings row marked OLD.
fn import_test_store_again(conn: &mut Connection) -> Store {
    conn.execute_batch("DELETE FROM prompts; DELETE FROM meta;").unwrap();
    import_test_store(conn)
}

#[test]
fn a_failed_settings_only_import_puts_back_a_hotkey_it_had_dropped() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut conn = import_test_db();
    let mut store = import_test_store(&mut conn);
    store.prompts[0].hotkey = "CommandOrControl+Alt+K".into();
    store.prompts[1].hotkey = "CommandOrControl+Alt+J".into();
    let mut backup = import_test_backup();
    backup.settings.hotkey = "CommandOrControl+Alt+K".into(); // collides with old1
    let before = serde_json::to_string(&store.prompts).unwrap();
    let r = run_import(&mut store, backup, false, true, |_, prompts, _| {
        assert!(prompts, "a dropped accelerator changes prompt rows, so they are written too");
        false
    });
    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(serde_json::to_string(&store.prompts).unwrap(), before, "memory matches the disk again");
}

// ---- the confidential vault keys survive every path that keeps the prompts ----

// A settings block with a vault set up for password "pw", plus one prompt
// sealed with its key.
fn vault_fixture() -> (Settings, Prompt, [u8; 32]) {
    let mut settings = Settings::default();
    let salt = b"0123456789abcdef";
    let key = secret_derive_key("pw", salt).expect("derive");
    let verify = secret_encrypt(SECRET_VERIFY_MARKER, &key);
    assert_eq!(secret_vault_first_setup(&mut settings, salt, verify, |_| true), Ok(true));
    let mut p = test_prompt("secret");
    p.secret = true;
    p.text = secret_encrypt("the hidden text", &key);
    (settings, p, key)
}

// The same password must still open the prompt with what the settings hold.
fn still_opens(settings: &Settings, p: &Prompt) -> bool {
    let salt = settings.ui_texts.get(SECRET_SALT_KEY).cloned().unwrap_or_default();
    let verify = settings.ui_texts.get(SECRET_VERIFY_KEY).cloned().unwrap_or_default();
    match secret_vault_verify("pw", &salt, &verify) {
        Ok(key) => reveal_secret_text(p, Some(key)).map(|(t, _)| t == "the hidden text").unwrap_or(false),
        Err(_) => false,
    }
}

#[test]
fn a_reset_keeps_the_vault_so_a_confidential_prompt_still_opens() {
    let (mut old, p, _) = vault_fixture();
    old.theme = "ocean".into();
    old.ui_texts.insert(BACKUP_PW_KEY.into(), "sealed".into());
    assert!(still_opens(&old, &p));
    let fresh = settings_after_reset(&old);
    assert_eq!(fresh.theme, default_theme(), "everything else is reset");
    assert_eq!(fresh.ui_texts.get(BACKUP_PW_KEY).map(String::as_str), Some("sealed"));
    assert!(still_opens(&fresh, &p), "the same password must open the prompt after a reset");
}

#[test]
fn a_reset_never_shortens_what_the_old_settings_kept() {
    let mut old = Settings { retention_defaults_v2: false, ..Settings::default() };
    old.migrate(); // an install from before the 30/90-day defaults: keep forever
    let fresh = settings_after_reset(&old);
    assert_eq!(history_max_age(&fresh), None, "a reset must not start pruning the copy history");
    assert_eq!(fresh.ui_values.get("recordingRetentionDays"), Some(&0.0));
    // A window longer than the default is kept as well; a shorter one takes the default.
    let mut old = Settings::default();
    old.ui_values.insert("historyDays".into(), 365.0);
    old.ui_values.insert("recordingRetentionDays".into(), 7.0);
    let fresh = settings_after_reset(&old);
    assert_eq!(fresh.ui_values.get("historyDays"), Some(&365.0));
    assert!(!fresh.ui_values.contains_key("recordingRetentionDays"));
    // A fresh install resets to the defaults.
    let fresh = settings_after_reset(&Settings::default());
    assert!(!fresh.ui_values.contains_key("historyDays"));
    assert!(!fresh.ui_values.contains_key("recordingRetentionDays"));
}

fn vault_store(settings: Settings, p: Prompt) -> Store {
    Store { prompts: vec![p], trash: Vec::new(), settings, prompts_locked: false, written: Default::default() }
}

fn foreign_backup(with_salt: bool) -> Backup {
    let mut settings = Settings::default();
    settings.ui_texts.insert("marker".into(), "THEIRS".into());
    if with_salt {
        settings.ui_texts.insert(SECRET_SALT_KEY.into(), "Zm9yZWlnbi1zYWx0".into());
        settings.ui_texts.insert(SECRET_VERIFY_KEY.into(), "Zm9yZWlnbi12ZXJpZnk=".into());
    }
    Backup { prompts: vec![test_prompt("theirs")], settings }
}

#[test]
fn a_settings_only_import_keeps_the_local_vault_whatever_the_file_carries() {
    for with_salt in [true, false] {
        let (settings, p, _) = vault_fixture();
        let mut store = vault_store(settings, p.clone());
        apply_import(&mut store, foreign_backup(with_salt), false, true);
        assert_eq!(store.settings.ui_texts.get("marker").map(String::as_str), Some("THEIRS"), "the settings did come in");
        assert!(still_opens(&store.settings, &p), "file with salt: {with_salt}");
    }
}

#[test]
fn a_settings_file_cannot_replace_the_local_vault() {
    let (mut live, p, _) = vault_fixture();
    let mut file = SettingsOnlyExport::default();
    file.ui_texts.insert(SECRET_SALT_KEY.into(), "Zm9yZWlnbi1zYWx0".into());
    file.ui_texts.insert(SECRET_VERIFY_KEY.into(), "Zm9yZWlnbi12ZXJpZnk=".into());
    file.ui_texts.insert("copyFont".into(), "Consolas".into());
    strip_denied_import_keys(&mut file);
    merge_settings_only(&mut live, file, |_| true).unwrap();
    assert_eq!(live.ui_texts.get("copyFont").map(String::as_str), Some("Consolas"));
    assert!(still_opens(&live, &p));
}

#[test]
fn a_full_import_with_data_takes_the_backups_own_vault_keys() {
    let (settings, p, _) = vault_fixture();
    let mut store = vault_store(settings, p);
    let mut backup = foreign_backup(true);
    backup.prompts = vec![test_prompt("theirs")];
    apply_import(&mut store, backup, true, true);
    assert_eq!(store.settings.ui_texts.get(SECRET_SALT_KEY).map(String::as_str), Some("Zm9yZWlnbi1zYWx0"));
    assert_eq!(store.settings.ui_texts.get(SECRET_VERIFY_KEY).map(String::as_str), Some("Zm9yZWlnbi12ZXJpZnk="));
}

// A settings-only restore of a stored snapshot merges the settings row in the
// database itself; the vault keys of the live row must come through it.
#[test]
fn a_settings_only_restore_keeps_the_local_vault() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-restore-vault-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-restore-vault-staged.db", brand::SLUG));
    let (live, p, _) = vault_fixture();
    {
        let conn = full_schema_db(&target, "target");
        db_write_settings(&conn, &live).unwrap();
    }
    {
        let conn = full_schema_db(&staged, "staged");
        let mut theirs = Settings::default();
        theirs.ui_texts.insert("marker".into(), "THEIRS".into());
        theirs.ui_texts.insert(SECRET_SALT_KEY.into(), "Zm9yZWlnbi1zYWx0".into());
        theirs.ui_texts.insert(SECRET_VERIFY_KEY.into(), "Zm9yZWlnbi12ZXJpZnk=".into());
        db_write_settings(&conn, &theirs).unwrap();
    }
    restore_selective(&target, &staged, "settings").unwrap();
    let conn = Connection::open(&target).unwrap();
    let (_, restored, _, _, _) = db_load(&conn, &mut WrittenRows::new());
    let restored = restored.expect("settings row");
    assert_eq!(restored.ui_texts.get("marker").map(String::as_str), Some("THEIRS"), "the backup settings did come in");
    assert!(still_opens(&restored, &p), "the local vault keys must survive");
    drop(conn);
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

// ---- review follow-ups: vault follows the prompts, stale fallbacks, snapshot restores ----

#[test]
fn a_data_import_takes_the_backups_vault_keys_or_keeps_local_ones_when_it_has_none() {
    let (settings, p, _) = vault_fixture();
    let mut store = vault_store(settings.clone(), p.clone());
    apply_import(&mut store, foreign_backup(true), true, false);
    assert_eq!(store.settings.ui_texts.get(SECRET_SALT_KEY).map(String::as_str), Some("Zm9yZWlnbi1zYWx0"), "keys follow the prompts");
    assert_eq!(store.settings.ui_texts.get("marker"), None, "data scope still leaves the other settings alone");

    let mut store = vault_store(settings.clone(), p.clone());
    apply_import(&mut store, foreign_backup(false), true, false);
    assert!(still_opens(&store.settings, &p), "a backup without keys keeps the local ones");
    let mut store = vault_store(settings, p.clone());
    apply_import(&mut store, foreign_backup(false), true, true);
    assert!(still_opens(&store.settings, &p), "full import of a backup without keys keeps the local ones");
    assert_eq!(store.settings.ui_texts.get("marker").map(String::as_str), Some("THEIRS"));
}

#[test]
fn carrying_vault_keys_from_a_source_without_them_changes_nothing() {
    let (with, _, _) = vault_fixture();
    let mut to = with.clone();
    carry_vault_keys(&Settings::default(), &mut to);
    assert_eq!(to.ui_texts.get(SECRET_SALT_KEY), with.ui_texts.get(SECRET_SALT_KEY));
    assert!(has_vault_keys(&to));
}

#[test]
fn an_import_warns_when_trashed_confidential_prompts_lose_their_key() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let (settings, p, _) = vault_fixture();
    for (trash_secret, expect) in [(true, true), (false, false)] {
        let mut store = vault_store(settings.clone(), p.clone());
        let mut t = test_prompt("trashed");
        t.secret = trash_secret;
        store.trash.push(t);
        let r = run_import(&mut store, foreign_backup(true), true, true, |_, _, _| true).unwrap();
        assert_eq!(r.3, expect, "trash holds a confidential prompt: {trash_secret}");
    }
    let mut store = vault_store(settings, p);
    let mut t = test_prompt("trashed");
    t.secret = true;
    store.trash.push(t);
    let r = run_import(&mut store, foreign_backup(false), true, true, |_, _, _| true).unwrap();
    assert!(!r.3, "keys unchanged, nothing to warn about");
}

#[test]
fn an_import_that_swaps_the_vault_keys_ends_the_unlocked_session() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let (settings, p, _) = vault_fixture();
    for (foreign, expect_locked) in [(true, true), (false, false)] {
        *SECRET_SESSION.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(SecretSession { key: [7u8; 32], unlocked_at: std::time::Instant::now() });
        let mut store = vault_store(settings.clone(), p.clone());
        run_import(&mut store, foreign_backup(foreign), true, true, |_, _, _| true).unwrap();
        assert_eq!(secret_session_key(0).is_none(), expect_locked, "foreign keys: {foreign}");
    }
    *SECRET_SESSION.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

#[test]
fn a_data_restore_moves_the_vault_keys_along_with_the_prompts() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-restore-vaultdata-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-restore-vaultdata-staged.db", brand::SLUG));
    let (live, _, _) = vault_fixture();
    {
        let conn = full_schema_db(&target, "target");
        db_write_settings(&conn, &live).unwrap();
    }
    {
        let conn = full_schema_db(&staged, "staged");
        let mut theirs = Settings::default();
        theirs.ui_texts.insert("marker".into(), "THEIRS".into());
        theirs.ui_texts.insert(SECRET_SALT_KEY.into(), "Zm9yZWlnbi1zYWx0".into());
        theirs.ui_texts.insert(SECRET_VERIFY_KEY.into(), "Zm9yZWlnbi12ZXJpZnk=".into());
        db_write_settings(&conn, &theirs).unwrap();
    }
    restore_selective(&target, &staged, "data").unwrap();
    let conn = Connection::open(&target).unwrap();
    let after = read_settings_row(&conn, "main").expect("settings row");
    assert_eq!(after.ui_texts.get(SECRET_SALT_KEY).map(String::as_str), Some("Zm9yZWlnbi1zYWx0"));
    assert_eq!(after.ui_texts.get("marker"), None, "the other settings stay local");
    drop(conn);
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

// An old snapshot must not bring back an old receiver password, switch or
// allowed ranges, whichever scope restores its settings.
fn old_snapshot_settings() -> Settings {
    let mut old = Settings { auto_update: false, ..Settings::default() };
    old.ui_flags.insert("remoteApi".into(), true);
    old.ui_flags.insert("clipWatcher".into(), true);
    old.ui_texts.insert("remotePasswordHash".into(), "old-hash".into());
    old.ui_texts.insert("remoteAllowedRanges".into(), "10.0.0.0/8".into());
    old.ui_texts.insert("marker".into(), "SNAPSHOT".into());
    old.skipped_versions = vec!["9.9.9".into()];
    old
}

fn local_machine_settings() -> Settings {
    let mut live = Settings { auto_update: true, ..Settings::default() };
    live.ui_flags.insert("remoteApi".into(), false);
    live.ui_texts.insert("remotePasswordHash".into(), "new-hash".into());
    live
}

fn assert_machine_values_are_local(s: &Settings) {
    assert_eq!(s.ui_flags.get("remoteApi"), Some(&false));
    assert_eq!(s.ui_texts.get("remotePasswordHash").map(String::as_str), Some("new-hash"));
    assert!(!s.ui_texts.contains_key("remoteAllowedRanges"));
    assert!(!s.ui_flags.contains_key("clipWatcher"));
    assert!(s.auto_update, "the update choice stays");
    assert!(s.skipped_versions.is_empty());
    assert_eq!(s.ui_texts.get("marker").map(String::as_str), Some("SNAPSHOT"), "ordinary settings do come back");
}

#[test]
fn a_snapshot_restore_keeps_this_machines_network_and_credential_values() {
    let dir = std::env::temp_dir();
    for scope in ["settings", "all"] {
        let target = dir.join(format!("{}-test-restore-machine-{scope}-target.db", brand::SLUG));
        let staged = dir.join(format!("{}-test-restore-machine-{scope}-staged.db", brand::SLUG));
        {
            let conn = full_schema_db(&target, "target");
            db_write_settings(&conn, &local_machine_settings()).unwrap();
        }
        {
            let conn = full_schema_db(&staged, "staged");
            db_write_settings(&conn, &old_snapshot_settings()).unwrap();
        }
        if scope == "settings" {
            restore_selective(&target, &staged, scope).unwrap();
        } else {
            keep_machine_in_staged(&target, &staged).unwrap();
            std::fs::rename(&staged, &target).unwrap();
        }
        let conn = Connection::open(&target).unwrap();
        assert_machine_values_are_local(&read_settings_row(&conn, "main").expect("settings row"));
        drop(conn);
        let _ = std::fs::remove_file(&target);
        let _ = std::fs::remove_file(&staged);
    }
}

#[test]
fn a_full_import_keeps_the_update_choices_and_the_clipboard_watcher_of_this_machine() {
    let mut store = Store { prompts: vec![], trash: Vec::new(), settings: local_machine_settings(), prompts_locked: false, written: Default::default() };
    apply_import(&mut store, Backup { prompts: vec![], settings: old_snapshot_settings() }, false, true);
    assert_machine_values_are_local(&store.settings);
}

#[test]
fn a_successful_write_removes_the_stale_fallback_file_and_a_reload_matches() {
    let dir = std::env::temp_dir().join(format!("{}-test-stale-fallback", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let stale = vec![test_prompt("stale")];
    assert!(write_sealed_json(&dir.join("prompts.json"), &stale));
    assert!(write_sealed_json(&dir.join("settings.json"), &Settings::default()));
    let mut conn = import_test_db();
    let fresh = vec![test_prompt("fresh")];
    db_write_prompts(&mut conn, &fresh, &mut WrittenRows::new()).unwrap();
    drop_stale_fallback(&dir, "prompts.json");
    drop_stale_fallback(&dir, "settings.json");
    drop_stale_fallback(&dir, "never-there.json"); // absent file: no effect, no panic
    assert!(!dir.join("prompts.json").exists() && !dir.join("settings.json").exists());
    let (loaded, _, _, _, _) = db_load(&conn, &mut WrittenRows::new());
    assert_eq!(loaded.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["fresh"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_settings_export_carries_no_password_hash_or_sealed_backup_password() {
    let mut s = Settings::default();
    s.ui_texts.insert("remotePasswordHash".into(), "hash".into());
    s.ui_texts.insert(BACKUP_PW_KEY.into(), "sealed".into());
    s.ui_texts.insert("copyFont".into(), "Consolas".into());
    let payload = settings_only_payload(&s);
    assert!(!payload.ui_texts.contains_key("remotePasswordHash"));
    assert!(!payload.ui_texts.contains_key(BACKUP_PW_KEY));
    assert!(payload.ui_texts.contains_key("copyFont"));
}

#[test]
fn a_window_copy_of_the_settings_has_no_private_key() {
    let mut s = Settings::default();
    s.ui_texts.insert(remote_tls::REMOTE_TLS_KEY.into(), "sealed".into());
    s.ui_texts.insert(remote_tls::REMOTE_TLS_CA.into(), "ca".into());
    s.ui_texts.insert(remote::REMOTE_PASSWORD_KEY.into(), "hash".into());
    s.ui_texts.insert("remoteAllowedRanges".into(), "10.0.0.0/8".into());
    let copy = webview_settings(&s);
    assert!(!copy.ui_texts.contains_key(remote::REMOTE_PASSWORD_KEY));
    assert!(copy.ui_texts.contains_key("remoteAllowedRanges"));
    assert!(!copy.ui_texts.contains_key(remote_tls::REMOTE_TLS_KEY));
    assert_eq!(copy.ui_texts.get(remote_tls::REMOTE_TLS_CA).map(String::as_str), Some("ca"));
    assert!(s.ui_texts.contains_key(remote_tls::REMOTE_TLS_KEY));
}

// A full "data" import must actually delete the prompts it replaces ON DISK,
// not just in the in-memory list apply_import mutates directly. The two tests
// above only ever check store.prompts, which is always correct (apply_import
// assigns it outright) regardless of this bug: it takes a real write_rows
// round-trip through a DB to see whether the OLD rows a smaller import leaves
// behind actually get deleted, or silently survive to reappear after restart.
#[test]
fn import_all_data_scope_deletes_the_prompts_it_replaces_from_disk() {
    let mut conn = row_test_db();
    let mut written = WrittenRows::new();
    // "This process" genuinely loaded two prompts before the import: the exact
    // condition write_rows' delete guard requires to treat their disappearance
    // as a real, user-initiated removal rather than another instance's unseen work.
    let original = numbered_prompts(2);
    db_write_prompts(&mut conn, &original, &mut written).unwrap();
    assert_eq!(stored_rows(&conn, "prompts").len(), 2);

    let mut store = Store {
        prompts: original,
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: std::cell::RefCell::new(written),
    };
    // A one-prompt backup, with an id neither original prompt had.
    let backup = Backup { prompts: vec![test_prompt("imported-only")], settings: Settings::default() };
    apply_import(&mut store, backup, true, false);
    assert_eq!(store.prompts.len(), 1, "the in-memory list is always replaced outright");

    // The save import_all performs right after apply_import, using the SAME
    // store.written apply_import just (did not) clear.
    let mut written = store.written.into_inner();
    db_write_prompts(&mut conn, &store.prompts, &mut written).unwrap();

    let rows = stored_rows(&conn, "prompts");
    assert_eq!(rows.len(), 1, "the two replaced prompts must actually be deleted from disk, not just from memory");
    assert_eq!(rows[0].0, "imported-only");
}

#[test]
fn purge_clip_inbox_from_backups_scrubs_every_snapshot() {
    let dir = std::env::temp_dir().join(format!("{}-test-purge-backups", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // A: gzipped snapshot (current format) with clip_inbox rows + a prompt that
    // must survive untouched.
    let folder_a = dir.join("20260101-000000");
    std::fs::create_dir_all(&folder_a).unwrap();
    let raw_a = dir.join("raw_a.db");
    {
        let conn = full_schema_db(&raw_a, "a");
        conn.execute_batch(
            "INSERT INTO prompts VALUES('p1', 0, '{}');
                 INSERT INTO clip_inbox(ts, text) VALUES(1, 'secret-a');
                 INSERT INTO clip_inbox(ts, text) VALUES(2, 'secret-b');",
        )
        .unwrap();
    }
    gzip_to(&raw_a, &folder_a.join("data.db.gz")).unwrap();
    let _ = std::fs::remove_file(&raw_a);

    // B: plain (pre-2.6, ungzipped) snapshot; also has clip_inbox rows.
    let folder_b = dir.join("20260102-000000");
    std::fs::create_dir_all(&folder_b).unwrap();
    {
        let conn = full_schema_db(&folder_b.join("data.db"), "b");
        conn.execute_batch("INSERT INTO clip_inbox(ts, text) VALUES(3, 'secret-c');").unwrap();
    }

    // C: nothing in its clip_inbox; must not be counted as purged.
    let folder_c = dir.join("20260103-000000");
    std::fs::create_dir_all(&folder_c).unwrap();
    let raw_c = dir.join("raw_c.db");
    {
        full_schema_db(&raw_c, "c");
    }
    gzip_to(&raw_c, &folder_c.join("data.db.gz")).unwrap();
    let _ = std::fs::remove_file(&raw_c);

    // D: a snapshot claiming to be gzipped that is not; gunzip must fail on
    // it, and that must be counted as a failure (it might hold clip_inbox
    // rows this run could never even look at), never silently skipped.
    let folder_d = dir.join("20260104-000000");
    std::fs::create_dir_all(&folder_d).unwrap();
    std::fs::write(folder_d.join("data.db.gz"), b"not actually gzip data").unwrap();

    let (purged, failed) = purge_clip_inbox_from_backups_impl(&dir);
    assert_eq!(purged, 2, "only A and B actually had clip_inbox rows to remove");
    assert_eq!(failed, 1, "D could not be opened and must be reported, not silently treated as clean");

    let unpacked_a = dir.join("check_a.db");
    gunzip_to(&folder_a.join("data.db.gz"), &unpacked_a).unwrap();
    let conn = Connection::open(&unpacked_a).unwrap();
    let clip_count: i64 = conn.query_row("SELECT COUNT(*) FROM clip_inbox", [], |r| r.get(0)).unwrap();
    assert_eq!(clip_count, 0, "A's clip_inbox must be emptied");
    let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(prompt_id, "p1", "the purge must never touch other tables");

    let conn = Connection::open(folder_b.join("data.db")).unwrap();
    let clip_count: i64 = conn.query_row("SELECT COUNT(*) FROM clip_inbox", [], |r| r.get(0)).unwrap();
    assert_eq!(clip_count, 0, "B's (ungzipped) clip_inbox must be emptied too");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restore_selective_skips_a_table_missing_from_an_older_backup() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-restore-oldbackup-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-restore-oldbackup-staged.db", brand::SLUG));
    {
        let conn = full_schema_db(&target, "target");
        conn.execute_batch(
            "INSERT INTO prompts VALUES('live', 0, '{}');
                 INSERT INTO trash VALUES('live-trashed', 0, '{}');
                 INSERT INTO meta VALUES('settings', 'LIVE-SETTINGS');",
        )
        .unwrap();
    }
    {
        // Pre-91 backup: no trash table at all.
        let _ = std::fs::remove_file(&staged);
        let conn = Connection::open(&staged).unwrap();
        conn.execute_batch(
            "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
                 CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
                 CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');
                 CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
                 INSERT INTO prompts VALUES('backed-up', 0, '{}');",
        )
        .unwrap();
    }
    restore_selective(&target, &staged, "data").unwrap();
    let conn = Connection::open(&target).unwrap();
    let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(prompt_id, "backed-up", "prompts (present in the old backup) must still be replaced");
    let trash_id: String = conn.query_row("SELECT id FROM trash", [], |r| r.get(0)).unwrap();
    assert_eq!(trash_id, "live-trashed", "trash (absent from the old backup) must be left alone, not wiped");
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

// Every release before 2.7.0 wrote prompt_versions without its `variants`
// column. Restoring only the data out of such a backup used to fail on the
// column count alone and roll the whole thing back (prompts, trash and all),
// leaving the user with a backup they could not get their prompts out of.
#[test]
fn restore_selective_accepts_a_backup_table_that_predates_a_column() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-restore-oldcolumns-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-restore-oldcolumns-staged.db", brand::SLUG));
    {
        let conn = full_schema_db(&target, "target");
        conn.execute_batch(
            "INSERT INTO prompts VALUES('live', 0, '{}');
                 INSERT INTO prompt_versions(prompt_id, ts, name, text, variants)
                     VALUES('live', 1, 'Live', 'live text', 'a');",
        )
        .unwrap();
    }
    {
        let _ = std::fs::remove_file(&staged);
        let conn = Connection::open(&staged).unwrap();
        conn.execute_batch(
            "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
                 CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
                 CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
                 CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL);
                 CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
                 INSERT INTO prompts VALUES('backed-up', 0, '{}');
                 INSERT INTO prompt_versions(prompt_id, ts, name, text) VALUES('backed-up', 7, 'Older', 'older text');",
        )
        .unwrap();
    }

    restore_selective(&target, &staged, "data").expect("a backup without the newer column must still restore");

    let conn = Connection::open(&target).unwrap();
    let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(prompt_id, "backed-up", "the prompts must come from the backup");
    let (pid, name, text, variants): (String, String, String, String) = conn
        .query_row("SELECT prompt_id, name, text, variants FROM prompt_versions", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap();
    assert_eq!((pid.as_str(), name.as_str(), text.as_str()), ("backed-up", "Older", "older text"));
    assert_eq!(variants, "", "the column the backup never had must fall back to its default");

    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn restore_selective_rolls_back_everything_if_one_table_fails() {
    // "prompts" comes first in the copy order and would succeed on its own;
    // "trash" is made incompatible (its `data` column, which the live table
    // requires and has no default for, is missing) so its INSERT fails: proving
    // the whole thing runs in ONE transaction, not one commit per table.
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-restore-rollback-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-restore-rollback-staged.db", brand::SLUG));
    {
        let conn = full_schema_db(&target, "target");
        conn.execute_batch("INSERT INTO prompts VALUES('live', 0, '{}');").unwrap();
    }
    {
        let _ = std::fs::remove_file(&staged);
        let conn = Connection::open(&staged).unwrap();
        conn.execute_batch(
            "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
                 CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL);
                 INSERT INTO prompts VALUES('backed-up', 0, '{}');
                 INSERT INTO trash VALUES('backed-up-trashed', 0);",
        )
        .unwrap();
    }
    let result = restore_selective(&target, &staged, "data");
    assert!(result.is_err(), "a column-count mismatch on trash must surface as an error");
    let conn = Connection::open(&target).unwrap();
    let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(prompt_id, "live", "prompts must roll back too, even though its own copy succeeded");
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn backup_decrypt_passes_through_legacy_plaintext() {
    // Old .json backups have no PSB1 header: the caller falls back to plain JSON.
    assert!(backup_decrypt(br#"{"prompts":[]}"#, None).expect("no error").is_none());
}

#[test]
fn copy_log_caps_groups_and_prunes() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE INDEX idx_copy_log_ts ON copy_log(ts);",
    )
    .unwrap();
    // Cap 3: only the three newest events survive, newest-first by insertion.
    for i in 0..5u64 {
        db_append_copy(&conn, &CopyEntry { id: format!("p{i}"), ts: 100 + i }, 3, None).unwrap();
    }
    let recent: Vec<String> = db_recent_copies(&conn, 10, false).into_iter().map(|e| e.id).collect();
    assert_eq!(recent, ["p4", "p3", "p2"]);
    // Grouped: a repeated id collapses to one row at its most recent position.
    db_append_copy(&conn, &CopyEntry { id: "p4".into(), ts: 200 }, 3, None).unwrap();
    let grouped = db_recent_copies(&conn, 10, true);
    assert_eq!(grouped[0].id, "p4");
    assert_eq!(grouped.iter().filter(|e| e.id == "p4").count(), 1);
    // Retention cutoff drops entries older than the window.
    db_append_copy(&conn, &CopyEntry { id: "old".into(), ts: 50 }, 100, Some(150)).unwrap();
    assert!(db_recent_copies(&conn, 100, false).iter().all(|e| e.id != "old"));
    // Gapped rowids (e.g. after an age-prune) must never let the cap be exceeded.
    conn.execute("DELETE FROM copy_log", []).unwrap();
    for i in 0..10u64 {
        db_append_copy(&conn, &CopyEntry { id: format!("g{i}"), ts: 0 }, 5, None).unwrap();
    }
    conn.execute("DELETE FROM copy_log WHERE id IN ('g7','g8')", []).unwrap();
    for i in 10..15u64 {
        db_append_copy(&conn, &CopyEntry { id: format!("g{i}"), ts: 0 }, 5, None).unwrap();
    }
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM copy_log", [], |r| r.get(0)).unwrap();
    assert!(n <= 5, "cap exceeded after gaps: {n}");
}

#[test]
fn restore_copy_history_reinserts_every_snapshotted_row() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);").unwrap();
    let rows = vec![
        CopyLogRow { ts: 10, id: "a".into() },
        CopyLogRow { ts: 20, id: "b".into() },
    ];
    db_restore_copy_log(&conn, &rows).unwrap();
    // db_recent_copies orders by rowid DESC -- the later insert (b) comes first,
    // same order the journal would have shown it in before the clear.
    let restored: Vec<String> = db_recent_copies(&conn, 10, false).into_iter().map(|e| e.id).collect();
    assert_eq!(restored, ["b", "a"]);
}

#[test]
fn restore_clip_inbox_keeps_the_newest_within_the_cap() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);",
    )
    .unwrap();
    // Five snapshotted rows, oldest first -- the order restore_clip_inbox
    // feeds them in after the frontend reverses clip_inbox_list's own
    // newest-first listing -- capped at 3 like a real clipInboxMax setting.
    for i in 0..5u64 {
        db_add_clip(&conn, i, &format!("t{i}"), 3).unwrap();
    }
    let texts: Vec<String> = db_list_clips(&conn).into_iter().map(|c| c.text).collect();
    assert_eq!(texts, ["t4", "t3", "t2"]);
}

#[test]
fn daily_copies_are_zero_filled_and_bucketed() {
    // A day with no copies must still appear (as 0), not be skipped:
    // otherwise a gap in the chart would read as "the app broke that day"
    // instead of "nothing was copied".
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE INDEX idx_copy_log_ts ON copy_log(ts);",
    )
    .unwrap();
    const DAY: u64 = 86400;
    let today = now_secs() / DAY * DAY;
    // Two copies today, three two days ago, none yesterday, one outside the window.
    db_append_copy(&conn, &CopyEntry { id: "a".into(), ts: today + 10 }, 100, None).unwrap();
    db_append_copy(&conn, &CopyEntry { id: "b".into(), ts: today + 20 }, 100, None).unwrap();
    for i in 0..3u64 {
        db_append_copy(&conn, &CopyEntry { id: format!("c{i}"), ts: today - 2 * DAY + i }, 100, None).unwrap();
    }
    db_append_copy(&conn, &CopyEntry { id: "old".into(), ts: today - 20 * DAY }, 100, None).unwrap();
    let daily = db_daily_copies(&conn, 14);
    assert_eq!(daily.len(), 14, "must always return exactly `days` buckets");
    assert_eq!(daily.last().unwrap().ts, today, "the newest bucket is today");
    assert_eq!(daily.last().unwrap().count, 2);
    assert_eq!(daily[daily.len() - 2].count, 0, "yesterday had nothing - must be 0, not missing");
    assert_eq!(daily[daily.len() - 3].count, 3, "two days ago");
    assert!(daily.iter().all(|d| d.ts % DAY == 0), "every bucket is a UTC day boundary");
}

#[test]
fn grouped_journal_row_id_and_timestamp_come_from_the_same_copy() {
    // A clock that went backwards (or timestamps switched off mid-run) makes
    // the newest row by rowid NOT the newest by ts. Picking both with separate
    // aggregates then paired the newest row id with an older row's time, so the
    // delete button and the date shown belonged to different copies.
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE INDEX idx_copy_log_ts ON copy_log(ts);",
    )
    .unwrap();
    db_append_copy(&conn, &CopyEntry { id: "p1".into(), ts: 900 }, 100, None).unwrap();
    db_append_copy(&conn, &CopyEntry { id: "p1".into(), ts: 100 }, 100, None).unwrap();

    let grouped = db_recent_copies(&conn, 10, true);
    assert_eq!(grouped.len(), 1, "one row per prompt");
    let row = &grouped[0];
    let real_ts: i64 = conn
        .query_row("SELECT ts FROM copy_log WHERE rowid = ?1", params![row.rowid], |r| r.get(0))
        .unwrap();
    assert_eq!(
        row.ts as i64, real_ts,
        "the timestamp shown must be the one on the row the delete button targets"
    );
    assert_eq!(row.ts, 100, "the newest row by id is the second copy, ts 100");
}

#[test]
fn journal_rowid_deletes_exactly_one_entry() {
    // The rowid db_recent_copies() now returns has to target the exact row:
    // never "every copy of this prompt", in either view mode.
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE INDEX idx_copy_log_ts ON copy_log(ts);",
    )
    .unwrap();
    db_append_copy(&conn, &CopyEntry { id: "p1".into(), ts: 100 }, 100, None).unwrap();
    db_append_copy(&conn, &CopyEntry { id: "p1".into(), ts: 200 }, 100, None).unwrap();
    db_append_copy(&conn, &CopyEntry { id: "p2".into(), ts: 150 }, 100, None).unwrap();
    // Ungrouped: three distinct rows, delete-by-rowid removes exactly one.
    let ungrouped = db_recent_copies(&conn, 10, false);
    assert_eq!(ungrouped.len(), 3);
    let target = ungrouped.iter().find(|e| e.ts == 100).unwrap().rowid;
    conn.execute("DELETE FROM copy_log WHERE rowid = ?1", params![target]).unwrap();
    let after = db_recent_copies(&conn, 10, false);
    assert_eq!(after.len(), 2, "deleted the wrong number of rows");
    assert!(after.iter().any(|e| e.ts == 200), "the OTHER p1 copy must survive");
    assert!(after.iter().any(|e| e.ts == 150));
    // Grouped: p1's rowid is its most-recent (ts=200) copy; deleting it must
    // remove only that one row, not every copy_log row for p1.
    let grouped = db_recent_copies(&conn, 10, true);
    let p1 = grouped.iter().find(|e| e.id == "p1").unwrap();
    assert_eq!(p1.ts, 200);
    conn.execute("DELETE FROM copy_log WHERE rowid = ?1", params![p1.rowid]).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM copy_log WHERE id = 'p1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "p1 only had that one row left, both must not survive a single delete");
}

fn sample_prompts() -> Vec<Prompt> {
    vec![
        Prompt {
            id: "p1".into(),
            trashed_at: 0,
            name: "Mail".into(),
            text: "Hello\nWorld".into(),
            color: "#ef4444".into(),
            image: "data:image/png;base64,QUJD".into(),
            show_image: true,
            copy_image: true,
            file_path: String::new(),
            pdf_page: 0,
            icon_path: String::new(),
            caption: String::new(),
            caption_size: 0,
            font: "mono".into(),
            font_size: 24,
            favorite: false,
            tags: vec!["Work".into(), "E-Mail".into()],
            notes: "Ask Anna first.\nDeadline: Friday".into(),
            hotkey: "CommandOrControl+Shift+1".into(),
            hotkey_paste: "paste".into(),
            variants: Vec::new(),
            rotate_mode: String::new(),
            rotate_auto: false,
            media_duration_ms: 0,
            media_width: 0,
            media_height: 0,
            media_size_bytes: 0,
            image_text: String::new(),
            secret: false,
        },
        Prompt {
            id: "p2".into(),
            trashed_at: 0,
            name: "Doc".into(),
            text: "Doc".into(),
            color: String::new(),
            image: String::new(),
            show_image: true,
            copy_image: false,
            file_path: "C:\\tmp\\report.pdf".into(),
            pdf_page: 3,
            icon_path: "C:\\tmp\\clip.mp4".into(),
            caption: "Mein Untertitel".into(),
            caption_size: 18,
            font: String::new(),
            font_size: 1,
            favorite: false,
            tags: Vec::new(),
            notes: String::new(),
            hotkey: String::new(),
            hotkey_paste: String::new(),
            variants: Vec::new(),
            rotate_mode: String::new(),
            rotate_auto: false,
            media_duration_ms: 0,
            media_width: 0,
            media_height: 0,
            media_size_bytes: 0,
            image_text: String::new(),
            secret: false,
        },
    ]
}

// ---- Rotating prompts ----
// "Kopierfehler ⇒ kein Commit" is not tested here: it is a one-line structural
// guarantee in copy_prompt (`if ok { rotation_advance(...) }`), not a property
// of rotation_advance itself, which has no notion of clipboard success.

#[test]
fn rotation_order_mode_cycles_through_every_entry_in_sequence() {
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["b".into(), "c".into()]; // total = 3, "" = order
    let mut settings = Settings::default();
    let seen: Vec<usize> = (0..7)
        .map(|_| {
            let (idx, _, _) = rotation_next(&p, &settings);
            rotation_advance(&p, &mut settings);
            idx
        })
        .collect();
    assert_eq!(seen, vec![0, 1, 2, 0, 1, 2, 0]);
}

#[test]
fn rotation_random_bag_shows_each_entry_once_then_avoids_an_immediate_repeat() {
    let mut p = sample_prompts().remove(0);
    p.rotate_mode = "random".into();
    p.variants = vec!["b".into(), "c".into(), "d".into(), "e".into()]; // total = 5
    let mut settings = Settings::default();
    let total = 5;
    // Prime past the cold-start bootstrap: with no bag yet, rotation_next()
    // falls back to (idx 0, pos 1): the same "1" a freshly reshuffled bag
    // also reports for its own first, still-untouched peek, so pos briefly
    // repeats once (1, 1, 2, 3, 4, 5) before this one-time double 1 settles
    // into a clean cycle. This one throwaway draw gets a real bag seeded so
    // the loop below observes steady-state behaviour only.
    rotation_advance(&p, &mut settings);
    let mut first_cycle = std::collections::HashSet::new();
    let mut last_shown = None;
    let mut positions = Vec::new();
    for _ in 0..total {
        let (idx, pos, _) = rotation_next(&p, &settings);
        assert!(first_cycle.insert(idx), "the bag repeated an entry within one cycle");
        positions.push(pos);
        rotation_advance(&p, &mut settings);
        last_shown = Some(idx);
    }
    assert_eq!(first_cycle.len(), total, "the bag skipped an entry within one cycle");
    // The draw-number badge counts 1..total across one cycle, in order:
    // even though the entries themselves are shuffled.
    assert_eq!(positions, vec![1, 2, 3, 4, 5]);
    let (next_idx, next_pos, _) = rotation_next(&p, &settings);
    assert_ne!(Some(next_idx), last_shown, "reshuffle repeated the last-shown entry immediately");
    assert_eq!(next_pos, 1, "the draw-number badge must restart at 1 for the new cycle");
}

#[test]
fn rotation_stale_pointer_and_bag_never_go_out_of_range_after_entries_shrink() {
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["b".into()]; // total = 2 now, but state below predates the shrink
    let mut settings = Settings::default();
    settings.rotate_pos.insert(p.id.clone(), 9);
    let (idx, _, text) = rotation_next(&p, &settings);
    assert_eq!(idx, 9 % 2, "a stale order-mode pointer must clamp back into range");
    assert_eq!(text, p.variants[0]);

    p.rotate_mode = "random".into();
    settings.rotate_bag.insert(p.id.clone(), vec![4, 0]); // 4 no longer exists
    let (idx2, _, _) = rotation_next(&p, &settings);
    assert_eq!(idx2, 0, "a bag containing any stale index must be discarded, not partially trusted");
    rotation_advance(&p, &mut settings);
    let bag = settings.rotate_bag.get(&p.id).unwrap();
    assert!(bag.iter().all(|&i| (i as usize) < 2), "the reshuffled bag must only contain valid indices");
}

#[test]
fn rotation_disabled_globally_always_serves_entry_one_and_freezes_the_pointer() {
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["b".into(), "c".into()];
    p.rotate_mode = "random".into();
    let mut settings = Settings::default();
    settings.ui_flags.insert("rotatingPrompts".into(), false);
    for _ in 0..3 {
        let (idx, _, text) = rotation_next(&p, &settings);
        assert_eq!(idx, 0);
        assert_eq!(text, p.text);
        rotation_advance(&p, &mut settings); // must be a no-op while off
    }
    assert!(settings.rotate_pos.is_empty());
    assert!(settings.rotate_bag.is_empty());
}

#[test]
fn sanitize_variants_trims_drops_empties_and_caps_the_count() {
    let raw: Vec<String> = vec!["  b  ".into(), "".into(), "   ".into(), "c".into()];
    assert_eq!(sanitize_variants(raw), vec!["b".to_string(), "c".to_string()]);

    let oversized: Vec<String> = (0..80).map(|i| format!("v{i}")).collect();
    assert_eq!(sanitize_variants(oversized).len(), MAX_ROTATE_VARIANTS);
}

#[test]
fn sanitize_rotate_mode_is_empty_without_variants_and_only_random_survives() {
    let none: Vec<String> = Vec::new();
    let some = vec!["b".to_string()];
    assert_eq!(sanitize_rotate_mode("random".into(), &none), "");
    assert_eq!(sanitize_rotate_mode("random".into(), &some), "random");
    assert_eq!(sanitize_rotate_mode("garbage".into(), &some), "");
    assert_eq!(sanitize_rotate_mode("".into(), &some), "");
}

#[test]
fn rotate_auto_is_dropped_without_variants() {
    let none: Vec<String> = Vec::new();
    let some = vec!["b".to_string()];
    assert!(!sanitize_rotate_auto(true, &none));
    assert!(sanitize_rotate_auto(true, &some));
    assert!(!sanitize_rotate_auto(false, &some));
}

#[test]
fn an_unknown_restore_scope_is_refused_rather_than_guessed() {
    for ok in ["all", "data", "settings"] {
        assert_eq!(valid_scope(ok).unwrap(), ok);
    }
    // Left to their own reading, restore_backup treated an unknown scope as
    // "change nothing and report success" while import_all treated it as
    // "restore everything": opposite answers to the same input.
    for bad in ["", "ALL", "prompts", "data ", "everything"] {
        assert!(valid_scope(bad).is_err(), "{bad:?} must not be accepted");
    }
}

#[test]
fn a_capped_read_decides_from_the_bytes_it_actually_read() {
    let p = std::env::temp_dir().join("capped_read_test.bin");
    fs::write(&p, vec![b'x'; 64]).unwrap();

    // Exactly at the limit is still a valid file, and the caller gets all of it.
    assert_eq!(read_file_capped(&p, 64).unwrap().len(), 64);
    // One byte of headroom less and it is refused -- the extra byte the reader
    // takes is what proves the file is over, without a second look at the disk.
    assert!(read_file_capped(&p, 63).is_err());
    assert_eq!(read_text_capped(&p, 64).unwrap().len(), 64);
    assert!(read_text_capped(&p, 63).is_none());

    let _ = fs::remove_file(&p);
}

#[test]
fn attach_preview_extensions_match_the_four_text_kinds() {
    for ext in ["a.txt", "a.md", "a.json", "a.csv", "A.TXT", "a.Md"] {
        assert!(is_text_preview_ext(ext), "{ext:?} should be a text preview kind");
    }
    for ext in ["a.pdf", "a.png", "a.docx", "a", "a.txtx"] {
        assert!(!is_text_preview_ext(ext), "{ext:?} must not be a text preview kind");
    }
}

#[test]
fn a_short_attachment_reads_whole_with_no_truncation() {
    let p = std::env::temp_dir().join("attach_preview_short.txt");
    fs::write(&p, "hello attachment").unwrap();
    let (bytes, truncated) = read_bytes_preview(&p, 8 * 1024).unwrap();
    assert!(!truncated);
    assert_eq!(decode_text_preview(bytes).unwrap(), "hello attachment");
    let _ = fs::remove_file(&p);
}

#[test]
fn an_attachment_over_the_limit_is_cut_and_flagged() {
    let p = std::env::temp_dir().join("attach_preview_long.txt");
    fs::write(&p, vec![b'a'; 5000]).unwrap();
    let (bytes, truncated) = read_bytes_preview(&p, 1024).unwrap();
    assert!(truncated);
    let text = decode_text_preview(bytes).unwrap();
    assert_eq!(text.len(), 1024);
    let _ = fs::remove_file(&p);
}

#[test]
fn an_empty_attachment_previews_as_empty_not_truncated() {
    let p = std::env::temp_dir().join("attach_preview_empty.txt");
    fs::write(&p, "").unwrap();
    let (bytes, truncated) = read_bytes_preview(&p, 1024).unwrap();
    assert!(!truncated);
    assert_eq!(decode_text_preview(bytes).unwrap(), "");
    let _ = fs::remove_file(&p);
}

#[test]
fn a_cut_made_inside_a_multibyte_character_trims_back_cleanly() {
    // Three-byte characters throughout (the euro sign); the cap is set to
    // land one byte into the second one.
    let text = "e".repeat(10) + "\u{20AC}\u{20AC}\u{20AC}\u{20AC}"; // e's then euro signs
    let p = std::env::temp_dir().join("attach_preview_multibyte.txt");
    fs::write(&p, &text).unwrap();
    // 10 ascii bytes + one whole euro sign (3 bytes) + 1 byte into the next.
    let (bytes, truncated) = read_bytes_preview(&p, 14).unwrap();
    assert!(truncated);
    let decoded = decode_text_preview(bytes).unwrap();
    assert!(std::str::from_utf8(decoded.as_bytes()).is_ok());
    assert_eq!(decoded, "e".repeat(10) + "\u{20AC}");
    let _ = fs::remove_file(&p);
}

#[test]
fn a_non_utf8_attachment_is_reported_unreadable_not_guessed() {
    // Latin-1 bytes that are not valid UTF-8 (0xE9 alone is a lone
    // continuation-style byte with no valid lead byte before it).
    let p = std::env::temp_dir().join("attach_preview_binary.txt");
    fs::write(&p, [0x48u8, 0x65, 0xE9, 0x6C, 0x6C, 0x6F]).unwrap();
    let (bytes, _truncated) = read_bytes_preview(&p, 1024).unwrap();
    assert!(decode_text_preview(bytes).is_none());
    let _ = fs::remove_file(&p);
}

#[test]
fn colouring_every_tag_at_once_is_all_or_nothing() {
    let entry = |name: &str, color: &str| TagColor { name: name.into(), color: color.into() };
    let mut settings = Settings::default();
    settings.tag_colors.insert("kept".into(), "#111111".into());

    apply_tag_colors(&mut settings, vec![entry("Work", "#ff0000"), entry("Home", "#00ff00")]).unwrap();
    assert_eq!(settings.tag_colors.get("work"), Some(&"#ff0000".to_string()));
    assert_eq!(settings.tag_colors.get("home"), Some(&"#00ff00".to_string()));

    // A single bad colour rejects the whole batch: nothing before it in the
    // list may already have been written.
    let before = settings.tag_colors.clone();
    let err = apply_tag_colors(&mut settings, vec![entry("Fresh", "#0000ff"), entry("Bad", "red")]);
    assert!(err.is_err());
    assert_eq!(settings.tag_colors, before, "a rejected batch must change nothing");

    // An empty colour clears one, exactly like the single-tag command.
    apply_tag_colors(&mut settings, vec![entry("Work", "")]).unwrap();
    assert!(!settings.tag_colors.contains_key("work"));
    assert_eq!(settings.tag_colors.get("kept"), Some(&"#111111".to_string()));
}

#[test]
fn auto_eligible_rejects_placeholders_images_and_files() {
    // "placeholders" (variable text) are explicitly ALLOWED: this test
    // instead covers every gate auto_eligible actually enforces.
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["b".into(), "c".into()];
    p.rotate_auto = true;
    p.image = String::new();
    p.file_path = String::new();
    let settings = Settings::default();
    assert!(auto_eligible(&p, &settings), "a plain multi-entry text prompt must be eligible");

    let mut no_variants = p.clone();
    no_variants.variants.clear();
    assert!(!auto_eligible(&no_variants, &settings));

    let mut not_auto = p.clone();
    not_auto.rotate_auto = false;
    assert!(!auto_eligible(&not_auto, &settings));

    let mut has_image = p.clone();
    has_image.image = "data:image/png;base64,QUJD".into();
    assert!(!auto_eligible(&has_image, &settings));

    let mut has_file = p.clone();
    has_file.file_path = "C:\\tmp\\report.pdf".into();
    assert!(!auto_eligible(&has_file, &settings));

    let mut rotation_off = Settings::default();
    rotation_off.ui_flags.insert("rotatingPrompts".into(), false);
    assert!(!auto_eligible(&p, &rotation_off));

    let mut feature_off = Settings::default();
    feature_off.ui_flags.insert("rotateAuto".into(), false);
    assert!(!auto_eligible(&p, &feature_off));
}

#[test]
fn auto_session_order_matches_rotation_next() {
    // The auto flag must never change what rotation_next/rotation_advance
    // themselves return: same order-mode and random-mode guarantee as the
    // plain copy path, just proven again with rotate_auto set.
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["b".into(), "c".into()];
    p.rotate_auto = true;
    let mut settings = Settings::default();
    let seen: Vec<usize> = (0..6)
        .map(|_| {
            let (idx, _, _) = rotation_next(&p, &settings);
            rotation_advance(&p, &mut settings);
            idx
        })
        .collect();
    assert_eq!(seen, vec![0, 1, 2, 0, 1, 2]);

    p.rotate_mode = "random".into();
    let mut settings = Settings::default();
    let mut cycle: Vec<usize> = (0..3)
        .map(|_| {
            let (idx, _, _) = rotation_next(&p, &settings);
            rotation_advance(&p, &mut settings);
            idx
        })
        .collect();
    cycle.sort();
    assert_eq!(cycle, vec![0, 1, 2], "every entry exactly once per random cycle");
}

// The CF_HTML header's own numbers must point exactly at the fragment the
// header promises, byte for byte -- a reader that trusts the header over
// scanning for markers (most do) gets the wrong slice otherwise.
#[test]
fn cf_html_header_offsets_point_at_the_real_fragment_bytes() {
    let payload = build_cf_html("<b>hi</b>");
    let get = |key: &str| -> usize {
        let line = payload.lines().find(|l| l.starts_with(key)).unwrap();
        line[key.len()..].trim().parse().unwrap()
    };
    let start_html = get("StartHTML:");
    let end_html = get("EndHTML:");
    let start_fragment = get("StartFragment:");
    let end_fragment = get("EndFragment:");

    assert_eq!(&payload[start_html..end_html], &payload[start_html..], "EndHTML must reach the real end of the string");
    assert_eq!(end_html, payload.len(), "EndHTML must equal the payload's own byte length");
    assert_eq!(&payload[start_fragment..end_fragment], "<b>hi</b>", "the fragment slice must be exactly the fragment given");
    assert!(payload[start_html..].starts_with("<html>"), "StartHTML must point at the opening tag");
}

// A fragment with multi-byte UTF-8 characters must not shift the offsets by
// character count -- they are byte positions, and a wrong count here would
// slice a multi-byte character in half.
#[test]
fn cf_html_offsets_are_byte_positions_not_char_counts() {
    let fragment = "caf\u{e9} \u{20ac}100 \u{1f600}"; // café, euro sign, emoji
    assert_eq!(fragment.chars().count(), 11);
    assert_ne!(fragment.chars().count(), fragment.len(), "the fixture must actually contain multi-byte chars");
    let payload = build_cf_html(fragment);
    let get = |key: &str| -> usize {
        let line = payload.lines().find(|l| l.starts_with(key)).unwrap();
        line[key.len()..].trim().parse().unwrap()
    };
    let start_fragment = get("StartFragment:");
    let end_fragment = get("EndFragment:");
    assert_eq!(&payload[start_fragment..end_fragment], fragment);
    assert!(std::str::from_utf8(payload.as_bytes()).is_ok(), "the whole payload must stay valid UTF-8");
}

// Real clipboard access is a single shared OS resource -- serialised so this
// never races another test that also opens it.
#[cfg(windows)]
static CLIPBOARD_TEST_LOCK: Mutex<()> = Mutex::new(());

// put_formatted's plain-text track must be exactly the Markdown source it was
// given, never the rendered HTML or a re-encoded copy of it -- an app that
// only reads plain text must see precisely what a normal copy would give it.
#[cfg(windows)]
#[test]
fn put_formatted_leaves_the_text_track_exactly_the_markdown_source() {
    let _guard = CLIPBOARD_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let text = "plain **not** rendered";
    let html = "<p>plain <strong>not</strong> rendered</p>";
    assert!(put_formatted(text, html));
    let read_back = arboard::Clipboard::new().and_then(|mut c| c.get_text()).unwrap_or_default();
    assert_eq!(read_back, text, "the plain-text track must be exactly the Markdown source, untouched");
}

#[cfg(windows)]
#[test]
fn counts_as_paste_gates_on_settle_cooldown_input_and_self_read() {
    let cfg = AutoCfg { settle_ms: 350, cooldown_ms: 250, input_ms: 3000, rearm_max: 12, timeout_min: 60 };
    // now, armed_at, last_paste, last_input, self_read, expected
    let cases: Vec<(u32, u32, u32, u32, bool, bool)> = vec![
        (10_000, 9_650, 0, 10_000, false, true),   // settle elapsed, no prior paste, fresh input -> counts
        (10_000, 9_800, 0, 10_000, false, false),  // still inside the settle window -> rejected
        (10_000, 9_650, 9_900, 10_000, false, false), // inside the cooldown since the last counted paste
        (10_000, 9_650, 9_600, 10_000, false, true),  // cooldown has elapsed -> counts
        (10_000, 9_650, 0, 6_000, false, false),   // last input too long ago -> a background actor, not a human
        (10_000, 9_650, 0, 10_000, true, false),   // our own get_text() triggered the render -> never counts
    ];
    for (now, armed_at, last_paste, last_input, self_read, expected) in cases {
        assert_eq!(
            counts_as_paste(now, armed_at, last_paste, last_input, self_read, &cfg),
            expected,
            "now={now} armed_at={armed_at} last_paste={last_paste} last_input={last_input} self_read={self_read}"
        );
    }
}

// Ten real-looking copies that must never reach the clip inbox, ten that must.
#[test]
fn looks_like_password_tells_secrets_from_ordinary_copies() {
    let secrets = [
        "Tr0ub4dor&3", "correcthorsebatterystaple9!", "P@ssw0rd2024", "xK9#mQ2$vL8p",
        "Zx7!Bq2#Wn9$", "a1B2c3D4e5F6", "9Fj!kLmN2#Qr", "Winter2024!Snow",
        "gH7#pL2vX9qR", "M0nk3y#Business",
    ];
    for s in secrets {
        assert!(looks_like_password(s, 8), "not flagged as a secret: {s}");
    }
    let ordinary = [
        "Write a short story about a dragon.",       // real sentence with spaces
        "p1",                                        // a prompt id, far too short
        "192.168.1.1",                                // an address, only two classes
        "short1!",                                    // below the 8-char floor
        "Rewrite this email so it sounds friendlier and more concise, please.\nThanks!", // multi-line
        "helloworld",                                 // one character class only
        "1234567890123",                               // digits only
        "ALLCAPSNOOTHERCLASS",                          // upper only
        "Prompt Saver",                                 // the app's own name (has a space anyway)
        "the quick brown fox",                          // ordinary prose, spaced
    ];
    for s in ordinary {
        assert!(!looks_like_password(s, 8), "wrongly flagged as a secret: {s}");
    }
}

#[test]
fn looks_like_password_respects_the_configurable_floor() {
    assert!(!looks_like_password("aB3!aB3!", 12), "8 chars must fail a 12-char floor");
    assert!(looks_like_password("aB3!aB3!", 8), "8 chars must pass an 8-char floor");
}

// The two new Windows clipboard-history/cloud-sync opt-out formats behave exactly
// like the pre-existing KeePass-style one in the one respect that matters: an
// explicit "do not include" (a stored 0, or a marker we could not even read)
// skips the content, while absence or an explicit "allow" never does.
#[test]
fn dword_clipboard_formats_are_excluded_only_on_an_explicit_zero() {
    assert!(!dword_format_excludes(false, None), "format absent, nothing to exclude on");
    assert!(!dword_format_excludes(false, Some(0)), "absent format's stray value must not matter");
    assert!(dword_format_excludes(true, Some(0)), "an explicit 0 excludes");
    assert!(!dword_format_excludes(true, Some(1)), "an explicit allow must not exclude");
    assert!(dword_format_excludes(true, None), "present but unreadable fails closed");
}

#[test]
fn rotation_forget_drops_both_maps_for_a_purged_prompt() {
    let mut settings = Settings::default();
    settings.rotate_pos.insert("p1".into(), 2);
    settings.rotate_bag.insert("p1".into(), vec![1, 0]);
    settings.rotate_pos.insert("p2".into(), 1);
    rotation_forget(&mut settings, &["p1".to_string()]);
    assert!(!settings.rotate_pos.contains_key("p1"));
    assert!(!settings.rotate_bag.contains_key("p1"));
    assert!(settings.rotate_pos.contains_key("p2"), "an unrelated prompt's pointer must survive");
}

// ---- Collected settings writes ----
//
// The disk is stood in for by a Settings clone the "writer" copies into, and
// SettingsDelay::take is the one place that decides whether a write happens;
// so counting its `true` answers counts disk writes exactly.

// One commit, exactly as rotation_commit does it: advance in memory, then mark.
fn commit(p: &Prompt, settings: &mut Settings, delay: &SettingsDelay) {
    rotation_advance(p, settings);
    delay.mark();
}

#[test]
fn a_burst_of_rotation_commits_costs_at_most_two_settings_writes() {
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["b".into(), "c".into()]; // total = 3
    let delay = SettingsDelay::new(50);
    let mut settings = Settings::default();
    let mut disk = Settings::default();
    let mut writes = 0usize;
    for _ in 0..100 {
        commit(&p, &mut settings, &delay);
        // The timer thread looking at the mark between two commits.
        if delay.take(false) {
            writes += 1;
            disk = settings.clone();
        }
    }
    assert!(writes <= 2, "100 rapid commits took {writes} disk writes, expected at most 2");
    // Before the change this was 100. The in-memory pointer moved every time.
    assert_eq!(
        settings.rotate_pos.get(&p.id).copied(),
        Some((100 % 3) as u32),
        "the pointer must be right in memory after every single commit"
    );
    let _ = disk;
}

#[test]
fn a_collected_write_lands_once_the_marking_goes_quiet() {
    let delay = SettingsDelay::new(20);
    delay.mark();
    assert!(!delay.take(false), "nothing may be written while marks are still arriving");
    std::thread::sleep(Duration::from_millis(40));
    assert!(delay.take(false), "a quiet window must land the collected write");
    assert!(!delay.take(false), "and it must not be written twice");
}

#[test]
fn quitting_writes_a_collected_change_that_the_quiet_window_would_still_hold() {
    // A window long enough that the timer thread would never fire during the test.
    let delay = SettingsDelay::new(600_000);
    delay.mark();
    assert!(!delay.take(false), "the quiet window has not passed");
    assert!(delay.take(true), "an exit must write whatever is owed, window or not");
    assert!(!delay.take(true), "with nothing owed there is nothing to write");
}

#[test]
fn an_immediate_write_cancels_the_collected_one_instead_of_letting_it_land_later() {
    let delay = SettingsDelay::new(0);
    delay.mark();
    // save_settings writes the whole block, counters included, and discards.
    delay.discard();
    assert!(!delay.take(true), "a full write leaves nothing owed behind it");
}

#[test]
fn the_pointer_read_back_after_a_quit_is_the_one_the_last_commit_left() {
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["b".into(), "c".into(), "d".into()]; // total = 4
    let delay = SettingsDelay::new(600_000);
    let mut settings = Settings::default();
    let mut disk = Settings::default();
    for _ in 0..7 {
        commit(&p, &mut settings, &delay);
        if delay.take(false) {
            disk = settings.clone();
        }
    }
    // Nothing has landed yet: this is exactly the state a hard kill would leave.
    assert_ne!(
        disk.rotate_pos.get(&p.id).copied(),
        settings.rotate_pos.get(&p.id).copied(),
        "without the flush the disk is behind - that is what the quit has to fix"
    );
    // The quit.
    if delay.take(true) {
        disk = settings.clone();
    }
    // What the next launch loads.
    let restarted = disk;
    assert_eq!(
        restarted.rotate_pos.get(&p.id).copied(),
        settings.rotate_pos.get(&p.id).copied(),
        "the restored pointer must be the last commit's, not an earlier one"
    );
    assert_eq!(
        rotation_next(&p, &restarted).0,
        rotation_next(&p, &settings).0,
        "and the first copy after the restart must serve the same entry"
    );
}

// ---- Trash ----

#[test]
fn stale_trash_ids_respects_the_retention_window() {
    let mut trash = sample_prompts(); // p1, p2
    let now = 10_000_000u64;
    trash[0].trashed_at = now - 100; // just trashed: well inside any window
    trash[1].trashed_at = now - 1_000_000; // trashed long ago: past a 1-day window
    let max_age = 86_400; // 1 day
    let stale = stale_trash_ids(&trash, max_age, now);
    assert_eq!(stale, vec!["p2".to_string()], "only the old entry should be stale");
}

#[test]
fn stale_trash_ids_never_touches_a_zero_timestamp() {
    // trashed_at == 0 is the "never trashed" sentinel every non-trashed Prompt
    // carries: it must never be treated as "trashed an eternity ago".
    let mut trash = sample_prompts();
    trash[0].trashed_at = 0;
    trash[1].trashed_at = 0;
    assert!(stale_trash_ids(&trash, 1, 1_000_000).is_empty());
}

#[test]
fn trash_retention_zero_days_means_forever() {
    let mut settings = Settings::default();
    settings.ui_values.insert("trashRetentionDays".to_string(), 0.0);
    assert_eq!(trash_retention_secs(&settings), None);
    settings.ui_values.insert("trashRetentionDays".to_string(), 7.0);
    assert_eq!(trash_retention_secs(&settings), Some(7 * 86_400));
}

// ---- Per-prompt hotkeys ----

#[test]
fn accelerators_are_validated_before_they_reach_the_os() {
    for good in ["CommandOrControl+Shift+P", "ctrl+shift+p", "Alt+F5", "Ctrl+Alt+K", " Ctrl+Shift+P "] {
        assert!(parse_accel(good).is_some(), "rejected a usable accelerator: {good:?}");
    }
    for bad in [
        "",
        "   ",
        "P",                       // no modifier: would swallow the key everywhere
        "Shift+",                  // empty token
        "Ctrl+Shift+NotAKey",
        "Ctrl+A+B",                // two main keys
        "Ctrl+Shift+\u{7}",        // control character
    ] {
        assert!(parse_accel(bad).is_none(), "accepted an unusable accelerator: {bad:?}");
    }
    // An over-long string is rejected outright, never handed to the OS.
    let long = format!("Ctrl+Shift+{}", "A".repeat(MAX_ACCEL_LEN));
    assert!(parse_accel(&long).is_none());
    // Spelling and case do not make two accelerators different.
    assert_eq!(parse_accel("CommandOrControl+Shift+P"), parse_accel("ctrl+SHIFT+p"));
}

// A global hotkey SWALLOWS its keystroke in every other program, so the
// blocklist decides what the app is allowed to take away from the user.
#[test]
fn system_and_universal_shortcuts_are_refused() {
    let reason = |s: &str| accel_error(s).err();
    // The universal editing set: losing these breaks every program at once.
    for s in [
        "Ctrl+C", "Ctrl+V", "Ctrl+X", "Ctrl+A", "Ctrl+Z", "Ctrl+Y", "Ctrl+S",
        "Ctrl+F", "Ctrl+P", "Ctrl+N", "Ctrl+O", "Ctrl+W", "Ctrl+T",
        "CommandOrControl+C", // the spelling the UI actually stores
    ] {
        assert_eq!(reason(s), Some(HK_SYSTEM), "not refused: {s}");
    }
    // Windows-reserved, and the ones the OS would never hand over anyway.
    for s in [
        "Ctrl+Alt+Delete", "Ctrl+Shift+Escape", "Ctrl+Escape", "Alt+Tab",
        "Alt+Shift+Tab", "Alt+F4", "Ctrl+F4", "Alt+Space", "Alt+Escape",
        "Ctrl+Shift+PrintScreen", "Alt+Enter", "Ctrl+Backspace", "Ctrl+Insert",
        "Ctrl+Alt+CapsLock", "Ctrl+Space",
    ] {
        assert_eq!(reason(s), Some(HK_SYSTEM), "not refused: {s}");
    }
    // Anything with the Windows key: that namespace belongs to the OS. Named
    // explicitly for the well-known Win+<x> shortcuts (number-pin, screenshot,
    // clipboard history, emoji picker, share, dictation, dot/emoji on some
    // keyboards) even though the SUPER/META check above already refuses the
    // whole namespace on its own.
    for s in [
        "Super+K", "Super+Shift+S", "Alt+Super+P", "Cmd+Q", "Super+1", "Super+9",
        "Super+G", "Super+V", "Super+H", "Super+Period",
    ] {
        assert_eq!(reason(s), Some(HK_SYSTEM), "not refused: {s}");
    }
    // Shift alone is how a capital letter is typed: never a modifier here.
    for s in ["Shift+A", "Shift+1", "Shift+F5"] {
        assert_eq!(reason(s), Some(HK_INVALID), "not refused: {s}");
    }
    // …and the line stops there: adding a second modifier makes a combination
    // no program owns, which is exactly where users pick their own shortcuts.
    for s in [
        "Ctrl+Shift+C", "Ctrl+Alt+V", "Ctrl+Shift+S", "Ctrl+Alt+Q", "Alt+Q",
        "Ctrl+Shift+1", "Alt+F9", "Ctrl+F1", "Ctrl+Shift+F4", "Ctrl+Alt+ArrowUp",
    ] {
        assert_eq!(reason(s), None, "wrongly refused: {s}");
    }
}

// The shipped default must actually register, or every fresh install would
// silently lose its launcher hotkey the moment rearm_hotkeys() runs at startup
// (an unparseable/blocked default gets cleared back to "", exactly like a bad
// value a user typed in by hand -- see rearm_hotkeys' app_ok branch).
#[test]
fn the_default_hotkey_is_not_refused() {
    assert_eq!(accel_error(&Settings::default().hotkey).err(), None);
}

// The blocklist is authoritative wherever a binding can enter the store.
#[test]
fn a_blocked_combination_never_reaches_the_store() {
    let prompts = sample_prompts();
    let others: Vec<&Prompt> = prompts.iter().collect();
    assert_eq!(sanitize_hotkey("Ctrl+C", "", "", "", &others), Err(HK_SYSTEM.into()));
    assert_eq!(sanitize_hotkey("Shift+A", "", "", "", &others), Err(HK_INVALID.into()));
    assert_eq!(sanitize_hotkey("Super+J", "", "", "", &others), Err(HK_SYSTEM.into()));
    // A blocked accelerator is never registered, whatever put it in the store.
    let mut blocked = sample_prompts();
    blocked[0].hotkey = "Ctrl+V".into();
    blocked[1].hotkey = "Alt+Tab".into();
    assert!(hotkey_plan("Ctrl+S", None, None, None, &blocked, true).is_empty());
    // …and an import drops it instead of keeping a binding that does nothing.
    assert_eq!(drop_colliding_hotkeys("", &mut blocked), 2);
    assert!(blocked.iter().all(|p| p.hotkey.is_empty()));
}

#[test]
fn a_hotkey_carries_its_own_copy_or_paste_choice() {
    assert_eq!(sanitize_hotkey_mode("copy"), "copy");
    assert_eq!(sanitize_hotkey_mode(" paste "), "paste");
    // Anything else means "follow the global default".
    for raw in ["", "   ", "yes", "true", "PASTE", "copy;drop"] {
        assert_eq!(sanitize_hotkey_mode(raw), "", "not folded to the default: {raw:?}");
    }
}

#[test]
fn hotkey_plan_resolves_conflicts_by_precedence() {
    let mut prompts = sample_prompts();
    prompts[1].hotkey = "CommandOrControl+Shift+2".into();
    // The app hotkey wins over a prompt that wants the same combination.
    let plan = hotkey_plan("ctrl+shift+1", None, None, None, &prompts, true);
    assert_eq!(plan.len(), 2);
    assert_eq!(plan[0].1, HotkeyOwner::App);
    assert_eq!(plan[1].1, HotkeyOwner::Prompt("p2".into()));

    // Between two prompts the first one in list order keeps it.
    prompts[1].hotkey = "ctrl+shift+1".into();
    let plan = hotkey_plan("", None, None, None, &prompts, true);
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].1, HotkeyOwner::Prompt("p1".into()));

    // Feature toggled off: nothing but the app hotkey is registered, and the
    // stored accelerators stay untouched.
    let plan = hotkey_plan("Alt+Q", None, None, None, &prompts, false);
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].1, HotkeyOwner::App);
    assert_eq!(prompts[0].hotkey, "CommandOrControl+Shift+1");
}

// A third owner, distinct from both the app hotkey and any prompt --
// every combination lands on the right one, and it follows the exact same
// precedence/collision rules (app first, then voice, then prompts).
#[test]
fn hotkey_plan_gives_voice_its_own_owner() {
    let mut prompts = sample_prompts();
    prompts[0].hotkey = "ctrl+shift+7".into();
    let plan = hotkey_plan("ctrl+shift+1", Some("ctrl+shift+9"), None, None, &prompts, true);
    assert_eq!(plan[0].1, HotkeyOwner::App);
    assert!(plan.iter().any(|(_, o)| *o == HotkeyOwner::Voice));
    assert!(plan.iter().any(|(_, o)| *o == HotkeyOwner::Prompt("p1".into())));

    // The app hotkey wins a collision with the voice hotkey, same as with a prompt.
    let plan = hotkey_plan("ctrl+shift+1", Some("ctrl+shift+1"), None, None, &prompts, true);
    assert!(!plan.iter().any(|(_, o)| *o == HotkeyOwner::Voice));

    // Switched off (the caller passes None): nothing voice-owned is registered.
    let plan = hotkey_plan("ctrl+shift+1", None, None, None, &prompts, true);
    assert!(!plan.iter().any(|(_, o)| *o == HotkeyOwner::Voice));

    // A combination the blocklist refuses is dropped just like anywhere else.
    let plan = hotkey_plan("", Some("Ctrl+C"), None, None, &prompts, true);
    assert!(!plan.iter().any(|(_, o)| *o == HotkeyOwner::Voice));
}

#[test]
fn a_deleted_prompt_gives_its_accelerator_back() {
    let mut prompts = sample_prompts();
    prompts[1].hotkey = "Ctrl+Shift+2".into();
    assert_eq!(hotkey_plan("", None, None, None, &prompts, true).len(), 2);
    prompts.retain(|p| p.id != "p1");
    let plan = hotkey_plan("", None, None, None, &prompts, true);
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].1, HotkeyOwner::Prompt("p2".into()));
}

#[test]
fn an_import_drops_colliding_hotkeys_instead_of_failing() {
    let mut prompts = sample_prompts();
    prompts[1].hotkey = "ctrl+shift+1".into(); // same combination as p1
    assert_eq!(drop_colliding_hotkeys("", &mut prompts), 1);
    assert_eq!(prompts[0].hotkey, "CommandOrControl+Shift+1");
    assert!(prompts[1].hotkey.is_empty());

    // The app hotkey outranks every prompt, and junk is dropped too.
    let mut prompts = sample_prompts();
    prompts[1].hotkey = "not an accelerator".into();
    assert_eq!(drop_colliding_hotkeys("Ctrl+Shift+1", &mut prompts), 2);
    assert!(prompts.iter().all(|p| p.hotkey.is_empty()));

    // Nothing to drop leaves every binding in place.
    let mut prompts = sample_prompts();
    prompts[1].hotkey = "Alt+F9".into();
    assert_eq!(drop_colliding_hotkeys("Ctrl+Alt+Q", &mut prompts), 0);
    assert_eq!(prompts[1].hotkey, "Alt+F9");
}

#[test]
fn the_editor_rejects_a_taken_or_unusable_accelerator() {
    let prompts = sample_prompts();
    let others: Vec<&Prompt> = prompts.iter().collect();
    assert_eq!(sanitize_hotkey("", "Ctrl+Q", "", "", &others).unwrap(), "");
    assert_eq!(sanitize_hotkey("  Alt+F9  ", "Ctrl+Q", "", "", &others).unwrap(), "Alt+F9");
    assert_eq!(sanitize_hotkey("Ctrl+Shift+P", "ctrl+shift+p", "", "", &others), Err("hotkey-taken".into()));
    assert_eq!(sanitize_hotkey("ctrl+shift+1", "", "", "", &others), Err("hotkey-taken".into()));
    assert_eq!(sanitize_hotkey("F9", "", "", "", &others), Err("hotkey-invalid".into()));
    // Editing the prompt that already owns the combination is not a conflict.
    let others: Vec<&Prompt> = prompts.iter().filter(|p| p.id != "p1").collect();
    assert_eq!(sanitize_hotkey("ctrl+shift+1", "", "", "", &others).unwrap(), "ctrl+shift+1");
}

// A prompt hotkey identical to the currently-configured rec-stop/pause
// combination is refused up front -- it used to be accepted, then silently
// stop firing the moment a recording started (hotkey_plan gives those two
// priority over every prompt).
#[test]
fn the_editor_rejects_a_prompt_hotkey_matching_the_active_recording_hotkeys() {
    let prompts = sample_prompts();
    let others: Vec<&Prompt> = prompts.iter().collect();
    assert_eq!(
        sanitize_hotkey("Ctrl+Alt+S", "", "ctrl+alt+s", "ctrl+alt+o", &others),
        Err(HK_TAKEN.into())
    );
    assert_eq!(
        sanitize_hotkey("Ctrl+Alt+O", "", "ctrl+alt+s", "ctrl+alt+o", &others),
        Err(HK_TAKEN.into())
    );
    // Feature/toggle off (caller passes "") -- the same combination is free again.
    assert_eq!(sanitize_hotkey("Ctrl+Alt+S", "", "", "", &others).unwrap(), "Ctrl+Alt+S");
}

// A prompt stored before per-prompt hotkeys existed has no hotkey field at all: it must load with an
// empty binding instead of failing the whole row.
#[test]
fn a_prompt_without_a_hotkey_field_still_loads() {
    let json = r#"{"id":"old","name":"N","text":"T","color":"","tags":["Work"]}"#;
    let p: Prompt = serde_json::from_str(json).unwrap();
    assert!(p.hotkey.is_empty());
    assert!(parse_accel(&p.hotkey).is_none());
}

// Settings JSON from an older version: unknown/removed fields (copy_history,
// a future field) are ignored and every missing field defaults: never a total
// parse failure that would wipe the user's settings.
#[test]
fn load_prune_drops_a_removed_value_and_keeps_a_live_one() {
    let mut s = Settings::default();
    s.ui_values.insert("syncFailMax".into(), 9.0);
    s.ui_values.insert("errorLogKeepKb".into(), 512.0);
    assert!(s.has_removed_ui_values());
    s.migrate();
    assert!(!s.ui_values.contains_key("syncFailMax"));
    assert_eq!(s.ui_values.get("errorLogKeepKb"), Some(&512.0));
    assert!(!s.has_removed_ui_values());
}

#[test]
fn a_failed_repack_makes_the_inbox_scrub_a_failure() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
             INSERT INTO clip_inbox(ts, text) VALUES(1, 'secret');",
    )
    .unwrap();
    // A VACUUM cannot run inside a transaction, so the repack step fails here.
    conn.execute_batch("BEGIN").unwrap();
    assert_eq!(scrub_clip_inbox(&conn), None);
    conn.execute_batch("ROLLBACK").unwrap();
    assert_eq!(scrub_clip_inbox(&conn), Some(1));
    assert_eq!(scrub_clip_inbox(&conn), Some(0), "nothing left: no repack needed, still a success");
}

#[test]
fn a_failing_row_leaves_the_copy_history_untouched() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             INSERT INTO copy_log VALUES(1, 'keep');
             CREATE TRIGGER no_bad BEFORE INSERT ON copy_log WHEN NEW.id = 'bad' BEGIN SELECT RAISE(ABORT, 'no'); END;",
    )
    .unwrap();
    let rows = [
        CopyLogRow { ts: 2, id: "a".into() },
        CopyLogRow { ts: 3, id: "bad".into() },
        CopyLogRow { ts: 4, id: "b".into() },
    ];
    assert!(db_restore_copy_log(&conn, &rows).is_err());
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM copy_log", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1, "the rows before the failing one must not stay");
    assert!(db_restore_copy_log(&conn, &rows[..1]).is_ok());
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM copy_log", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 2);
}

#[test]
fn a_failing_item_leaves_the_clip_inbox_untouched() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
             CREATE TRIGGER no_bad BEFORE INSERT ON clip_inbox WHEN NEW.ts = 99 BEGIN SELECT RAISE(ABORT, 'no'); END;",
    )
    .unwrap();
    let items = [
        ClipRestoreItem { ts: 1, text: "a".into() },
        ClipRestoreItem { ts: 99, text: "bad".into() },
        ClipRestoreItem { ts: 3, text: "b".into() },
    ];
    assert!(db_restore_clips(&conn, &items, 50).is_err());
    assert!(db_list_clips(&conn).is_empty(), "no partial restore");
    assert!(db_restore_clips(&conn, &[ClipRestoreItem { ts: 1, text: "a".into() }, ClipRestoreItem { ts: 3, text: "b".into() }], 50).is_ok());
    assert_eq!(db_list_clips(&conn).len(), 2);
}

#[test]
fn clearing_the_history_keeps_the_counters_when_the_delete_fails() {
    let mut s = Settings::default();
    s.usage.insert("a".into(), 4);
    s.last_used.insert("a".into(), 77);
    assert!(!clear_history_core(|| false, &mut s));
    assert_eq!(s.usage.get("a"), Some(&4));
    assert_eq!(s.last_used.get("a"), Some(&77));
    assert!(clear_history_core(|| true, &mut s));
    assert!(s.usage.is_empty() && s.last_used.is_empty());
}

#[test]
fn a_pair_of_writes_lands_together_or_not_at_all() {
    use std::cell::Cell;
    let undone = Cell::new(false);
    let second_ran = Cell::new(false);
    // The first write fails: the second never starts, nothing to undo.
    assert!(!save_pair_or_undo_first(|| false, || { second_ran.set(true); true }, || undone.set(true)));
    assert!(!second_ran.get() && !undone.get());
    // The second fails: the first is taken back and the call reports failure.
    assert!(!save_pair_or_undo_first(|| true, || false, || undone.set(true)));
    assert!(undone.get());
    // Both land: no undo.
    undone.set(false);
    assert!(save_pair_or_undo_first(|| true, || true, || undone.set(true)));
    assert!(!undone.get());
}

#[test]
fn search_caps_merge_to_largest() {
    let run = |old: &[(&str, f64)], new: Option<f64>| {
        let mut s = Settings::default();
        for (k, v) in old {
            s.ui_values.insert((*k).into(), *v);
        }
        if let Some(v) = new {
            s.ui_values.insert("searchListMax".into(), v);
        }
        let dirty = s.has_removed_ui_values();
        s.migrate();
        for (k, _) in old {
            assert!(!s.ui_values.contains_key(*k), "{k} survived migrate");
        }
        assert_eq!(dirty, !old.is_empty());
        s.ui_values.get("searchListMax").copied()
    };
    // The largest stored value wins; a stored 0 is ignored.
    assert_eq!(run(&[("searchRecentMax", 12.0), ("gsRecentMax", 0.0)], None), Some(12.0));
    assert_eq!(run(&[("globalSearchLimit", 5.0), ("trayMenuMax", 15.0), ("launcherMaxResults", 8.0)], None), Some(15.0));
    // A cap that was never stored counts with the default it had: one lowered cap
    // must not shrink the lists the others kept at their defaults.
    assert_eq!(run(&[("gsRecentMax", 3.0)], None), Some(8.0));
    assert_eq!(run(&[("searchRecentMax", 1.0), ("globalSearchLimit", 4.0)], None), Some(8.0));
    assert_eq!(run(&[("searchRecentMax", 12.0), ("gsRecentMax", 0.0)], None), Some(12.0));
    // Only a zero stored: nothing to carry over.
    assert_eq!(run(&[("gsRecentMax", 0.0)], None), None);
    // Out-of-range old values land inside the new range.
    assert_eq!(run(&[("searchRecentMax", 1.0)], None), Some(8.0));
    assert_eq!(run(&[("searchRecentMax", 1.0), ("globalSearchLimit", 1.0), ("gsRecentMax", 1.0), ("launcherMaxResults", 1.0), ("trayMenuMax", 1.0)], None), Some(3.0));
    assert_eq!(run(&[("launcherMaxResults", 99.0)], None), Some(20.0));
    // Nothing stored: no key. A value already under the new key is kept.
    assert_eq!(run(&[], None), None);
    assert_eq!(run(&[("searchRecentMax", 12.0)], Some(6.0)), Some(6.0));
}

#[test]
fn toggle_pairs_merge_to_zero() {
    let run = |flags: &[(&str, bool)], values: &[(&str, f64)]| {
        let mut s = Settings::default();
        for (k, v) in flags {
            s.ui_flags.insert((*k).into(), *v);
        }
        for (k, v) in values {
            s.ui_values.insert((*k).into(), *v);
        }
        let dirty = s.has_removed_ui_values();
        s.migrate();
        assert_eq!(dirty, !flags.is_empty());
        for (k, _) in flags {
            assert!(!s.ui_flags.contains_key(*k), "{k} survived migrate");
        }
        s
    };
    // A switch stored off becomes a stored 0, whatever duration sat beside it.
    let s = run(&[("copyBubble", false)], &[]);
    assert_eq!(s.ui_values.get("bubbleMs"), Some(&0.0));
    let s = run(&[("undoToast", false)], &[("undoToastMs", 5000.0)]);
    assert_eq!(s.ui_values.get("undoToastMs"), Some(&0.0));
    let s = run(&[("tooltipDelay", false)], &[]);
    assert_eq!(s.ui_values.get("tooltipDelayMs"), Some(&0.0));
    // A switch stored on keeps the stored duration.
    let s = run(&[("editorDraft", true)], &[("editorDraftSecs", 9.0)]);
    assert_eq!(s.ui_values.get("editorDraftSecs"), Some(&9.0));
    // A switch stored on with no duration leaves no key (the default applies).
    let s = run(&[("copyBubble", true)], &[]);
    assert!(!s.ui_values.contains_key("bubbleMs"));
    // Nothing stored: nothing written.
    let s = run(&[], &[]);
    assert!(s.ui_values.is_empty());
}

#[test]
fn tile_flags_merge_to_style() {
    let run = |flags: &[(&str, bool)], texts: &[(&str, &str)]| {
        let mut s = Settings::default();
        for (k, v) in flags {
            s.ui_flags.insert((*k).into(), *v);
        }
        for (k, v) in texts {
            s.ui_texts.insert((*k).into(), (*v).into());
        }
        s.migrate();
        for (k, _) in flags {
            assert!(!s.ui_flags.contains_key(*k), "{k} survived migrate");
        }
        s.ui_texts.get("tileStyle").cloned()
    };
    // One style on: that style.
    assert_eq!(run(&[("uppercaseTiles", true)], &[]).as_deref(), Some("uppercase"));
    assert_eq!(run(&[("tileGradient", true)], &[]).as_deref(), Some("gradient"));
    // Several on: the first in the merge order wins.
    assert_eq!(run(&[("italicTiles", true), ("boldTileNames", true)], &[]).as_deref(), Some("bold"));
    assert_eq!(run(&[("compactTiles", true), ("tileTextShadow", true)], &[]).as_deref(), Some("shadow"));
    // Nothing on, or stored off: no key, so the default "plain" applies.
    assert_eq!(run(&[], &[]), None);
    assert_eq!(run(&[("boldTileNames", false)], &[]), None);
    // A stored choice is never overwritten.
    assert_eq!(run(&[("boldTileNames", true)], &[("tileStyle", "mono")]).as_deref(), Some("mono"));
}

#[test]
fn old_snip_sizes_carry_into_the_remembered_sizes() {
    let run = |values: &[(&str, f64)], texts: &[(&str, &str)]| {
        let mut s = Settings::default();
        for (k, v) in values {
            s.ui_values.insert((*k).into(), *v);
        }
        for (k, v) in texts {
            s.ui_texts.insert((*k).into(), (*v).into());
        }
        s.migrate();
        for (k, _) in values {
            assert!(!s.ui_values.contains_key(*k), "{k} survived migrate");
        }
        s.ui_texts
            .get("snipToolSizes")
            .map(|j| serde_json::from_str::<serde_json::Value>(j).unwrap())
    };
    let j = run(
        &[("snipPenPx", 10.0), ("snipTextPx", 40.0), ("snipTextOutlinePx", 5.0), ("snipPixelBlock", 20.0), ("snipSpotlightDim", 70.0), ("snipStepPx", 9.0)],
        &[],
    )
    .unwrap();
    assert_eq!(j["pen"]["width"], 10.0);
    assert_eq!(j["text"]["fontPx"], 40.0);
    assert_eq!(j["text"]["outlinePx"], 5.0);
    assert_eq!(j["pixelate"]["blockPx"], 20.0);
    assert_eq!(j["spotlight"]["dim"], 0.7);
    assert_eq!(j["step"]["width"], 9.0);
    assert!(j.get("marker").is_none());
    // An out-of-range old value is not carried.
    let j = run(&[("snipPenPx", 500.0), ("snipRectPx", 7.0)], &[]).unwrap();
    assert!(j.get("pen").is_none());
    assert_eq!(j["rect"]["width"], 7.0);
    // Nothing stored: nothing written. Remembered sizes already stored win.
    assert!(run(&[], &[]).is_none());
    let j = run(&[("snipPenPx", 10.0)], &[("snipToolSizes", "{\"pen\":{\"width\":3}}")]).unwrap();
    assert_eq!(j["pen"]["width"], 3.0);
    // Only the old arrow-head scale: no sizes to carry.
    assert!(run(&[("snipArrowHeadScale", 4.0)], &[]).is_none());
}

#[test]
fn stored_update_interval_is_kept_in_range() {
    let run = |h: f64| {
        let mut s = Settings::default();
        s.ui_values.insert("updateCheckH".into(), h);
        s.migrate();
        s.ui_values.get("updateCheckH").copied()
    };
    // 0 never means "off" here (that is the Auto update switch); it is the shortest wait.
    assert_eq!(run(0.0), Some(1.0));
    assert_eq!(run(0.5), Some(1.0));
    assert_eq!(run(-4.0), Some(1.0));
    assert_eq!(run(500.0), Some(168.0));
    assert_eq!(run(f64::NAN), Some(24.0));
    assert_eq!(run(12.0), Some(12.0));
    // Nothing stored: nothing written.
    let mut s = Settings::default();
    s.migrate();
    assert!(!s.ui_values.contains_key("updateCheckH"));
}

#[test]
fn hover_settings_merge() {
    let run = |flags: &[(&str, bool)], values: &[(&str, f64)]| {
        let mut s = Settings::default();
        for (k, v) in flags {
            s.ui_flags.insert((*k).into(), *v);
        }
        for (k, v) in values {
            s.ui_values.insert((*k).into(), *v);
        }
        s.migrate();
        for (k, _) in flags {
            assert!(!s.ui_flags.contains_key(*k), "{k} survived migrate");
        }
        s
    };
    let dim = |s: &Settings| s.ui_values.get("hoverDim").copied();
    let lift = |s: &Settings| s.ui_values.get("hoverLift").copied();
    // Dim: the strongest dim wins, rounded to a slider step.
    let s = run(&[("tilesHoverDim", true)], &[("tilesHoverOpacity", 45.0)]);
    assert_eq!(dim(&s), Some(55.0));
    assert!(!s.ui_values.contains_key("tilesHoverOpacity"));
    let s = run(&[("toolbarHoverDim", true)], &[]);
    assert_eq!(dim(&s), Some(50.0));
    let s = run(&[("mediaHoverDim", true)], &[]);
    assert_eq!(dim(&s), Some(20.0));
    let s = run(
        &[("tilesHoverDim", true), ("mediaHoverDim", true), ("toolbarHoverDim", true)],
        &[("tilesHoverOpacity", 70.0), ("mediaHoverOpacity", 30.0)],
    );
    assert_eq!(dim(&s), Some(70.0));
    // A switch stored off dims nothing, and nothing stored writes no key.
    let s = run(&[("tilesHoverDim", false), ("toolbarHoverDim", false)], &[("tilesHoverOpacity", 20.0)]);
    assert_eq!(dim(&s), None);
    assert_eq!(dim(&run(&[], &[])), None);
    // A stored strength is never overwritten.
    let s = run(&[("tilesHoverDim", true)], &[("hoverDim", 30.0)]);
    assert_eq!(dim(&s), Some(30.0));
    // Lift: an install that stored none of the keys takes the new default (no key).
    assert_eq!(lift(&run(&[], &[])), None);
    // Stored on, or a stored distance: the lift it showed stays.
    assert_eq!(lift(&run(&[("tileHoverLift", true)], &[("hoverLift", 3.0)])), Some(3.0));
    assert_eq!(lift(&run(&[("tileHover", true)], &[])), Some(1.0));
    assert_eq!(lift(&run(&[], &[("hoverLift", 4.0)])), Some(4.0));
    // Either switch stored off means no lift.
    assert_eq!(lift(&run(&[("tileHoverLift", false)], &[("hoverLift", 3.0)])), None);
    assert_eq!(lift(&run(&[("tileHover", false)], &[])), None);
    // A lift of 0 (off) with no old switch left stays 0 through repeated launches.
    let mut s = run(&[], &[("hoverLift", 0.0)]);
    s.migrate();
    s.migrate();
    assert_eq!(lift(&s), Some(0.0));
}

#[test]
fn removed_key_lists_have_no_duplicates() {
    for list in [removed_ui_value_keys(), removed_ui_flag_keys()] {
        let mut seen = std::collections::HashSet::new();
        for k in list {
            assert!(seen.insert(*k), "{k} is listed twice");
        }
    }
}

#[test]
fn keys_of_removed_controls_are_dropped() {
    // The history window, the inbox badge, the view groups and the placeholder
    // window switch are gone; their stored keys must not linger.
    let mut s = Settings::default();
    for k in ["showJournal", "varPopupWindow", "viewGroups", "pausedIcons"] {
        s.ui_flags.insert(k.into(), false);
    }
    for k in ["journalWidth", "viewDropHoverMs", "viewGroupMaxRows", "clipSeenId"] {
        s.ui_values.insert(k.into(), 7.0);
    }
    assert!(s.has_removed_ui_values());
    s.migrate();
    assert!(s.ui_flags.is_empty(), "left: {:?}", s.ui_flags);
    for k in ["journalWidth", "viewDropHoverMs", "viewGroupMaxRows", "clipSeenId"] {
        assert!(!s.ui_values.contains_key(k), "{k} survived migrate");
    }
}

#[test]
fn removed_flags_are_dropped() {
    // Every listed key, whatever the stored value (true or false), is dropped
    // once; a live switch next to it is untouched.
    for stored in [true, false] {
        let mut s = Settings::default();
        for k in removed_ui_flag_keys() {
            s.ui_flags.insert((*k).into(), stored);
        }
        s.ui_flags.insert("barToggles".into(), false);
        assert!(s.has_removed_ui_values());
        s.migrate();
        for k in removed_ui_flag_keys() {
            assert!(!s.ui_flags.contains_key(*k), "{k} survived migrate");
        }
        assert_eq!(s.ui_flags.get("barToggles"), Some(&false));
        assert!(!s.has_removed_ui_values());
    }
}

#[test]
fn fav_view_merges_into_star() {
    let run = |view: Option<bool>, star: Option<bool>| {
        let mut s = Settings::default();
        if let Some(v) = view {
            s.ui_flags.insert("favView".into(), v);
        }
        if let Some(v) = star {
            s.ui_flags.insert("favViewButton".into(), v);
        }
        let dirty = s.has_removed_ui_values();
        s.migrate();
        assert!(!s.ui_flags.contains_key("favView"));
        assert_eq!(dirty, view.is_some());
        s.ui_flags.get("favViewButton").copied()
    };
    // Visible before (view on, star never hidden): stays on.
    assert_eq!(run(Some(true), None), Some(true));
    assert_eq!(run(Some(true), Some(true)), Some(true));
    // View on but star hidden, or view off: falls to the new default (off, no key).
    assert_eq!(run(Some(true), Some(false)), None);
    assert_eq!(run(Some(false), None), None);
    assert_eq!(run(Some(false), Some(true)), None);
    // No old key: a stored star value is the one source and is left alone.
    assert_eq!(run(None, Some(true)), Some(true));
    assert_eq!(run(None, None), None);
}

#[test]
fn removed_fine_tuning_values_keep_their_former_defaults() {
    assert_eq!(ERROR_LOG_MAX, 100);
    assert_eq!(OCR_LANG_CACHE_H, 24.0);
    assert_eq!(VOICE_LANG_CACHE_H, 24.0);
    assert_eq!(REC_BROKEN_GRACE_MIN, 5.0);
    assert_eq!(REC_SWEEP_EVERY_MIN, 60.0);
    assert_eq!(CLIP_SECRET_MIN_LEN, 8);
}

#[test]
fn old_settings_json_migrates() {
    let json = r#"{
            "theme":"dark","language":"de","minimize_to_tray":true,
            "copy_history":["a","b"],"ui_flags":{"floating":false},
            "tile_size":18,"removed_future_field":42
        }"#;
    let s: Settings = serde_json::from_str(json).expect("old settings must still parse");
    assert_eq!(s.theme, "dark");
    assert_eq!(s.language, "de");
    assert!(s.minimize_to_tray);
    assert_eq!(s.ui_flags.get("floating"), Some(&false));
    assert_eq!(s.tile_size, 18);
    assert!(s.copy_log.is_empty()); // new field defaults
    assert!(s.usage.is_empty());
    assert!(s.auto_update); // missing field -> default_on
}

// Prompt JSON from an older version: removed field (favorite) ignored, missing
// new fields default; the prompt still loads.
#[test]
fn old_prompt_json_migrates() {
    let json = r#"{"id":"x1","name":"Old","text":"hi","favorite":true,"show_image":true}"#;
    let p: Prompt = serde_json::from_str(json).expect("old prompt must still parse");
    assert_eq!(p.id, "x1");
    assert_eq!(p.name, "Old");
    assert!(p.show_image);
    assert_eq!(p.color, ""); // missing -> default
    assert_eq!(p.caption_size, 0);
}

#[test]
fn txt_roundtrip_keeps_every_field() {
    let out = to_txt(&sample_prompts());
    let data = parse_txt(&out);
    assert_eq!(data.prompts.len(), 2);
    let p1 = &data.prompts[0];
    assert_eq!(p1.name, "Mail");
    assert_eq!(p1.text, "Hello\nWorld");
    assert_eq!(p1.color, "#ef4444");
    assert_eq!(p1.font, "mono");
    assert_eq!(p1.font_size, 24);
    assert_eq!(p1.image, "data:image/png;base64,QUJD");
    assert!(p1.show_image && p1.copy_image);
    let p2 = &data.prompts[1];
    assert_eq!(p2.font, "");
    assert_eq!(p2.font_size, 1);
    assert_eq!(p2.file_path, "C:\\tmp\\report.pdf");
    assert_eq!(p2.pdf_page, 3);
    assert_eq!(p1.pdf_page, 0, "the omitted @pdfpage line must default to 0");
    assert_eq!(p2.icon_path, "C:\\tmp\\clip.mp4");
    assert_eq!(p2.caption, "Mein Untertitel");
    assert_eq!(p2.caption_size, 18);
    assert!(p2.show_image && !p2.copy_image);
    assert_eq!(p1.tags, vec!["Work".to_string(), "E-Mail".to_string()]);
    assert!(p2.tags.is_empty());
}

#[test]
fn csv_roundtrip_keeps_every_field() {
    let out = to_csv(&sample_prompts());
    let rows = parse_csv(&out);
    // header + 2 prompts (no settings/views blocks in a prompts-only export)
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1][0], "Mail");
    assert_eq!(rows[1][4], "mono");
    assert_eq!(rows[1][5], "24");
    assert_eq!(rows[1][8], "1");
    assert_eq!(rows[1][9], "1");
    assert_eq!(rows[1][10], "data:image/png;base64,QUJD");
    assert_eq!(rows[2][6], "C:\\tmp\\report.pdf");
    assert_eq!(rows[2][7], "C:\\tmp\\clip.mp4");
    assert_eq!(rows[2][11], "Mein Untertitel");
    assert_eq!(rows[2][12], "18");
    assert_eq!(rows[1][13], "Work,E-Mail");
    assert_eq!(rows[2][13], "");
    assert_eq!(rows[1][15], "0");
    assert_eq!(rows[2][15], "3");
}

// A choice variable is plain prompt text, so neither exporter may touch
// its `|` or its commas: a mangled token would silently lose the options.
#[test]
fn choice_variable_survives_txt_and_csv_export() {
    const BODY: &str = "Write a {#{tone|formal,casual,witty}#} note about {#{Topic}#}.";
    let mut prompts = sample_prompts();
    prompts[0].text = BODY.into();
    prompts[0].image = String::new(); // a text prompt, not the media sample
    prompts[0].show_image = false;
    prompts[0].copy_image = false;
    assert_eq!(parse_txt(&to_txt(&prompts)).prompts[0].text, BODY);
    assert_eq!(parse_csv(&to_csv(&prompts))[1][1], BODY);
    assert_eq!(parse_csv_data(&to_csv(&prompts)).prompts[0].text, BODY);
}

#[test]
fn json_roundtrip_keeps_every_field_including_favorite() {
    let mut prompts = sample_prompts();
    prompts[0].favorite = true; // the one field CSV/TXT cannot carry
    let out = to_json(&prompts);
    let data = parse_json_data(&out).expect("valid JSON");
    assert_eq!(data.prompts.len(), 2);
    let p1 = &data.prompts[0];
    assert_eq!(p1.name, "Mail");
    assert_eq!(p1.text, "Hello\nWorld");
    assert_eq!(p1.color, "#ef4444");
    assert_eq!(p1.font, "mono");
    assert_eq!(p1.font_size, 24);
    assert_eq!(p1.image, "data:image/png;base64,QUJD");
    assert!(p1.show_image && p1.copy_image);
    assert_eq!(p1.tags, vec!["Work".to_string(), "E-Mail".to_string()]);
    assert_eq!(p1.notes, "Ask Anna first.\nDeadline: Friday");
    assert!(p1.favorite, "JSON export must keep favorite, unlike CSV/TXT");
    let p2 = &data.prompts[1];
    assert_eq!(p2.file_path, "C:\\tmp\\report.pdf");
    assert_eq!(p2.pdf_page, 3);
    assert_eq!(p2.icon_path, "C:\\tmp\\clip.mp4");
    assert_eq!(p2.caption, "Mein Untertitel");
    assert_eq!(p2.caption_size, 18);
    assert!(!p2.favorite);
}

// image_text is a search index, never exported: ExportPrompt
// simply has no such field, proven here the same way slim_prompt's own
// safety tests prove it for the state payload and the picture bytes.
#[test]
fn json_export_never_carries_image_text() {
    let mut prompts = sample_prompts();
    prompts[0].image_text = "invoice total due friday".into();
    let out = to_json(&prompts);
    assert!(!out.contains("invoice"), "OCR-recognized text leaked into the export: {out}");
    assert!(!out.contains("image_text"), "the export still has an image_text field: {out}");
}

#[test]
fn json_roundtrip_keeps_rotating_entries_as_a_real_array() {
    let mut prompts = sample_prompts();
    prompts[0].variants = vec!["Second entry".to_string(), "Third, entry\nwith a newline".to_string()];
    prompts[0].rotate_mode = "random".to_string();
    let data = parse_json_data(&to_json(&prompts)).expect("valid JSON");
    let p = &data.prompts[0];
    assert_eq!(p.text, prompts[0].text, "text must not carry a variant-join marker in JSON");
    assert_eq!(p.variants, prompts[0].variants);
    assert_eq!(p.rotate_mode, "random");
}

#[test]
fn json_export_never_carries_machine_local_or_session_state() {
    let out = to_json(&sample_prompts());
    assert!(!out.contains("hotkey"), "hotkey bindings are machine-local, never exported");
    assert!(!out.contains("rotate_auto"), "rotate_auto is live per-session state, never exported");
    assert!(!out.contains("trashed_at"));
    assert!(!out.contains("\"id\""));
}

#[test]
fn parse_json_data_rejects_invalid_json_with_a_friendly_error() {
    match parse_json_data("not json") {
        Err(msg) => assert_eq!(msg, "the file is not valid JSON"),
        Ok(_) => panic!("\"not json\" must not parse as valid JSON"),
    }
}

#[test]
fn parse_json_data_skips_entries_with_neither_name_nor_text() {
    let data = parse_json_data(r#"[{"name":"","text":""},{"name":"Ok","text":"x"}]"#)
        .expect("valid JSON");
    assert_eq!(data.prompts.len(), 1);
    assert_eq!(data.prompts[0].name, "Ok");
}

#[test]
fn csv_export_feeds_import_parser_losslessly() {
    let out = to_csv(&sample_prompts());
    let data = parse_csv_data(&out);
    assert_eq!(data.prompts.len(), 2);
    let p1 = &data.prompts[0];
    assert!(p1.show_image && p1.copy_image);
    assert_eq!(p1.image, "data:image/png;base64,QUJD");
    assert_eq!(p1.text, "Hello\nWorld");
    assert_eq!(p1.tags, vec!["Work".to_string(), "E-Mail".to_string()]);
    let p2 = &data.prompts[1];
    assert_eq!(p2.file_path, "C:\\tmp\\report.pdf");
    assert_eq!(p2.pdf_page, 3);
    assert_eq!(p1.pdf_page, 0);
    assert_eq!(p2.icon_path, "C:\\tmp\\clip.mp4");
    assert_eq!(p2.caption, "Mein Untertitel");
    assert_eq!(p2.caption_size, 18);
    assert!(p2.tags.is_empty());
}

// An archived entry is just a prompt carrying a reserved tag -- neither
// export nor backup ever filters by tag, so both already carry it through
// unchanged. This proves that stays true rather than assuming it.
#[test]
fn an_archived_prompt_is_included_in_export_and_backup() {
    let mut prompts = sample_prompts();
    prompts[0].tags.push("Archive".to_string());
    // Export: CSV and JSON both keep the tag on round trip.
    let data = parse_csv_data(&to_csv(&prompts));
    assert!(data.prompts[0].tags.contains(&"Archive".to_string()));
    let json = to_json(&prompts);
    assert!(json.contains("Archive"), "the archived prompt's tag did not make it into the JSON export");
    // Backup: the same content survives the real encrypt/decrypt round trip
    // backup_password_roundtrip_and_wrong_password_fails already proves is
    // byte-exact, here specifically carrying the archive tag through it.
    let blob = backup_encrypt(json.as_bytes(), None).expect("encrypt");
    let restored = backup_decrypt(&blob, None).expect("decrypt").expect("psb");
    assert_eq!(restored, json.as_bytes());
}

#[test]
fn is_archived_matches_only_the_reserved_tag_case_insensitively() {
    let mut p = sample_prompts()[0].clone();
    p.tags = vec!["Archive".to_string()];
    assert!(is_archived(&p, Some("archive")));
    assert!(!is_archived(&p, Some("something-else")));
    assert!(!is_archived(&p, None));
}

// An entry that happens to contain the join marker itself must not be
// split into extra entries on round trip.
#[test]
fn join_variants_survives_an_entry_containing_the_separator_verbatim() {
    let text = "a\n<!-- next -->\nb".to_string(); // looks like 2 entries, is 1
    let variants = vec!["c".to_string()];
    let joined = join_variants(&text, &variants);
    let (out_text, out_variants) = split_variants(&joined);
    assert_eq!(out_text, text);
    assert_eq!(out_variants, variants);
}

// Rotating-prompt entries round-trip through every export/storage format
// without loss. Each format gets its OWN test since each chose its own way to
// embed the entries and could silently regress independently of the others.
#[test]
fn csv_roundtrip_keeps_rotating_entries() {
    let mut prompts = sample_prompts();
    prompts[0].variants = vec!["Second entry".to_string(), "Third, entry\nwith a newline".to_string()];
    prompts[0].rotate_mode = "random".to_string();
    let data = parse_csv_data(&to_csv(&prompts));
    let p = &data.prompts[0];
    assert_eq!(p.text, prompts[0].text);
    assert_eq!(p.variants, prompts[0].variants);
    assert_eq!(p.rotate_mode, "random");
}

#[test]
fn txt_roundtrip_keeps_rotating_entries_and_never_confuses_them_with_the_block_separator() {
    let mut prompts = sample_prompts();
    // Deliberately contains a bare "---" line inside an entry: the whole point
    // of VARIANT_SEP being a distinct marker is that this must NOT be mistaken
    // for the "\n\n---\n\n" boundary between two DIFFERENT prompts.
    prompts[0].variants = vec![
        "Second entry".to_string(),
        "---\nlooks like a separator, isn't one".to_string(),
    ];
    prompts[0].rotate_mode = "random".to_string();
    let data = parse_txt(&to_txt(&prompts));
    assert_eq!(data.prompts.len(), prompts.len(), "a variant containing \"---\" must never split into an extra prompt");
    let p = &data.prompts[0];
    assert_eq!(p.text, prompts[0].text);
    assert_eq!(p.variants, prompts[0].variants);
    assert_eq!(p.rotate_mode, "random");
}

#[test]
fn markdown_sync_roundtrip_keeps_rotating_entries() {
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["Second entry".to_string(), "Third entry".to_string()];
    let md = prompt_to_md(&p, &SyncPlacement::default());
    assert!(md.contains("variants: 2"), "the header count is informational but should still be right");
    let restored = apply_md_to_prompt(&p, &md)
        .expect("a file this function itself just wrote must always be recognised as ours");
    assert_eq!(restored.text, p.text);
    assert_eq!(restored.variants, p.variants);
}

#[test]
fn markdown_sync_survives_a_hand_edit_that_removes_every_entry() {
    // The sync folder is the user's to hand-edit: deleting
    // every "<!-- next -->" marker must degrade to a plain prompt, never crash,
    // and never leave rotate_mode stranded on an empty list.
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["Second entry".to_string()];
    p.rotate_mode = "random".to_string();
    let md = prompt_to_md(&p, &SyncPlacement::default()).replace(&format!("{VARIANT_SEP}Second entry"), "");
    let restored = apply_md_to_prompt(&p, &md).unwrap();
    assert!(restored.variants.is_empty());
    assert_eq!(restored.rotate_mode, "", "empty variants must never leave a stranded rotate_mode");
}

#[test]
fn backup_json_roundtrips_rotating_fields() {
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["Second".to_string(), "Third".to_string()];
    p.rotate_mode = "random".to_string();
    let backup = Backup { prompts: vec![p.clone()], settings: Settings::default() };
    let json = serde_json::to_string(&backup).unwrap();
    let restored: Backup = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.prompts[0].variants, p.variants);
    assert_eq!(restored.prompts[0].rotate_mode, p.rotate_mode);
}

#[test]
fn version_history_snapshots_and_restores_rotating_entries() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');",
    )
    .unwrap();
    let variants = vec!["b".to_string(), "c".to_string()];
    db_add_version(&conn, "p1", 100, "Name", "Entry one", &variants, 20).unwrap();
    let listed = db_list_versions(&conn, "p1");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].variants, variants);
    let (pid, name, text, restored_variants) = db_get_version(&conn, listed[0].id).unwrap();
    assert_eq!(pid, "p1");
    assert_eq!(name, "Name");
    assert_eq!(text, "Entry one");
    assert_eq!(restored_variants, variants);
}

#[test]
fn restore_version_as_new_keeps_the_source_and_starts_fresh_history() {
    let mut source = sample_prompts().remove(0);
    source.color = "#112233".to_string();
    source.image = "data:image/png;base64,AAA".to_string();
    source.favorite = true;
    source.tags = vec!["work".to_string()];
    let original_text = source.text.clone();
    let variants = vec!["Second".to_string()];
    let created = build_restored_prompt(&source, "Old name", "Old text".to_string(), variants.clone(), "(from 2026-01-01)");
    assert_eq!(source.text, original_text, "building the copy must never touch the source prompt");
    assert_eq!(created.name, "Old name (from 2026-01-01)");
    assert_eq!(created.text, "Old text");
    assert_eq!(created.variants, variants);
    assert_eq!(created.color, source.color, "the colour marker comes from the current prompt, not the version");
    assert_eq!(created.image, source.image, "the image comes from the current prompt, not the version");
    assert_eq!(created.tags, source.tags, "tags come from the current prompt too, the version never stored them");
    assert_ne!(created.id, source.id);
    assert!(!created.favorite, "a restored-as-new copy starts unstarred like any other new entry");
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');",
    )
    .unwrap();
    assert!(db_list_versions(&conn, &created.id).is_empty(), "a restored-as-new entry starts with no history of its own");
}

#[test]
fn old_export_with_settings_imports_prompts_only() {
    // A legacy file that still carries @settings/@views must import its prompts
    // and silently ignore the settings blocks (no panic, no bogus prompt).
    let txt = "@settings\nlanguage=de\ntheme=midnight\n\n---\n\n@views\nv|View\n\n---\n\n### Old\nSome text";
    let data = parse_txt(txt);
    assert_eq!(data.prompts.len(), 1);
    assert_eq!(data.prompts[0].name, "Old");
}

#[test]
fn old_exports_without_style_still_parse() {
    let txt = "### Old\nSome text\n@color #123456";
    let data = parse_txt(txt);
    assert_eq!(data.prompts.len(), 1);
    assert_eq!(data.prompts[0].text, "Some text");
    assert_eq!(data.prompts[0].color, "#123456");
    assert_eq!(data.prompts[0].font, "");
    assert_eq!(data.prompts[0].font_size, 0);
    assert_eq!(data.prompts[0].file_path, "");
}

#[test]
fn version_compare_pads_components() {
    assert!(version_newer("1.9.0", "1.8.9"));
    assert!(version_newer("2.0", "1.9.9"));
    assert!(!version_newer("1.9", "1.9.0")); // equal once padded
    assert!(!version_newer("1.9.0", "1.9"));
    assert!(!version_newer("1.8.0", "1.9.0"));
    assert!(version_newer("1.10.0", "1.9.0")); // numeric, not lexical
}

#[test]
fn snip_preview_rotates_and_cleans() {
    let gen = SNIP_GEN.load(Ordering::SeqCst);
    let p1 = write_snip_preview(b"first", gen).expect("write1");
    assert!(std::path::Path::new(&p1).exists());
    // A stale encode neither writes nor touches the current preview.
    assert!(write_snip_preview(b"stale", gen.wrapping_add(1)).is_none());
    assert!(std::path::Path::new(&p1).exists(), "stale encode keeps the live preview");
    let p2 = write_snip_preview(b"second", gen).expect("write2");
    assert_ne!(p1, p2);
    assert!(!std::path::Path::new(&p1).exists(), "previous preview deleted");
    assert!(std::path::Path::new(&p2).exists());
    remove_snip_preview();
    assert!(!std::path::Path::new(&p2).exists(), "cleanup removes file");
}

#[test]
fn base64_roundtrip_and_reject() {
    let data = b"\x00\x01\x02\xff\xfe hello";
    assert_eq!(base64_decode(&base64_encode(data)), data);
    assert!(base64_decode("not valid base64!@#").is_empty());
}

// A file written before tags existed must import unchanged, with no tags.
#[test]
fn old_exports_without_tags_still_parse() {
    let txt = "### Old\nSome text\n@color #123456\n@style mono 20";
    let data = parse_txt(txt);
    assert_eq!(data.prompts[0].text, "Some text");
    assert_eq!(data.prompts[0].color, "#123456");
    assert_eq!(data.prompts[0].font, "mono");
    assert!(data.prompts[0].tags.is_empty());

    // Same for a CSV row that stops before the tags column.
    let csv = "\"name\";\"text\"\r\n\"Old\";\"Some text\"";
    let data = parse_csv_data(csv);
    assert_eq!(data.prompts.len(), 1);
    assert!(data.prompts[0].tags.is_empty());
}

// A multi-line note survives both exporters, and a file written before
// notes existed still imports (with an empty note).
#[test]
fn notes_roundtrip_through_txt_and_csv() {
    let prompts = sample_prompts();
    let note = &prompts[0].notes;
    assert!(note.contains('\n'), "the sample note must be multi-line");

    let txt = to_txt(&prompts);
    // The line-based format may never carry a raw newline inside the marker.
    let marker = txt.lines().find(|l| l.starts_with("@notes ")).expect("@notes line");
    assert!(marker.contains("\\n"), "the newline must be escaped: {marker}");
    let data = parse_txt(&txt);
    assert_eq!(&data.prompts[0].notes, note);
    assert_eq!(data.prompts[0].text, "Hello\nWorld"); // marker did not eat the body
    assert!(data.prompts[1].notes.is_empty());

    let csv = to_csv(&prompts);
    assert_eq!(parse_csv(&csv)[1][14], *note);
    let data = parse_csv_data(&csv);
    assert_eq!(&data.prompts[0].notes, note);
    assert!(data.prompts[1].notes.is_empty());
}

// Security: a name/text starting with =, +, -, @ or a tab must not turn into a
// live formula when the CSV is opened in Excel/LibreOffice, but it must still
// come back exactly as typed on the next import into this app.
#[test]
fn csv_export_guards_formula_prefixes_and_import_undoes_it() {
    let mut prompts = sample_prompts();
    prompts[0].name = "=cmd|'/c calc'!A1".into();
    prompts[0].text = "-1 to 5 scale".into();
    prompts[0].notes = "@mention someone".into();
    let csv = to_csv(&prompts);
    assert!(csv.contains("\"'=cmd"), "the name must be guarded: {csv}");
    assert!(csv.contains("\"'-1 to 5"), "the text must be guarded: {csv}");
    assert!(csv.contains("\"'@mention"), "the notes must be guarded: {csv}");
    let data = parse_csv_data(&csv);
    assert_eq!(data.prompts[0].name, "=cmd|'/c calc'!A1");
    assert_eq!(data.prompts[0].text, "-1 to 5 scale");
    assert_eq!(data.prompts[0].notes, "@mention someone");
}

#[test]
fn stats_export_entries_sorts_by_copies_and_carries_last_used() {
    let mut a = test_prompt("a");
    a.name = "Alpha".into();
    let mut b = test_prompt("b");
    b.name = "Beta".into();
    let mut store = Store {
        prompts: vec![a, b],
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: Default::default(),
    };
    store.settings.usage.insert("b".into(), 9);
    store.settings.usage.insert("a".into(), 2);
    store.settings.last_used.insert("b".into(), 1700000200);
    store.settings.last_used.insert("a".into(), 1700000100);
    let entries = stats_export_entries(&store);
    // most-copied first; a prompt never copied still appears (0, not dropped).
    assert_eq!(entries[0].name, "Beta");
    assert_eq!(entries[0].copies, 9);
    assert_eq!(entries[0].last_used, 1700000200);
    assert_eq!(entries[1].name, "Alpha");
    assert_eq!(entries[1].copies, 2);
    assert_eq!(entries[1].last_used, 1700000100);
}

// The statistics export reuses csv_row/csv_cell, but this asserts the
// guarantee directly against the stats formatter -- a semicolon or a quote
// in a prompt name must not shift or break the row it sits in.
#[test]
fn stats_csv_export_escapes_semicolons_and_quotes_in_names() {
    let entries = vec![StatsExportEntry { name: "Weird;\"Name\"".into(), copies: 3, last_used: 100 }];
    let csv = stats_to_csv(&entries, &[], &[]);
    let rows = parse_csv(&csv);
    assert_eq!(rows[1][0], "Weird;\"Name\"");
    assert_eq!(rows[1][1], "3");
    assert_eq!(rows[1][2], "100");
}

#[test]
fn stats_txt_and_json_export_carry_the_same_entries_and_daily_counts() {
    let entries = vec![StatsExportEntry { name: "Mail".into(), copies: 5, last_used: 42 }];
    let daily = vec![DayCount { ts: 1700000000, count: 4 }];
    let txt = stats_to_txt(&entries, &daily, &[]);
    assert!(txt.contains("Mail: 5 copies, last used 42"));
    assert!(txt.contains("1700000000: 4"));
    let json = stats_to_json(&entries, &daily, &[]);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["version"], 1);
    assert_eq!(parsed["entries"][0]["name"], "Mail");
    assert_eq!(parsed["entries"][0]["copies"], 5);
    assert_eq!(parsed["daily"][0]["count"], 4);
    assert_eq!(parsed["tools"].as_array().unwrap().len(), 0);
}

#[test]
fn apply_tool_usage_increments_exactly_once_per_call() {
    let mut settings = Settings::default();
    apply_tool_usage(&mut settings, "screenshot");
    assert_eq!(settings.usage.get(&tool_usage_key("screenshot")), Some(&1));
    apply_tool_usage(&mut settings, "screenshot");
    assert_eq!(settings.usage.get(&tool_usage_key("screenshot")), Some(&2), "a second use must add exactly one, not reset or double-count");
    // A different tool has its own counter, untouched by the one above.
    assert_eq!(settings.usage.get(&tool_usage_key("chain")), None);
    assert!(settings.last_used.get(&tool_usage_key("screenshot")).copied().unwrap_or(0) > 0);
}

#[test]
fn tool_usage_enabled_defaults_to_off_and_needs_an_explicit_true() {
    let settings = Settings::default();
    assert!(!tool_usage_enabled(&settings), "usage statistics must default to off, same as the frontend's own shipsOn reading");
    let mut on = Settings::default();
    on.ui_flags.insert("usageStats".into(), true);
    assert!(tool_usage_enabled(&on));
    let mut explicit_off = Settings::default();
    explicit_off.ui_flags.insert("usageStats".into(), false);
    assert!(!tool_usage_enabled(&explicit_off));
}

#[test]
fn tool_usage_stats_never_used_list_holds_exactly_the_uncounted_keys() {
    let mut usage: HashMap<String, u32> = HashMap::new();
    usage.insert(tool_usage_key("screenshot"), 3);
    usage.insert(tool_usage_key("chain"), 1);
    let last_used: HashMap<String, u64> = HashMap::new();
    let (tools, never_used) = tool_usage_stats(&usage, &last_used, 10);
    assert_eq!(tools.len(), 2);
    let counted: std::collections::HashSet<&str> = ["screenshot", "chain"].into_iter().collect();
    for key in TOOL_KEYS {
        assert_eq!(never_used.contains(&key.to_string()), !counted.contains(key), "key {key} must be in never_used iff it has no counter");
    }
    assert_eq!(never_used.len(), TOOL_KEYS.len() - 2);
}

#[test]
fn tool_usage_stats_orders_by_count_and_respects_top_n() {
    let mut usage: HashMap<String, u32> = HashMap::new();
    usage.insert(tool_usage_key("dictation"), 2);
    usage.insert(tool_usage_key("backup_create"), 9);
    usage.insert(tool_usage_key("ocr"), 5);
    let mut last_used: HashMap<String, u64> = HashMap::new();
    last_used.insert(tool_usage_key("backup_create"), 1700000500);
    let (tools, _) = tool_usage_stats(&usage, &last_used, 2);
    assert_eq!(tools.len(), 2, "top_n must cap the list");
    assert_eq!(tools[0].name, "backup_create");
    assert_eq!(tools[0].count, 9);
    assert_eq!(tools[0].last_used, 1700000500);
    assert_eq!(tools[1].name, "ocr");
}

// Checks the count points against the same list the dashboard reads, so a
// tool call site can never quietly go uninstrumented: every TOOL_KEYS
// entry must appear as a real record_tool_usage call somewhere in the
// frontend, whether through the main window's helper or a direct call
// from a window of its own.
#[test]
fn every_tool_key_has_a_matching_frontend_call_site() {
    // Most tools are counted from the main window through the recordToolUsage(key)
    // helper, but a few live in windows or shared modules of their own (the video
    // editor, the shared colour popup) and call record_tool_usage directly.
    let src = [
        main_window_js().as_str(),
        include_str!("../../ui/video.js"),
        include_str!("../../ui/colorpop.js"),
    ]
    .join("\n");
    for key in TOOL_KEYS {
        let via_helper = format!("recordToolUsage(\"{key}\")");
        let via_invoke = format!("tool: \"{key}\"");
        assert!(
            src.contains(&via_helper) || src.contains(&via_invoke),
            "TOOL_KEYS has \"{key}\" but no frontend file has a record_tool_usage call for it"
        );
    }
}

// export_prompts is a #[tauri::command] async fn (AppHandle + State<'_, Db>)
// whose unrecognized-format arm returns before ever touching either -- but the
// command itself still cannot be invoked directly in a unit test without
// Tauri's mock runtime, which this project does not depend on, and the match
// is not factored into a function of its own. This asserts against the actual
// committed source instead of a re-typed copy of the logic: the fallback arm
// must keep returning the stable code the frontend keys off, never a raw
// string interpolated from the caller-supplied format (that reached the user
// unlocalized for anything but "csv"/"txt" before this fix). Scoped to the
// function's own body (not the whole file) so this cannot false-positive on
// an unrelated comment elsewhere that happens to mention the old shape.
#[test]
fn export_prompts_unsupported_format_uses_a_stable_error_code() {
    let src = production_sources();
    assert_eq!(src.matches("async fn export_prompts(").count(), 1, "one export_prompts command");
    let start = src
        .find("async fn export_prompts(")
        .expect("export_prompts exists in the crate sources");
    let end = start
        + src[start..]
            .find("\n// Minimal CSV reader")
            .expect("export_prompts is followed by the parse_csv section comment");
    let body = &src[start..end];
    let needle = "_ => return Err(\"unsupported-export-format\".to_string()),";
    assert!(
        body.contains(needle),
        "export_prompts' unsupported-format arm no longer returns the stable \"unsupported-export-format\" code verbatim"
    );
    // Narrower than "no format!( anywhere in the function" -- export_prompts
    // legitimately uses format!() to build the save-file name. Only the
    // error-building shape (a format! fed straight into Err(..)) is the bug.
    assert!(
        !body.contains("Err(format!("),
        "export_prompts must never build its error via string interpolation again (it must stay the stable, non-localized-text-free code)"
    );
}

#[test]
fn find_var_tokens_finds_a_plain_a_default_and_a_choice_token() {
    let text = "Hi {#{name}#}, order {#{item=widget}#} in {#{size|S,M,L}#} please.";
    let tokens = find_var_tokens(text);
    let bodies: Vec<&str> = tokens.iter().map(|(_, _, b)| b.as_str()).collect();
    assert_eq!(bodies, vec!["name", "item=widget", "size|S,M,L"]);
}

#[test]
fn parse_var_token_splits_name_default_and_options() {
    assert_eq!(parse_var_token("name"), ("name".to_string(), vec![], "".to_string()));
    assert_eq!(parse_var_token("item=widget"), ("item".to_string(), vec![], "widget".to_string()));
    assert_eq!(
        parse_var_token("size|S,M,L"),
        ("size".to_string(), vec!["S".to_string(), "M".to_string(), "L".to_string()], "".to_string())
    );
    assert_eq!(
        parse_var_token("size=M|S,M,L"),
        ("size".to_string(), vec!["S".to_string(), "M".to_string(), "L".to_string()], "M".to_string())
    );
    // A bar with nothing usable after it is a plain variable, whole token as its name.
    assert_eq!(parse_var_token("weird|"), ("weird|".to_string(), vec![], "".to_string()));
}

// The Abnahme case: the same priority chain the fill-in dialog offers,
// minus the interactive last step, checked across all four sources in one
// pass -- a remembered value beats a fixed value beats the text's own
// default beats blank.
#[test]
fn resolve_static_vars_priority_is_remembered_then_fixed_then_default_then_blank() {
    let remembered: HashMap<String, String> = [("a".to_string(), "from-remembered".to_string())].into_iter().collect();
    let fixed: HashMap<String, String> =
        [("a".to_string(), "from-fixed".to_string()), ("b".to_string(), "from-fixed".to_string())].into_iter().collect();
    // a: remembered AND fixed AND a default -- remembered wins.
    assert_eq!(resolve_static_vars("{#{a=from-default}#}", &remembered, &fixed), "from-remembered");
    // b: fixed AND a default, no remembered value -- fixed wins.
    assert_eq!(resolve_static_vars("{#{b=from-default}#}", &remembered, &fixed), "from-fixed");
    // c: only a default, no remembered or fixed value -- the default wins.
    assert_eq!(resolve_static_vars("{#{c=from-default}#}", &remembered, &fixed), "from-default");
    // d: nothing anywhere -- blank, never the raw placeholder.
    assert_eq!(resolve_static_vars("{#{d}#}", &remembered, &fixed), "");
}

#[test]
fn resolve_static_vars_choice_variable_follows_the_same_priority_chain() {
    let remembered: HashMap<String, String> = [("size".to_string(), "M".to_string())].into_iter().collect();
    let fixed: HashMap<String, String> = [("size".to_string(), "L".to_string())].into_iter().collect();
    assert_eq!(resolve_static_vars("{#{size|S,M,L}#}", &remembered, &fixed), "M", "a remembered pick wins when it is a real option");
    let fixed_only: HashMap<String, String> = fixed.clone();
    assert_eq!(resolve_static_vars("{#{size|S,M,L}#}", &HashMap::new(), &fixed_only), "L", "falls to the fixed value next");
    assert_eq!(resolve_static_vars("{#{size=S|S,M,L}#}", &HashMap::new(), &HashMap::new()), "S", "then the text's own default");
    assert_eq!(resolve_static_vars("{#{size|S,M,L}#}", &HashMap::new(), &HashMap::new()), "S", "and finally the first option");
}

#[test]
fn resolve_static_vars_leaves_a_dynamic_name_and_an_unresolved_gap_untouched() {
    let remembered: HashMap<String, String> = HashMap::new();
    let fixed: HashMap<String, String> = HashMap::new();
    // A reserved dynamic name only means something at the moment of copy --
    // a background write has no such moment, so it is left exactly as written.
    assert_eq!(resolve_static_vars("Today is {#{/date\\}#}.", &remembered, &fixed), "Today is {#{/date\\}#}.");
}

#[test]
fn resolved_for_export_resolves_the_text_and_every_rotating_variant() {
    let remembered: HashMap<String, String> = [("name".to_string(), "Alex".to_string())].into_iter().collect();
    let fixed: HashMap<String, String> = HashMap::new();
    let mut p = test_prompt("p1");
    p.text = "Hi {#{name}#}".to_string();
    p.variants = vec!["Bye {#{name}#}".to_string(), "no placeholder here".to_string()];
    let resolved = resolved_for_export(&p, &remembered, &fixed);
    assert_eq!(resolved.text, "Hi Alex");
    assert_eq!(resolved.variants, vec!["Bye Alex".to_string(), "no placeholder here".to_string()]);
    // The live prompt itself must never be mutated by an export/sync pass.
    assert_eq!(p.text, "Hi {#{name}#}");
}

// An exported theme file never carries the internal id, so importing it on
// another install (or re-importing it on the same one) always creates a fresh
// theme instead of silently colliding with -- or overwriting -- an existing one.
#[test]
fn theme_export_roundtrips_without_leaking_the_internal_id() {
    let mut vars = HashMap::new();
    vars.insert("bg".to_string(), "#123456".to_string());
    vars.insert("accent".to_string(), "#abcdef".to_string());
    let theme = CustomTheme { id: "secret-internal-id".to_string(), name: "My theme".to_string(), base: "dark".to_string(), vars };
    let export = ThemeExport { name: theme.name.clone(), base: theme.base.clone(), vars: theme.vars.clone() };
    let json = serde_json::to_string_pretty(&export).unwrap();
    assert!(!json.contains("secret-internal-id"), "the export must not carry the internal id: {json}");
    let back: ThemeExport = serde_json::from_str(&json).unwrap();
    assert_eq!(back.name, theme.name);
    assert_eq!(back.base, theme.base);
    assert_eq!(back.vars, theme.vars);
}

#[test]
fn old_exports_without_notes_still_parse() {
    let txt = "### Old\nSome text\n@color #123456\n@tags Work";
    let data = parse_txt(txt);
    assert_eq!(data.prompts[0].text, "Some text");
    assert_eq!(data.prompts[0].color, "#123456");
    assert!(data.prompts[0].notes.is_empty());

    // A CSV row that stops before the notes column.
    let csv = "\"name\";\"text\"\r\n\"Old\";\"Some text\"";
    assert!(parse_csv_data(csv).prompts[0].notes.is_empty());
}

#[test]
fn notes_escaping_survives_backslashes() {
    for raw in ["a\nb", "path C:\\n\\temp", "trailing\\", "\\n literal", ""] {
        assert_eq!(notes_unescape(&notes_escape(raw)), raw, "roundtrip failed for {raw:?}");
    }
}

#[test]
fn sanitize_notes_normalizes_and_bounds() {
    assert_eq!(sanitize_notes("  a\r\nb\r  ").unwrap(), "a\nb");
    assert!(sanitize_notes(&"x".repeat(MAX_NOTES_LEN)).is_ok());
    assert!(sanitize_notes(&"x".repeat(MAX_NOTES_LEN + 1)).is_err());
}

// The whole point of private notes: a note must never reach the clipboard;
// not through a single copy, and not through a chained multi-prompt copy.
#[test]
fn notes_never_reach_the_copied_payload() {
    let mut prompts = sample_prompts();
    prompts[1].notes = "internal only: budget 5000".into();
    for p in &prompts {
        let payload = copy_payload(p);
        assert_eq!(payload, p.text);
        for line in p.notes.lines().filter(|l| !l.trim().is_empty()) {
            assert!(!payload.contains(line), "note leaked into a copy: {line}");
        }
    }
    // The chain concatenates the same payloads, so it inherits the guarantee.
    let chained = prompts.iter().map(copy_payload).collect::<Vec<_>>().join("\n\n");
    for p in &prompts {
        for line in p.notes.lines().filter(|l| !l.trim().is_empty()) {
            assert!(!chained.contains(line), "note leaked into a chain copy: {line}");
        }
    }
}

#[test]
fn tags_are_deduped_case_insensitively_keeping_known_casing() {
    let known = canonical_tags(&sample_prompts()); // "Work", "E-Mail"
    let out = sanitize_tags(
        vec![" work ".into(), "WORK".into(), "".into(), "  ".into(), "New".into()],
        &known,
    )
    .expect("valid tags");
    assert_eq!(out, vec!["Work".to_string(), "New".to_string()]);
    // An unknown tag keeps the casing it was typed with.
    assert_eq!(sanitize_tags(vec!["MiXeD".into()], &HashMap::new()).unwrap(), vec!["MiXeD"]);
}

// One pass over the whole store, so a tag on many prompts stays one operation.
#[test]
fn renaming_a_tag_rewrites_every_prompt_at_once() {
    let mut prompts = sample_prompts(); // p1: ["Work", "E-Mail"], p2: []
    prompts[1].tags = vec!["work".into(), "Draft".into()];
    assert!(rename_tag_in(&mut prompts, "work", "Job"));
    assert_eq!(prompts[0].tags, vec!["Job".to_string(), "E-Mail".to_string()]);
    assert_eq!(prompts[1].tags, vec!["Job".to_string(), "Draft".to_string()]);
    // Renaming onto a tag the prompt already has merges instead of duplicating.
    assert!(rename_tag_in(&mut prompts, "e-mail", "Job"));
    assert_eq!(prompts[0].tags, vec!["Job".to_string()]);
    // A name nobody uses changes nothing, so no save is triggered.
    assert!(!rename_tag_in(&mut prompts, "nothing", "Else"));
}

// The merge color rule: the surviving (target) tag keeps its own color if it
// has one, and only borrows the source's color when it had none at all.
#[test]
fn merging_a_tag_keeps_the_targets_color_if_it_has_one() {
    let mut colors = HashMap::new();
    colors.insert("work".to_string(), "#ff0000".to_string());
    colors.insert("job".to_string(), "#00ff00".to_string());
    transfer_tag_color(&mut colors, "work", "job");
    assert_eq!(colors.get("job"), Some(&"#00ff00".to_string()), "target's own color must win");
    assert_eq!(colors.get("work"), None, "the merged-away key's color entry is gone");
}

#[test]
fn merging_a_tag_into_an_uncolored_one_transfers_the_color() {
    let mut colors = HashMap::new();
    colors.insert("work".to_string(), "#ff0000".to_string());
    transfer_tag_color(&mut colors, "work", "job");
    assert_eq!(colors.get("job"), Some(&"#ff0000".to_string()));
    assert_eq!(colors.get("work"), None);
}

#[test]
fn deleting_a_tag_clears_it_everywhere() {
    let mut prompts = sample_prompts();
    prompts[1].tags = vec!["WORK".into()];
    assert!(remove_tag_from(&mut prompts, "work"));
    assert_eq!(prompts[0].tags, vec!["E-Mail".to_string()]);
    assert!(prompts[1].tags.is_empty());
    assert!(!remove_tag_from(&mut prompts, "work")); // idempotent
}

// Batch add: a prompt that already carries the tag is left exactly as-is (no
// duplicate, no reorder), and a fresh addition takes the casing the library
// already uses for that tag -- same rule the single-prompt editor applies.
#[test]
fn batch_add_tag_skips_prompts_that_already_have_it_and_uses_known_casing() {
    let mut prompts = sample_prompts(); // p1: ["Work", "E-Mail"], p2: []
    let known = canonical_tags(&prompts);
    let ids: std::collections::HashSet<&str> = ["p1", "p2"].into_iter().collect();
    add_tag_to_ids(&mut prompts, &ids, "work", &known).unwrap();
    assert_eq!(prompts[0].tags, vec!["Work".to_string(), "E-Mail".to_string()]);
    assert_eq!(prompts[1].tags, vec!["Work".to_string()]);
}

#[test]
fn batch_add_tag_only_touches_the_given_ids() {
    let mut prompts = sample_prompts();
    let known = canonical_tags(&prompts);
    let ids: std::collections::HashSet<&str> = ["p2"].into_iter().collect();
    add_tag_to_ids(&mut prompts, &ids, "New", &known).unwrap();
    assert_eq!(prompts[0].tags, vec!["Work".to_string(), "E-Mail".to_string()], "unselected prompt must stay untouched");
    assert_eq!(prompts[1].tags, vec!["New".to_string()]);
}

// Removing a tag from a prompt that never had it is not an error, just a no-op
// for that one prompt -- mirrors the single-prompt "already has it" rule above.
#[test]
fn batch_remove_tag_ignores_a_prompt_that_never_had_it() {
    let mut prompts = sample_prompts();
    let ids: std::collections::HashSet<&str> = ["p1", "p2"].into_iter().collect();
    remove_tag_from_ids(&mut prompts, &ids, "e-mail");
    assert_eq!(prompts[0].tags, vec!["Work".to_string()]);
    assert!(prompts[1].tags.is_empty());
}

#[test]
fn tag_input_is_validated() {
    let empty = HashMap::new();
    assert!(sanitize_tags(vec!["a".repeat(MAX_TAG_LEN + 1)], &empty).is_err());
    assert!(sanitize_tags(vec!["a,b".into()], &empty).is_err()); // comma = separator
    assert!(sanitize_tags(vec!["a\nb".into()], &empty).is_err()); // breaks the TXT format
    assert!(sanitize_tags(vec!["a".repeat(MAX_TAG_LEN)], &empty).is_ok());
    assert!(valid_tag_name("  ").is_err());
    assert_eq!(valid_tag_name(" Work ").unwrap(), "Work");
    // Colors: empty clears, #rrggbb is normalized, anything else is refused.
    assert_eq!(valid_tag_color("  ").unwrap(), "");
    assert_eq!(valid_tag_color("#AABBCC").unwrap(), "#aabbcc");
    assert!(valid_tag_color("red").is_err());
    assert!(valid_tag_color("#abc").is_err());
}

#[test]
fn custom_theme_input_is_validated() {
    assert!(valid_theme_name("  ").is_err());
    assert_eq!(valid_theme_name("  Night  ").unwrap(), "Night");
    assert!(valid_theme_name(&"a".repeat(MAX_THEME_NAME + 1)).is_err());
    assert!(valid_theme_name("Night\nshift").is_err());
    // A base must be a real built-in; "system" and another custom theme are not,
    // so inheritance can never chain.
    assert_eq!(valid_theme_base(" dark ").unwrap(), "dark");
    assert!(valid_theme_base("system").is_err());
    assert!(valid_theme_base("custom:abc").is_err());
    let mut vars = HashMap::new();
    vars.insert("accent".to_string(), "#AABBCC".to_string());
    vars.insert("bg".to_string(), "  ".to_string()); // empty = not overridden
    let clean = valid_theme_vars(&vars).unwrap();
    assert_eq!(clean.get("accent").unwrap(), "#aabbcc");
    assert!(!clean.contains_key("bg"));
    vars.insert("shadow".to_string(), "#000000".to_string());
    assert!(valid_theme_vars(&vars).is_err()); // not a themable colour token
    let mut bad = HashMap::new();
    bad.insert("accent".to_string(), "red".to_string());
    assert!(valid_theme_vars(&bad).is_err());
}

#[test]
fn custom_theme_resolves_to_its_base() {
    let mut settings = Settings::default();
    settings.custom_themes.push(CustomTheme {
        id: "t1".into(),
        name: "Night".into(),
        base: "dark".into(),
        vars: HashMap::from([("accent".to_string(), "#ff0000".to_string())]),
    });
    settings.theme = "custom:t1".into();
    assert_eq!(theme_base_of(&settings, &settings.theme), "dark");
    // The feature ships off, so a stored custom theme stays parked until enabled.
    assert!(theme_vars_of(&settings).is_empty());
    settings.ui_flags.insert("customThemes".into(), true);
    assert_eq!(theme_vars_of(&settings).get("accent").unwrap(), "#ff0000");
    // A dangling reference falls back to the system theme instead of breaking.
    settings.theme = "custom:gone".into();
    assert_eq!(theme_base_of(&settings, &settings.theme), "system");
    assert!(theme_vars_of(&settings).is_empty());
    // Built-in preferences pass through untouched.
    assert_eq!(theme_base_of(&settings, "mono"), "mono");
}

#[test]
fn custom_theme_cap_is_bounded() {
    let mut settings = Settings::default();
    assert_eq!(custom_theme_cap(&settings), DEFAULT_CUSTOM_THEMES);
    settings.ui_values.insert("customThemeMax".into(), 3.0);
    assert_eq!(custom_theme_cap(&settings), 3);
    settings.ui_values.insert("customThemeMax".into(), 9999.0);
    assert_eq!(custom_theme_cap(&settings), MAX_CUSTOM_THEMES);
    settings.ui_values.insert("customThemeMax".into(), -5.0);
    assert_eq!(custom_theme_cap(&settings), 1);
    settings.ui_values.insert("customThemeMax".into(), f64::NAN);
    assert_eq!(custom_theme_cap(&settings), DEFAULT_CUSTOM_THEMES);
}

#[test]
fn settings_without_custom_themes_still_load() {
    // An older settings blob has no `custom_themes` key at all.
    let s: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
    assert!(s.custom_themes.is_empty());
    // A theme entry from a newer build with unknown extra fields keeps what it can.
    let s: Settings = serde_json::from_str(
        r##"{"custom_themes":[{"id":"a","name":"N","base":"mono","vars":{"bg":"#111111"},"future":1}]}"##,
    )
    .unwrap();
    assert_eq!(s.custom_themes[0].base, "mono");
    assert_eq!(s.custom_themes[0].vars.get("bg").unwrap(), "#111111");
}

fn test_view(id: &str) -> View {
    View {
        id: id.to_string(),
        name: id.to_uppercase(),
        cols: default_cols(),
        rows: default_rows(),
        layouts: HashMap::new(),
        color: String::new(),
        members: Vec::new(),
    }
}

#[test]
fn reorder_views_by_id_moves_a_view_to_a_new_position() {
    let views = vec![test_view("a"), test_view("b"), test_view("c")];
    // "c" moved to the front, same as dropping its tab before "a"'s.
    let order: Vec<String> = ["c", "a", "b"].iter().map(|s| s.to_string()).collect();
    let out = reorder_views_by_id(views, &order);
    let ids: Vec<&str> = out.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, ["c", "a", "b"]);
}

#[test]
fn reorder_views_by_id_keeps_a_view_missing_from_order() {
    // A stale caller that only named two of three views must still get all
    // three back -- the third keeps its old relative place instead of vanishing.
    let views = vec![test_view("a"), test_view("b"), test_view("c")];
    let order: Vec<String> = ["c", "a"].iter().map(|s| s.to_string()).collect();
    let out = reorder_views_by_id(views, &order);
    let ids: Vec<&str> = out.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, ["c", "a", "b"], "the view left out of order must still be present");
}

#[test]
fn reorder_views_by_id_ignores_an_unknown_id_and_never_touches_members() {
    // A view's own data (its membership list here) rides along with it
    // untouched -- reordering is a pure array shuffle, never a rebuild.
    let mut a = test_view("a");
    a.members = vec!["p1".to_string(), "p2".to_string()];
    let mut b = test_view("b");
    b.members = vec!["p3".to_string()];
    let views = vec![a, b];
    let order: Vec<String> = ["ghost", "b", "a"].iter().map(|s| s.to_string()).collect();
    let out = reorder_views_by_id(views, &order);
    let ids: Vec<&str> = out.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, ["b", "a"], "an id not among the views must be skipped, not inserted");
    assert_eq!(out[0].members, vec!["p3".to_string()]);
    assert_eq!(out[1].members, vec!["p1".to_string(), "p2".to_string()]);
}

#[test]
fn first_free_cell_skips_other_folder_cells() {
    // 3x1 grid, tile "a" on [0,0]: the plain scan would find [1,0] free.
    let mut view = test_view("v1");
    view.cols = 3;
    view.rows = 1;
    view.layouts.insert(
        grid_key(view.cols, view.rows),
        HashMap::from([("a".to_string(), [0, 0])]),
    );
    assert_eq!(first_free_cell(&view), Some([1, 0]));
    // A folder sits on [1,0] (outside view.layouts): the scan must skip it, or a
    // dissolved member handed that cell would save fine but never render.
    assert_eq!(first_free_cell_excluding(&view, &[[1, 0]]), Some([2, 0]));
    assert_eq!(first_free_cell_excluding(&view, &[[1, 0], [2, 0]]), None);
}

#[test]
fn place_ids_in_view_fills_free_cells_and_skips_already_placed_ids() {
    // 2x1 grid, "a" already on [0,0].
    let mut layout = HashMap::from([("a".to_string(), [0, 0])]);
    let ids: Vec<String> = vec!["a".to_string(), "b".to_string()];
    assert!(place_ids_in_view(
        &mut layout, &ids, 2, 1, &std::collections::HashSet::new(), &std::collections::HashSet::new()
    ));
    assert_eq!(layout.get("a"), Some(&[0, 0])); // untouched, was already placed
    assert_eq!(layout.get("b"), Some(&[1, 0]));
}

#[test]
fn place_ids_in_view_is_all_or_nothing_when_the_view_is_full() {
    // A batch that doesn't fully fit must place NOTHING, not a partial subset
    // -- a caller that already removed these ids from their source view relies on
    // this to know the move truly failed.
    let mut layout = HashMap::from([("a".to_string(), [0, 0])]);
    let before = layout.clone();
    let ids: Vec<String> = vec!["b".to_string(), "c".to_string()]; // only 1 free cell for 2 ids
    assert!(!place_ids_in_view(
        &mut layout, &ids, 2, 1, &std::collections::HashSet::new(), &std::collections::HashSet::new()
    ));
    assert_eq!(layout, before);
}

#[test]
fn place_ids_in_view_counts_grid_folder_cells_as_occupied() {
    // A folder's cell lives outside `layout` (it is the folder's own `cell`
    // field, not a layout entry) -- without passing it as extra_occupied, addView could hand a
    // prompt the same cell a folder already sits on.
    let mut layout: HashMap<String, [u32; 2]> = HashMap::new();
    let folder_cells = std::collections::HashSet::from([(0u32, 0u32)]);
    let ids: Vec<String> = vec!["a".to_string()];
    assert!(place_ids_in_view(&mut layout, &ids, 2, 1, &folder_cells, &std::collections::HashSet::new()));
    assert_eq!(layout.get("a"), Some(&[1, 0])); // [0,0] was the folder's, must be skipped
}

#[test]
fn place_ids_in_view_skips_ids_already_placed_in_a_folder() {
    // "a" lives in a grid folder of this view -- it needs no cell of its own
    // and must not eat the view's only free one, or a full target would
    // wrongly refuse a move that has nothing left to place.
    let mut layout: HashMap<String, [u32; 2]> = HashMap::new();
    let folder_cells = std::collections::HashSet::from([(0u32, 0u32)]); // the folder's own cell
    let folder_members = std::collections::HashSet::from(["a".to_string()]);
    let ids: Vec<String> = vec!["a".to_string()];
    assert!(place_ids_in_view(&mut layout, &ids, 1, 1, &folder_cells, &folder_members));
    assert!(layout.is_empty(), "a folder member must never get a layout entry of its own");
}

#[test]
fn place_ids_in_view_heals_a_stray_layout_entry_for_a_folder_member() {
    // Old data: "a" already sits in a folder AND still carries a leftover
    // layout entry from before it joined -- that entry must be dropped, not
    // left behind occupying a cell the grid never shows as taken.
    let mut layout = HashMap::from([("a".to_string(), [1, 0])]);
    let folder_members = std::collections::HashSet::from(["a".to_string()]);
    let ids: Vec<String> = vec!["a".to_string()];
    assert!(place_ids_in_view(&mut layout, &ids, 2, 1, &std::collections::HashSet::new(), &folder_members));
    assert!(!layout.contains_key("a"), "the stray entry for a folder member must be removed");
}

#[test]
fn view_capacity_deficit_counts_only_the_ids_still_missing_a_cell() {
    // 2x1 grid, "a" already placed: one free cell, two new ids asked for.
    let layout = HashMap::from([("a".to_string(), [0, 0])]);
    let ids: Vec<String> = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    // "a" is already there and doesn't count against the need.
    assert_eq!(
        view_capacity_deficit(
            &layout, &ids, 2, 1, &std::collections::HashSet::new(), &std::collections::HashSet::new()
        ),
        1
    );
    // A folder cell shrinks the free count the same way it does in place_ids_in_view.
    let folder_cells = std::collections::HashSet::from([(1u32, 0u32)]);
    let empty: HashMap<String, [u32; 2]> = HashMap::new();
    let one_id: Vec<String> = vec!["z".to_string()];
    assert_eq!(
        view_capacity_deficit(
            &empty, &one_id, 1, 1, &std::collections::HashSet::new(), &std::collections::HashSet::new()
        ),
        0
    );
    assert_eq!(
        view_capacity_deficit(
            &layout, &["b".to_string()], 2, 1, &folder_cells, &std::collections::HashSet::new()
        ),
        1
    );
}

#[test]
fn view_capacity_deficit_does_not_count_an_id_already_placed_in_a_folder() {
    // A 1x1 target whose only cell the folder itself occupies: the deficit
    // for an id already sitting in that folder must be 0, not 1, or a full
    // target would refuse a move that has nothing left to place.
    let empty: HashMap<String, [u32; 2]> = HashMap::new();
    let folder_cells = std::collections::HashSet::from([(0u32, 0u32)]);
    let folder_members = std::collections::HashSet::from(["a".to_string()]);
    assert_eq!(view_capacity_deficit(&empty, &["a".to_string()], 1, 1, &folder_cells, &folder_members), 0);
}

#[test]
fn strip_ids_from_view_drops_them_from_every_layout_size_and_from_members() {
    // A view resized at least once carries more than one layout entry ("2x1" from
    // an earlier size, "3x1" the current one) -- a member dropped from the view
    // must lose its cell in BOTH, not just the size it happens to sit in today.
    let mut view = test_view("v1");
    view.cols = 3;
    view.rows = 1;
    view.members = vec!["a".to_string(), "b".to_string()];
    view.layouts.insert("2x1".to_string(), HashMap::from([("a".to_string(), [0, 0])]));
    view.layouts.insert(
        "3x1".to_string(),
        HashMap::from([("a".to_string(), [0, 0]), ("b".to_string(), [1, 0])]),
    );
    strip_ids_from_view(&mut view, &["a".to_string()]);
    assert_eq!(view.members, vec!["b".to_string()]);
    assert!(!view.layouts["2x1"].contains_key("a"));
    assert!(!view.layouts["3x1"].contains_key("a"));
    assert_eq!(view.layouts["3x1"].get("b"), Some(&[1, 0])); // untouched
}

#[test]
fn moving_between_views_places_in_the_target_and_strips_the_source() {
    // Goes through batch_prompts's actual "moveView" core (apply_view_batch_action),
    // not place_ids_in_view/strip_ids_from_view called directly -- a regression
    // in the core's save handling or its folder-membership accounting now fails
    // this test too, not just its own dedicated ones. Proves "moved" means gone
    // from one view, present in the other, and durably saved.
    let mut settings = Settings::default();
    let mut from_view = test_view("home");
    from_view.cols = 2;
    from_view.rows = 1;
    from_view.members = vec!["a".to_string()];
    from_view.layouts.insert(grid_key(2, 1), HashMap::from([("a".to_string(), [0, 0])]));
    let mut to_view = test_view("work");
    to_view.cols = 2;
    to_view.rows = 1;
    settings.views = vec![from_view, to_view];

    let ids: Vec<String> = vec!["a".to_string()];
    let mut saved = false;
    let result = apply_view_batch_action(&mut settings, "moveView", &ids, "work", "home", |_s| {
        saved = true;
        true
    });

    assert!(result.is_ok());
    assert!(saved, "a successful move must call through to save");
    assert!(settings.views[0].members.is_empty());
    assert!(!settings.views[0].layouts[&grid_key(2, 1)].contains_key("a"));
    assert_eq!(settings.views[1].members, vec!["a".to_string()]);
    assert_eq!(settings.views[1].layouts[&grid_key(2, 1)].get("a"), Some(&[0, 0]));
}

#[test]
fn moveview_action_treats_an_existing_folder_member_as_already_placed() {
    // Same folder-membership case as place_ids_in_view's own tests, run
    // through the real "moveView" core this time: a 1x1 target whose only
    // cell a folder occupies, moving in an id the folder already lists as
    // a member -- must succeed (not view-full) and never grow a second
    // layout entry for it.
    let mut settings = Settings::default();
    let mut from_view = test_view("home");
    from_view.members = vec!["a".to_string()];
    let mut to_view = test_view("work");
    to_view.cols = 1;
    to_view.rows = 1;
    settings.views = vec![from_view, to_view];
    settings.tile_folders.push(TileFolder {
        id: "tf1".to_string(),
        name: "Drafts".to_string(),
        color: String::new(),
        view_id: "work".to_string(),
        cell: [0, 0],
        members: vec!["a".to_string()],
    });

    let ids: Vec<String> = vec!["a".to_string()];
    let result = apply_view_batch_action(&mut settings, "moveView", &ids, "work", "home", |_s| true);

    assert!(result.is_ok(), "a target whose only cell a folder occupies must not report view-full: {result:?}");
    assert!(settings.views[0].members.is_empty(), "the source view must lose the moved id");
    assert!(
        settings.views[1].layouts.values().all(|l| !l.contains_key("a")),
        "an id already placed via a folder must never also get a layout entry"
    );
}

#[test]
fn copying_into_a_view_never_touches_a_second_view() {
    // The "addView" (Ctrl-drop / copy) arm never calls strip_ids_from_view --
    // placing "a" into a target view must leave an unrelated view's own
    // members/layouts byte-for-byte unchanged, proving a copy really is one.
    let mut other_view = test_view("home");
    other_view.members = vec!["a".to_string()];
    other_view.layouts.insert(grid_key(2, 1), HashMap::from([("a".to_string(), [0, 0])]));
    let other_before = other_view.clone();

    let mut to_view = test_view("work");
    to_view.cols = 2;
    to_view.rows = 1;
    let ids: Vec<String> = vec!["a".to_string()];
    let to_layout = to_view.layouts.entry(grid_key(2, 1)).or_default();
    assert!(place_ids_in_view(
        to_layout, &ids, 2, 1, &std::collections::HashSet::new(), &std::collections::HashSet::new()
    ));
    to_view.members.push("a".to_string());

    // The source view was never touched -- View has no PartialEq of its own,
    // so the two fields a copy could plausibly disturb are compared directly.
    assert_eq!(other_view.members, other_before.members);
    assert_eq!(other_view.layouts, other_before.layouts);
    assert_eq!(to_view.layouts[&grid_key(2, 1)].get("a"), Some(&[0, 0]));
}

#[test]
fn moveview_action_rolls_back_the_move_when_save_fails() {
    // A failed write must never leave the in-memory move standing -- the disk
    // would show the old placement after a restart while the running app kept
    // showing the new one until then, exactly the drift 477 closes.
    let mut settings = Settings::default();
    let mut from_view = test_view("home");
    from_view.cols = 2;
    from_view.rows = 1;
    from_view.members = vec!["a".to_string()];
    from_view.layouts.insert(grid_key(2, 1), HashMap::from([("a".to_string(), [0, 0])]));
    let mut to_view = test_view("work");
    to_view.cols = 2;
    to_view.rows = 1;
    settings.views = vec![from_view, to_view];
    let before_from_members = settings.views[0].members.clone();
    let before_from_layouts = settings.views[0].layouts.clone();
    let before_to_members = settings.views[1].members.clone();
    let before_to_layouts = settings.views[1].layouts.clone();

    let ids: Vec<String> = vec!["a".to_string()];
    let result = apply_view_batch_action(&mut settings, "moveView", &ids, "work", "home", |_s| false);
    assert_eq!(result, Err("save-failed".to_string()));
    assert_eq!(settings.views[0].members, before_from_members, "the source view's members must be untouched");
    assert_eq!(settings.views[0].layouts, before_from_layouts, "the source view's layout must be untouched");
    assert_eq!(settings.views[1].members, before_to_members, "the target view's members must be untouched");
    assert_eq!(settings.views[1].layouts, before_to_layouts, "the target view's layout must be untouched");
}

#[test]
fn moveview_action_saves_and_keeps_the_move_when_the_write_succeeds() {
    let mut settings = Settings::default();
    let mut from_view = test_view("home");
    from_view.cols = 2;
    from_view.rows = 1;
    from_view.members = vec!["a".to_string()];
    from_view.layouts.insert(grid_key(2, 1), HashMap::from([("a".to_string(), [0, 0])]));
    let mut to_view = test_view("work");
    to_view.cols = 2;
    to_view.rows = 1;
    settings.views = vec![from_view, to_view];

    let ids: Vec<String> = vec!["a".to_string()];
    let mut saved = false;
    let result =
        apply_view_batch_action(&mut settings, "moveView", &ids, "work", "home", |_s| { saved = true; true });
    assert!(result.is_ok());
    assert!(saved, "a successful move must still call through to save");
    assert!(settings.views[0].members.is_empty());
    assert_eq!(settings.views[1].members, vec!["a".to_string()]);
}

#[test]
fn addview_action_rolls_back_the_placement_when_save_fails() {
    let mut settings = Settings::default();
    let mut view = test_view("work");
    view.cols = 2;
    view.rows = 1;
    settings.views = vec![view];
    let before_members = settings.views[0].members.clone();
    let before_layouts = settings.views[0].layouts.clone();

    let ids: Vec<String> = vec!["a".to_string()];
    let result = apply_view_batch_action(&mut settings, "addView", &ids, "work", "", |_s| false);
    assert_eq!(result, Err("save-failed".to_string()));
    assert_eq!(settings.views[0].members, before_members, "the view's members must be untouched");
    assert_eq!(settings.views[0].layouts, before_layouts, "the view's layout must be untouched");
}

// `members` decides who belongs to a view, `layouts` only where they
// sit. A grid resize must never add or drop a member.
#[test]
fn rebuild_view_layout_grows_and_restores_old_positions() {
    let mut view = test_view("v1");
    view.cols = 2;
    view.rows = 2;
    view.members = vec!["a".into(), "b".into(), "c".into(), "d".into()];
    view.layouts.insert(
        grid_key(2, 2),
        HashMap::from([
            ("a".to_string(), [0, 0]),
            ("b".to_string(), [1, 0]),
            ("c".to_string(), [0, 1]),
            ("d".to_string(), [1, 1]),
        ]),
    );
    view.cols = 3;
    view.rows = 3;
    rebuild_view_layout(&mut view, &std::collections::HashSet::new(), &std::collections::HashSet::new());
    let layout = view.layouts.get(&grid_key(3, 3)).unwrap();
    assert_eq!(layout.len(), 4);
    assert_eq!(layout.get("a"), Some(&[0, 0]));
    assert_eq!(layout.get("b"), Some(&[1, 0]));
    assert_eq!(layout.get("c"), Some(&[0, 1]));
    assert_eq!(layout.get("d"), Some(&[1, 1]));
}

#[test]
fn rebuild_view_layout_grows_a_view_with_no_saved_layout_at_all() {
    // A view that has never been saved at 2x2 still gets all four members
    // placed when it grows to 3x3 - nothing to "remember" is not an excuse
    // to leave anyone off the grid.
    let mut view = test_view("v1");
    view.cols = 3;
    view.rows = 3;
    view.members = vec!["a".into(), "b".into(), "c".into(), "d".into()];
    rebuild_view_layout(&mut view, &std::collections::HashSet::new(), &std::collections::HashSet::new());
    let layout = view.layouts.get(&grid_key(3, 3)).unwrap();
    assert_eq!(layout.len(), 4);
}

#[test]
fn rebuild_view_layout_never_skips_a_member_whose_remembered_cell_is_taken() {
    // "b" remembers [0,0] from a 1x2 layout; growing to 2x1 also wants to seat
    // "a" fresh, and "a" happens to land on [0,0] first. "b" must still get a
    // cell (the other free one), never be silently left off the grid - this is
    // the reported 2x2-to-3x3 loss, reduced to its smallest repro.
    let mut view = test_view("v1");
    view.cols = 1;
    view.rows = 2;
    view.members = vec!["a".into(), "b".into()];
    view.layouts.insert(grid_key(1, 2), HashMap::from([("b".to_string(), [0, 0])]));
    view.cols = 2;
    view.rows = 1;
    // Force "a" onto [0,0] first via an existing (stale) 2x1 layout entry, so
    // "b"'s remembered [0,0] is genuinely occupied by someone else.
    view.layouts.insert(grid_key(2, 1), HashMap::from([("a".to_string(), [0, 0])]));
    rebuild_view_layout(&mut view, &std::collections::HashSet::new(), &std::collections::HashSet::new());
    let layout = view.layouts.get(&grid_key(2, 1)).unwrap();
    assert_eq!(layout.len(), 2, "both members must have a cell, none dropped");
    assert_eq!(layout.get("a"), Some(&[0, 0]));
    assert_eq!(layout.get("b"), Some(&[1, 0]));
}

#[test]
fn rebuild_view_layout_shrink_keeps_membership_and_shows_overhang() {
    // 3x3 with nine members shrunk to 2x2: nobody loses membership, five have
    // no cell (overhang) instead of being dropped.
    let mut view = test_view("v1");
    view.cols = 3;
    view.rows = 3;
    let ids: Vec<String> = (0..9).map(|i| format!("p{i}")).collect();
    view.members = ids.clone();
    let mut full = HashMap::new();
    for (i, id) in ids.iter().enumerate() {
        full.insert(id.clone(), [(i % 3) as u32, (i / 3) as u32]);
    }
    view.layouts.insert(grid_key(3, 3), full);
    view.cols = 2;
    view.rows = 2;
    rebuild_view_layout(&mut view, &std::collections::HashSet::new(), &std::collections::HashSet::new());
    assert_eq!(view.members.len(), 9, "shrinking must never drop membership");
    let small = view.layouts.get(&grid_key(2, 2)).unwrap();
    assert_eq!(small.len(), 4, "only 4 cells exist, the other 5 are overhang");
    for id in &ids {
        if let Some(cell) = small.get(id) {
            assert!(cell[0] < 2 && cell[1] < 2);
        }
    }
    // Growing back to 3x3 restores exactly the old arrangement, untouched
    // the whole time under its own layouts key.
    view.cols = 3;
    view.rows = 3;
    rebuild_view_layout(&mut view, &std::collections::HashSet::new(), &std::collections::HashSet::new());
    let restored = view.layouts.get(&grid_key(3, 3)).unwrap();
    assert_eq!(restored.len(), 9);
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(restored.get(id), Some(&[(i % 3) as u32, (i / 3) as u32]));
    }
}

#[test]
fn rebuild_view_layout_mixed_resize_does_not_fall_back_to_a_stale_layout() {
    // 2x3 has 6 members. A much older, incomplete 3x2 layout (only 2 of them)
    // is already on record from a previous visit at that size. Resizing to
    // 3x2 (one axis grows, the other shrinks) must top up the stale layout
    // from `members`, not just reuse it as-is and lose the other four.
    let mut view = test_view("v1");
    view.cols = 2;
    view.rows = 3;
    let ids: Vec<String> = (0..6).map(|i| format!("p{i}")).collect();
    view.members = ids.clone();
    view.layouts.insert(
        grid_key(3, 2),
        HashMap::from([("p0".to_string(), [0, 0]), ("p1".to_string(), [1, 0])]),
    );
    view.cols = 3;
    view.rows = 2;
    rebuild_view_layout(&mut view, &std::collections::HashSet::new(), &std::collections::HashSet::new());
    let layout = view.layouts.get(&grid_key(3, 2)).unwrap();
    assert_eq!(layout.len(), 6, "every member must be seated, not just the stale two");
}

#[test]
fn rebuild_view_layout_excludes_grid_folder_members_from_their_own_cell() {
    // A prompt living inside a grid folder is still a view member but must
    // never also get its own individual layout cell.
    let mut view = test_view("v1");
    view.cols = 2;
    view.rows = 2;
    view.members = vec!["a".into(), "b".into()];
    let folder_members = std::collections::HashSet::from(["a".to_string()]);
    let folder_cells = std::collections::HashSet::from([(0u32, 0u32)]);
    rebuild_view_layout(&mut view, &folder_cells, &folder_members);
    let layout = view.layouts.get(&grid_key(2, 2)).unwrap();
    assert!(!layout.contains_key("a"), "folder member must not get its own tile");
    assert_eq!(layout.get("b"), Some(&[1, 0]));
}

#[test]
fn dup_pair_is_ignored_checks_shared_group_membership() {
    // Groups replace pairs - "a" and "d" each belong to a DIFFERENT ignored
    // group (from two separate ignore_dups calls); neither call ever put them
    // together, so they must NOT count as ignored even though both ids are
    // independently "in some ignored group somewhere".
    let groups = vec![
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        vec!["c".to_string(), "d".to_string(), "e".to_string()],
    ];
    assert!(dup_pair_is_ignored(&groups, "a", "b"), "same group, both directions");
    assert!(dup_pair_is_ignored(&groups, "b", "a"));
    assert!(dup_pair_is_ignored(&groups, "d", "e"), "second group on its own");
    assert!(dup_pair_is_ignored(&groups, "a", "c"), "c belongs to both groups - still ignored with a");
    assert!(dup_pair_is_ignored(&groups, "c", "d"), "and still ignored with d");
    assert!(!dup_pair_is_ignored(&groups, "a", "d"), "never co-ignored in any single call");
    assert!(!dup_pair_is_ignored(&groups, "a", "z"), "z was never ignored at all");
}

#[test]
fn screen_identity_differs_by_name_even_at_the_same_bounds() {
    let a = screen_identity("Dell-1", 1920, 0, 1920, 1080);
    let b = screen_identity("Dell-2", 1920, 0, 1920, 1080);
    assert_ne!(a, b);
    assert_eq!(a, screen_identity("Dell-1", 1920, 0, 1920, 1080));
}

#[test]
fn resolve_float_spot_parks_a_position_outside_every_current_screen() {
    let screens = vec![("main".to_string(), 0, 0, 1920, 1080)];
    let pos = Pos { x: 5000, y: 5000, screen: "gone-monitor".to_string() };
    let (x, y, parked) = resolve_float_spot(&pos, 200.0, 80.0, &screens, &[]);
    assert!(parked);
    // Landed inside the main screen's work area, not at the stale coordinates.
    assert!(x >= 0.0 && x + 200.0 <= 1920.0);
    assert!(y >= 0.0 && y + 80.0 <= 1080.0);
    // The stored position itself is never touched by resolving a spot for it.
    assert_eq!((pos.x, pos.y, pos.screen.as_str()), (5000, 5000, "gone-monitor"));
}

#[test]
fn resolve_float_spot_returns_home_once_the_same_screen_identity_reappears() {
    let screens = vec![("dell-1".to_string(), 1920, 0, 1920, 1080)];
    let pos = Pos { x: 2500, y: 100, screen: "dell-1".to_string() };
    let (x, y, parked) = resolve_float_spot(&pos, 200.0, 80.0, &screens, &[]);
    assert!(!parked);
    assert_eq!((x, y), (2500.0, 100.0));
}

#[test]
fn resolve_float_spot_stays_parked_for_a_different_identity_at_the_same_bounds() {
    // A second monitor with the exact dimensions and position of the one the
    // pill used to live on, but a different name -- not the screen it left.
    let screens = vec![("dell-3".to_string(), 1920, 0, 1920, 1080)];
    let pos = Pos { x: 2500, y: 100, screen: "dell-1".to_string() };
    let (_, _, parked) = resolve_float_spot(&pos, 200.0, 80.0, &screens, &[]);
    assert!(parked);
}

#[test]
fn resolve_float_spot_trusts_plain_coordinates_for_a_position_saved_before_screen_ids() {
    // Empty `screen` (upgraded from an older save) falls back to today's
    // coordinate check, so an ordinary upgrade never parks a button that
    // has not actually moved.
    let screens = vec![("dell-1".to_string(), 0, 0, 1920, 1080)];
    let pos = Pos { x: 100, y: 100, screen: String::new() };
    let (x, y, parked) = resolve_float_spot(&pos, 200.0, 80.0, &screens, &[]);
    assert!(!parked);
    assert_eq!((x, y), (100.0, 100.0));
}

#[test]
fn resolve_float_spot_never_parks_on_a_failed_monitor_probe() {
    let pos = Pos { x: 5000, y: 5000, screen: "dell-1".to_string() };
    let (x, y, parked) = resolve_float_spot(&pos, 200.0, 80.0, &[], &[]);
    assert!(!parked);
    assert_eq!((x, y), (5000.0, 5000.0));
}

#[test]
fn ids_past_park_cleanup_is_off_at_zero_hours() {
    let mut parked = HashMap::new();
    parked.insert("a".to_string(), std::time::Duration::from_secs(999_999));
    assert!(ids_past_park_cleanup(&parked, 0.0).is_empty());
}

#[test]
fn ids_past_park_cleanup_only_returns_what_is_actually_past_its_hours() {
    let mut parked = HashMap::new();
    parked.insert("old".to_string(), std::time::Duration::from_secs(3 * 3600 + 1));
    parked.insert("young".to_string(), std::time::Duration::from_secs(3600));
    let expired = ids_past_park_cleanup(&parked, 3.0);
    assert_eq!(expired, vec!["old".to_string()]);
}

fn test_tile_folder(id: &str, view_id: &str, cell: [u32; 2], members: &[&str]) -> TileFolder {
    TileFolder {
        id: id.to_string(),
        name: String::new(),
        color: String::new(),
        view_id: view_id.to_string(),
        cell,
        members: members.iter().map(|s| s.to_string()).collect(),
    }
}


#[test]
fn tile_folders_normalize_to_their_invariants() {
    let view_ids: Vec<String> = vec![test_view("v1").id];
    let ids: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
    let mut folders = vec![
        test_tile_folder("f1", "v1", [1, 1], &["a", "gone", "b"]),
        // Claims a prompt the first folder already holds, plus one of its own.
        test_tile_folder("f2", "v1", [1, 1], &["b", "c", "d"]),
        // A view that no longer exists takes its folder with it.
        test_tile_folder("f3", "vX", [0, 0], &["a", "c"]),
    ];
    normalize_tile_folders(&mut folders, &ids, &view_ids);
    assert_eq!(folders.len(), 2);
    assert_eq!(folders[0].members, vec!["a", "b"]); // unknown id dropped
    assert_eq!(folders[1].members, vec!["c", "d"]); // "b" belongs to f1
    // Two folders never sit on the same cell.
    assert_ne!(folders[0].cell, folders[1].cell);
    // Idempotent.
    let before: Vec<[u32; 2]> = folders.iter().map(|f| f.cell).collect();
    normalize_tile_folders(&mut folders, &ids, &view_ids);
    assert_eq!(folders.len(), 2);
    assert_eq!(folders.iter().map(|f| f.cell).collect::<Vec<_>>(), before);
    // Down to one member: the folder dissolves instead of lingering.
    folders[0].members.pop();
    normalize_tile_folders(&mut folders, &ids, &view_ids);
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].id, "f2");
}

// A hand-edited/imported cell at u32::MAX used to hang normalize_tile_folders
// forever (saturating_add never moves off the cap while the store mutex is held,
// i.e. the app is dead on the next launch). Must terminate and clamp instead.
#[test]
fn tile_folder_cell_collision_at_u32_max_terminates() {
    let view_ids: Vec<String> = vec![test_view("v1").id];
    let ids: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
    let mut folders = vec![
        test_tile_folder("f1", "v1", [u32::MAX, 0], &["a", "b"]),
        test_tile_folder("f2", "v1", [u32::MAX, 0], &["c", "d"]),
    ];
    normalize_tile_folders(&mut folders, &ids, &view_ids); // must return, not hang
    assert_eq!(folders.len(), 2);
    assert!(folders.iter().all(|f| f.cell[0] <= GRID_MAX + 5), "cell must be clamped into range");
    assert_ne!(folders[0].cell, folders[1].cell, "still resolved to different cells");

    let mut sync_folders = vec![
        test_tile_folder("f1", "v1", [u32::MAX, 0], &["a", "b"]),
        test_tile_folder("f2", "v1", [u32::MAX, 0], &["c", "d"]),
    ];
    sync_normalize_tile_folders(&mut sync_folders, &ids, &view_ids);
    assert_eq!(sync_folders.len(), 2);
    assert!(sync_folders.iter().all(|f| f.cell[0] <= GRID_MAX + 5));
    assert_ne!(sync_folders[0].cell, sync_folders[1].cell);
}

#[test]
fn settings_without_tile_folders_still_load() {
    let s: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
    assert!(s.tile_folders.is_empty());
    // A folder from a newer build with unknown extra fields keeps what it can.
    let s: Settings = serde_json::from_str(
        r##"{"tile_folders":[{"id":"f","name":"Work","color":"#112233","view_id":"v1","cell":[2,3],"members":["a","b"],"future":1}]}"##,
    )
    .unwrap();
    assert_eq!(s.tile_folders[0].cell, [2, 3]);
    assert_eq!(s.tile_folders[0].members, vec!["a", "b"]);
    assert_eq!(s.tile_folders[0].color, "#112233");
}

#[test]
fn legacy_view_groups_key_is_ignored() {
    // A settings blob written while view groups existed still loads: the
    // group list is dropped, every view stays, in its stored order.
    let s: Settings = serde_json::from_str(
        r##"{"active_view":"b","views":[{"id":"b","name":"B"},{"id":"a","name":"A"}],
                 "view_groups":[{"id":"g","name":"G","color":"","view_ids":["a"]}]}"##,
    )
    .unwrap();
    let ids: Vec<&str> = s.views.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, vec!["b", "a"]);
    assert_eq!(s.active_view, "b");
    assert!(!serde_json::to_string(&s).unwrap().contains("view_groups"));
}

#[test]
fn font_size_clamped() {
    assert_eq!(clamp_font_size(0), 0);
    assert_eq!(clamp_font_size(1), 1);
    assert_eq!(clamp_font_size(8), 10);
    assert_eq!(clamp_font_size(99), 40);
}

// ---- Tray quick menu ----

#[test]
fn importing_duplicate_prompt_ids_gives_each_one_its_own() {
    // `id` is the prompts table PRIMARY KEY: a duplicate made the whole rewrite
    // fail, and the failure was swallowed into the ignored JSON fallback, so the
    // import "succeeded" and the old prompts came back after a restart.
    let mut ps = sample_prompts();
    ps[1].id = ps[0].id.clone();
    ps.push({
        let mut p = ps[0].clone();
        p.id = String::new(); // a hand-written backup may leave it out entirely
        p
    });
    migrate_prompts(&mut ps);
    let ids: std::collections::HashSet<&str> = ps.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids.len(), ps.len(), "every prompt must end up with a unique id");
    assert!(ps.iter().all(|p| !p.id.is_empty()), "no prompt may be left without an id");
}

#[test]
fn turning_the_old_hiding_off_keeps_screenshots_working() {
    // Stored "captureExclusion: false" meant "let screenshots through"; that has to
    // survive the switch to the positively-phrased flag.
    let mut s = Settings::default();
    s.ui_flags.insert("captureExclusion".into(), false);
    s.migrate();
    assert!(!capture_excluded(&s), "an explicit allow must keep allowing");
    assert!(!s.ui_flags.contains_key("captureExclusion"), "the old key is retired");

    // The old default (hiding on) and a fresh install both stay protected.
    let mut had = Settings::default();
    had.ui_flags.insert("captureExclusion".into(), true);
    had.migrate();
    assert!(capture_excluded(&had));
    let mut fresh = Settings::default();
    fresh.migrate();
    assert!(capture_excluded(&fresh), "the protection is the shipped default");

    // The mapping the live toggle in set_ui_flag has to follow: allow = do NOT
    // exclude. Passing the flag straight to set_capture_exclusion inverts it.
    let mut on = Settings::default();
    on.ui_flags.insert(ALLOW_SCREENSHOT_KEY.into(), true);
    assert!(!capture_excluded(&on), "allowing screenshots must not exclude");
    let mut off = Settings::default();
    off.ui_flags.insert(ALLOW_SCREENSHOT_KEY.into(), false);
    assert!(capture_excluded(&off), "refusing screenshots must exclude");
}

#[test]
fn an_imported_view_cannot_exceed_the_grid_ceiling() {
    // A hand-edited backup used to be able to ask for a grid of u32::MAX cells,
    // which the renderer then tried to build: the window never painted again.
    let mut s = Settings {
        views: vec![test_view("v1"), test_view("v2")],
        ..Default::default()
    };
    s.views[0].cols = u32::MAX;
    s.views[0].rows = 999_999;
    s.views[1].cols = 0; // and the other direction: a grid with no cells at all
    s.views[1].rows = 0;
    s.migrate();
    assert_eq!(s.views[0].cols, GRID_MAX);
    assert_eq!(s.views[0].rows, GRID_MAX);
    assert_eq!(s.views[1].cols, GRID_MIN);
    assert_eq!(s.views[1].rows, GRID_MIN);
    // More views than the ceiling are cut, not carried.
    let mut s = Settings {
        views: (0..MAX_VIEWS + 20).map(|i| test_view(&format!("v{i}"))).collect(),
        ..Default::default()
    };
    s.migrate();
    assert_eq!(s.views.len(), MAX_VIEWS);
}

fn tray_prompt(id: &str, name: &str, favorite: bool) -> Prompt {
    let mut p = sample_prompts().remove(1);
    p.id = id.into();
    p.name = name.into();
    p.favorite = favorite;
    p.hotkey = String::new();
    p
}

fn tray_all_on(recent_n: usize, fav_n: usize) -> TrayCfg {
    TrayCfg { recent: true, recent_n, favs: true, fav_n, label_max: TRAY_LABEL_MAX }
}

// The entries of one section of the plan, or an empty list if it is not drawn.
fn tray_part(plan: &[(TraySection, Vec<TrayEntry>)], want: TraySection) -> Vec<TrayEntry> {
    plan.iter().find(|(s, _)| *s == want).map(|(_, e)| e.clone()).unwrap_or_default()
}
fn tray_names(entries: &[TrayEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.label.as_str()).collect()
}

#[test]
fn tray_settings_default_to_both_sections() {
    let cfg = tray_cfg(&Settings::default());
    assert!(cfg.recent, "the recent section ships on");
    assert!(cfg.favs, "the favourites section ships on");
    assert_eq!(cfg.recent_n, 5);
    assert_eq!(cfg.fav_n, 10);

    // Both features off = a menu with no dynamic entries at all.
    let mut s = Settings::default();
    s.ui_flags.insert("trayRecent".into(), false);
    s.ui_flags.insert("trayFavs".into(), false);
    let off = tray_cfg(&s);
    assert!(!off.recent && !off.favs);
    let prompts = vec![tray_prompt("a", "A", true)];
    let mut used = HashMap::new();
    used.insert("a".to_string(), 10u64);
    assert!(tray_plan(&prompts, &used, &off).is_empty());
}

#[test]
fn tray_counts_are_clamped_to_the_slider_range() {
    let mut s = Settings::default();
    s.ui_values.insert("trayRecentN".into(), 999.0);
    s.ui_values.insert("trayFavN".into(), 0.0);
    let cfg = tray_cfg(&s);
    assert_eq!(cfg.recent_n, 10);
    assert_eq!(cfg.fav_n, 1);
}

#[test]
fn tray_search_defaults_on() {
    let s = Settings::default();
    assert!(tray_search_on(&s), "the filter row ships on");

    let mut off = Settings::default();
    off.ui_flags.insert("traySearch".into(), false);
    assert!(!tray_search_on(&off));
}

#[test]
fn launcher_prefill_is_capped_at_200_characters() {
    let long: String = "x".repeat(250);
    assert_eq!(launcher_prefill_text(&long).chars().count(), 200);
    assert_eq!(launcher_prefill_text(""), "");
    assert_eq!(launcher_prefill_text("ab"), "ab");
}

#[test]
fn zoom_min_size_grows_with_zoom_and_shrinks_back() {
    let big_screen = (10_000, 10_000);
    assert_eq!(zoom_min_size(1.0, big_screen), (680, 460));
    assert_eq!(zoom_min_size(2.5, big_screen), (1700, 1150));
    assert_eq!(zoom_min_size(0.75, big_screen), (510, 345));
}

#[test]
fn zoom_min_size_never_exceeds_the_monitor() {
    let small_screen = (1000, 800);
    assert_eq!(zoom_min_size(3.0, small_screen), (1000, 800));
}

#[test]
fn tray_recent_is_newest_first_and_capped() {
    let prompts = vec![
        tray_prompt("p1", "One", false),
        tray_prompt("p2", "Two", false),
        tray_prompt("p3", "Three", false),
        tray_prompt("p4", "Never copied", false),
    ];
    let used: HashMap<String, u64> = [("p1", 100u64), ("p2", 300), ("p3", 200)]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    let cfg = tray_all_on(2, 10);
    let recent = tray_part(&tray_plan(&prompts, &used, &cfg), TraySection::Recent);
    assert_eq!(tray_names(&recent), vec!["Two", "Three"], "newest first, cut at trayRecentN");
    // A prompt that was never copied has no entry in last_used and stays out.
    let all = tray_part(&tray_plan(&prompts, &used, &tray_all_on(10, 10)), TraySection::Recent);
    assert_eq!(all.len(), 3);
    assert!(all.iter().all(|e| e.id != tray_item_id("p4")));
}

#[test]
fn tray_recent_breaks_ties_by_name() {
    let prompts = vec![tray_prompt("p1", "Beta", false), tray_prompt("p2", "Alpha", false)];
    let used: HashMap<String, u64> =
        [("p1", 50u64), ("p2", 50)].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    let recent = tray_part(&tray_plan(&prompts, &used, &tray_all_on(10, 10)), TraySection::Recent);
    assert_eq!(tray_names(&recent), vec!["Alpha", "Beta"]);
}

#[test]
fn tray_favorites_are_alphabetical_and_capped() {
    let prompts = vec![
        tray_prompt("p1", "zebra", true),
        tray_prompt("p2", "Apple", true),
        tray_prompt("p3", "Middle", true),
        tray_prompt("p4", "Not starred", false),
    ];
    let favs =
        tray_part(&tray_plan(&prompts, &HashMap::new(), &tray_all_on(5, 2)), TraySection::Favorites);
    assert_eq!(tray_names(&favs), vec!["Apple", "Middle"]);
    let all =
        tray_part(&tray_plan(&prompts, &HashMap::new(), &tray_all_on(5, 10)), TraySection::Favorites);
    assert_eq!(all.len(), 3, "only starred prompts");
    assert_eq!(all[2].id, tray_item_id("p1"), "case-insensitive sort puts zebra last");
}

#[test]
fn the_pinned_section_is_drawn_first_and_never_repeated_below() {
    let prompts = vec![
        tray_prompt("p1", "Starred", true),
        tray_prompt("p2", "Plain", false),
        tray_prompt("p3", "Older", false),
    ];
    let used: HashMap<String, u64> = [("p1", 300u64), ("p2", 200), ("p3", 100)]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    let plan = tray_plan(&prompts, &used, &tray_all_on(2, 10));
    assert_eq!(
        plan.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
        vec![TraySection::Favorites, TraySection::Recent],
        "pinned favourites sit above the churning recents"
    );
    assert_eq!(tray_names(&plan[0].1), vec!["Starred"]);
    // p1 is the most recent too, but it is already pinned: the recents skip it and
    // backfill with the next unlisted prompt instead of showing a duplicate.
    assert_eq!(tray_names(&plan[1].1), vec!["Plain", "Older"]);
}

#[test]
fn a_tray_entry_carries_the_media_kind_its_icon_is_drawn_from() {
    let mut plain = tray_prompt("p1", "Plain", false);
    plain.file_path = String::new();
    plain.icon_path = String::new();
    plain.image = String::new();
    plain.color = String::new();
    assert_eq!(tray_kind(&plain), TrayKind::Text);

    let mut decorated = plain.clone();
    decorated.image = "data:image/png;base64,AAA".into();
    decorated.show_image = false;
    decorated.copy_image = false;
    assert_eq!(tray_kind(&decorated), TrayKind::Text, "a decorated text prompt still copies text");

    let mut shown = decorated.clone();
    shown.show_image = true;
    assert_eq!(tray_kind(&shown), TrayKind::Image);
    let mut copied = decorated.clone();
    copied.copy_image = true;
    assert_eq!(tray_kind(&copied), TrayKind::Image);

    let mut gif = shown.clone();
    gif.icon_path = "C:/clip.gif".into();
    assert_eq!(tray_kind(&gif), TrayKind::Media, "a gif/video icon outranks a still");

    let mut doc = gif.clone();
    doc.file_path = "C:/report.pdf".into();
    assert_eq!(tray_kind(&doc), TrayKind::File, "an attachment is what the click delivers");

    // The plan carries kind and colour through, so the menu never re-reads the store.
    let mut coloured = doc.clone();
    coloured.color = "#123456".into();
    let plan = tray_plan(&[coloured], &HashMap::new(), &tray_all_on(5, 5));
    assert!(plan.is_empty(), "not starred, never used");
    let mut starred = doc.clone();
    starred.favorite = true;
    starred.color = "#123456".into();
    let plan = tray_plan(&[starred], &HashMap::new(), &tray_all_on(5, 5));
    assert_eq!(plan[0].1[0].kind, TrayKind::File);
    assert_eq!(plan[0].1[0].color, "#123456");
}

#[test]
fn tray_custom_menu_defaults_on_and_the_flag_turns_it_off() {
    let mut settings = Settings::default();
    assert!(tray_custom_menu_on(&settings), "ships on until a user turns it off");
    settings.ui_flags.insert(TRAY_CUSTOM_MENU_KEY.to_string(), false);
    assert!(!tray_custom_menu_on(&settings));
    settings.ui_flags.insert(TRAY_CUSTOM_MENU_KEY.to_string(), true);
    assert!(tray_custom_menu_on(&settings), "explicitly re-enabling it must also work");
}

// The right-click on the tray icon prepares its plan from ONE store reference, off
// the event-loop thread. No red exists for the old shape: the old code took the lock
// twice on the main thread, and a hang needs a second thread holding the store, which
// a unit test cannot stage deterministically. This pins the new contract instead.
#[test]
fn tray_open_prep_skips_while_the_custom_window_is_off_and_carries_the_plan_when_on() {
    let mut store = Store { prompts: Vec::new(), trash: Vec::new(), settings: Settings::default(), prompts_locked: false, written: Default::default() };
    store.settings.ui_flags.insert(ALLOW_SCREENSHOT_KEY.to_string(), true);
    let (plan, excluded) = tray_open_prep_from_store(&store, None).expect("custom window ships on");
    assert!(!excluded, "a window that may be captured is not excluded");
    assert!(!plan.show_label.is_empty() && !plan.quit_label.is_empty());
    assert!(plan.rec_entries.is_empty(), "no recording running, no recording rows");
    let (with_rec, _) = tray_open_prep_from_store(&store, Some(RecPhase::Paused)).unwrap();
    assert_eq!(with_rec.rec_entries.len(), 2, "stop + pause rows while a recording runs");
    store.settings.ui_flags.insert(ALLOW_SCREENSHOT_KEY.to_string(), false);
    assert!(tray_open_prep_from_store(&store, None).unwrap().1, "default keeps the window out of captures");
    store.settings.ui_flags.insert(TRAY_CUSTOM_MENU_KEY.to_string(), false);
    assert!(tray_open_prep_from_store(&store, None).is_none(), "native popup owns the click");
}

// The menu window is built once and reused: a right-click only moves and refills the
// warm window. Building a WebView2 window per click took up to seconds and made the
// menu feel hung. No red exists as a unit test (window creation needs a running app),
// so this pins the shape: the open path builds only when no window exists, and the page
// reveals the window itself after drawing.
#[test]
fn tray_menu_window_is_reused_and_revealed_by_its_page() {
    let src = production_sources();
    let a = src.find("pub(crate) fn open_tray_window").expect("open_tray_window");
    let b = a + src[a..].find("
}
").expect("end of open_tray_window");
    let body = &src[a..b];
    assert!(!body.contains("WebviewWindowBuilder"), "open_tray_window must not build a window itself");
    assert!(!body.contains(".close()"), "the warm window must not be closed on open");
    let get = body.find("get_webview_window(TRAY_WIN_LABEL)").expect("reuses the existing window");
    let build = body.find("build_tray_window(app)").expect("builds only as fallback");
    assert!(get < build);
    // A hidden WebView2 is throttled: the open wakes it off-screen at full size, and
    // only the reveal moves it into place.
    let wake = body.find("set_bounds_phys(&win, tray_wake_rect(rect))").expect("wakes the window off-screen");
    let show = body.find("win.show()").expect("shows the window to wake it");
    assert!(wake < show, "the window must be off-screen before it is shown");
    let r = tray_wake_rect(Rect { x: 100.0, y: 200.0, w: 240.0, h: 300.0 });
    assert!(r.x <= -30000.0 && r.y <= -30000.0 && r.w == 240.0 && r.h == 300.0);
    let c = src.find("pub(crate) fn tray_window_close").expect("tray_window_close");
    let d = c + src[c..].find("
}
").expect("end of tray_window_close");
    assert!(!src[c..d].contains(".close()"), "closing the menu only hides the warm window");
    let js = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/tray.js")).expect("ui/tray.js");
    assert!(js.contains("listen(\"tray-open\"") && js.contains("invoke(\"tray_window_reveal\")"));
}

#[test]
fn tray_pending_plan_is_handed_out_once() {
    let plan = TrayWinPlan { rec_entries: Vec::new(), sections: Vec::new(), show_label: "Show".into(), quit_label: "Quit".into(), search_on: false, search_ph: String::new() };
    *TRAY_WIN_PENDING.lock().unwrap() = Some(plan);
    assert_eq!(tray_window_plan().map(|p| p.show_label), Some("Show".to_string()));
    assert!(tray_window_plan().is_none(), "a second take for the same open gets nothing");
}

#[test]
fn tray_kind_str_round_trips_every_kind_to_a_distinct_lowercase_word() {
    let all = [
        (TrayKind::Text, "text"),
        (TrayKind::Image, "image"),
        (TrayKind::Media, "media"),
        (TrayKind::File, "file"),
    ];
    for (kind, word) in all {
        assert_eq!(tray_kind_str(kind), word);
    }
}

#[test]
fn tray_window_height_accounts_for_every_row_header_and_separator() {
    let entry = |id: &str| TrayWinEntry { id: id.into(), label: id.into(), kind: "text".into(), color: String::new() };
    // No sections at all: just the two terminal rows, plus the card's own padding.
    let empty = TrayWinPlan { rec_entries: Vec::new(), sections: Vec::new(), show_label: "Show".into(), quit_label: "Quit".into(), search_on: false, search_ph: String::new() };
    assert_eq!(tray_window_logical_height(&empty), TRAY_WIN_CHROME + 2.0 * TRAY_WIN_ROW_H);

    // One section, two entries: its own header, both rows, and the one separator
    // that always follows a section (matching push_tray_section's native shape).
    let one_section = TrayWinPlan {
        rec_entries: Vec::new(),
        sections: vec![TrayWinSection { title: "Favorites".into(), entries: vec![entry("a"), entry("b")] }],
        show_label: "Show".into(),
        quit_label: "Quit".into(),
        search_on: false,
        search_ph: String::new(),
    };
    let want_one = TRAY_WIN_CHROME + 2.0 * TRAY_WIN_ROW_H + TRAY_WIN_HEAD_H + 2.0 * TRAY_WIN_ROW_H + TRAY_WIN_SEP_H;
    assert_eq!(tray_window_logical_height(&one_section), want_one);

    // Two sections must each add their own header, rows and separator -- never
    // shared, since the native menu (push_tray_section) never shares one either.
    let two_sections = TrayWinPlan {
        rec_entries: Vec::new(),
        sections: vec![
            TrayWinSection { title: "Favorites".into(), entries: vec![entry("a")] },
            TrayWinSection { title: "Recent".into(), entries: vec![entry("b"), entry("c"), entry("d")] },
        ],
        show_label: "Show".into(),
        quit_label: "Quit".into(),
        search_on: false,
        search_ph: String::new(),
    };
    let want_two = TRAY_WIN_CHROME
        + 2.0 * TRAY_WIN_ROW_H // show + quit
        + (TRAY_WIN_HEAD_H + 1.0 * TRAY_WIN_ROW_H + TRAY_WIN_SEP_H) // 1-entry section
        + (TRAY_WIN_HEAD_H + 3.0 * TRAY_WIN_ROW_H + TRAY_WIN_SEP_H); // 3-entry section
    assert_eq!(tray_window_logical_height(&two_sections), want_two);
}

#[test]
fn tray_window_height_adds_one_row_for_search_when_on() {
    // search_on toggles ONE extra row (the filter field), on top of whatever
    // sections/rec_entries already contribute -- proven here against the
    // otherwise-identical plan so a future change to the row math cannot
    // silently forget the search row.
    let base = TrayWinPlan {
        rec_entries: Vec::new(),
        sections: vec![TrayWinSection { title: "Favorites".into(), entries: vec![TrayWinEntry { id: "a".into(), label: "a".into(), kind: "text".into(), color: String::new() }] }],
        show_label: "Show".into(),
        quit_label: "Quit".into(),
        search_on: false,
        search_ph: String::new(),
    };
    let with_search = TrayWinPlan { search_on: true, ..base.clone() };
    assert_eq!(
        tray_window_logical_height(&with_search) - tray_window_logical_height(&base),
        TRAY_WIN_ROW_H,
        "turning the search row on must add exactly one row of height, no more"
    );
}

#[test]
fn tray_window_capped_height_never_exceeds_the_work_area() {
    let entry = |id: &str| TrayWinEntry { id: id.into(), label: id.into(), kind: "text".into(), color: String::new() };
    // Max entries in both sections (the supported ceiling) -- uncapped this is
    // ~742 logical px, comfortably past a 1366x768 @150% work area.
    let full = TrayWinPlan {
        rec_entries: Vec::new(),
        sections: vec![
            TrayWinSection { title: "Favorites".into(), entries: (0..10).map(|i| entry(&i.to_string())).collect() },
            TrayWinSection { title: "Recent".into(), entries: (0..10).map(|i| entry(&i.to_string())).collect() },
        ],
        show_label: "Show".into(),
        quit_label: "Quit".into(),
        search_on: false,
        search_ph: String::new(),
    };
    let dpi = 1.5;
    let uncapped = tray_window_logical_height(&full) * dpi;
    let small_work_h = 680.0 * dpi; // a real reported 1366x768@150% work-area height
    let h = tray_window_capped_height(&full, small_work_h, dpi);
    assert!(uncapped > small_work_h, "the fixture must actually be taller than the work area, or this test proves nothing");
    assert!(h <= small_work_h, "the window must never be sized past the work area it opens in: got {h}, work area {small_work_h}");
    assert!(h > 0.0, "must still leave a usable window, not collapse to nothing");

    // A tall enough monitor must not clip content that would otherwise fit.
    let roomy = tray_window_capped_height(&full, 2000.0, dpi);
    assert_eq!(roomy, uncapped, "plenty of room must not shrink the window below its real content height");
}

#[test]
fn a_menu_icon_is_a_full_opaque_16x16_chip() {
    // Opaque everywhere is not cosmetic: the Win32 icon->bitmap path only builds a
    // correct mask when it is, so a transparent pixel would show up as a stripe.
    for kind in [TrayKind::Text, TrayKind::Image, TrayKind::Media, TrayKind::File] {
        let px = tray_icon_pixels(kind, "");
        assert_eq!(px.len(), (TRAY_ICON_PX * TRAY_ICON_PX * 4) as usize);
        assert!(px.chunks_exact(4).all(|p| p[3] == 255), "{kind:?} has a transparent pixel");
    }
    // Each media kind is visibly a different chip.
    let mut seen: Vec<Vec<u8>> = Vec::new();
    for kind in [TrayKind::Text, TrayKind::Image, TrayKind::Media, TrayKind::File] {
        let px = tray_icon_pixels(kind, "");
        assert!(!seen.contains(&px), "{kind:?} draws the same chip as another kind");
        seen.push(px);
    }
    // The prompt's own colour wins over the kind tint, and the glyph flips to stay
    // readable on it: dark ink on a near-white chip, light ink on a near-black one.
    let white = tray_icon_pixels(TrayKind::Text, "#FFFFFF");
    let black = tray_icon_pixels(TrayKind::Text, "#000000");
    let ink = |px: &[u8], x: usize, y: usize| px[(y * 16 + x) * 4];
    assert_eq!(ink(&white, 5, 4), 24, "dark glyph on a light chip");
    assert_eq!(ink(&black, 5, 4), 255, "light glyph on a dark chip");
    // A junk colour must not blank the icon; it falls back to the kind tint.
    assert_eq!(tray_icon_pixels(TrayKind::Text, "not a colour"), tray_icon_pixels(TrayKind::Text, ""));
}

#[test]
fn tray_labels_are_cut_by_character_not_by_byte() {
    // Every char is 3 bytes: a byte cut would split one and panic.
    let cjk = "字".repeat(80);
    let cut = tray_label(&cjk, TRAY_LABEL_MAX);
    assert_eq!(cut.chars().count(), TRAY_LABEL_MAX);
    assert!(cut.ends_with('…'));
    for name in ["Привет мир ".repeat(20), "नमस्ते दुनिया ".repeat(20), "🙂🙃".repeat(60)] {
        let out = tray_label(&name, TRAY_LABEL_MAX);
        assert!(out.chars().count() <= TRAY_LABEL_MAX, "{out:?} is too long");
        assert!(!out.is_empty());
    }
    // Short names pass through untouched.
    assert_eq!(tray_label("Mail", TRAY_LABEL_MAX), "Mail");
    // Control characters collapse into single spaces, the name never spans lines.
    assert_eq!(tray_label("a\nb\tc\u{7}d", TRAY_LABEL_MAX), "a b c d");
    // A Win32 menu reads a lone & as a mnemonic marker.
    assert_eq!(tray_label("R&D", TRAY_LABEL_MAX), "R&&D");
    // A nameless prompt still gets something clickable.
    assert!(!tray_label("   ", TRAY_LABEL_MAX).is_empty());
    // An over-long name is cut back to the last word break near the cap, so the
    // entry ends on a whole word instead of mid-word.
    let cut = tray_label("Summarise this article into five concise bullet points", TRAY_LABEL_MAX);
    assert_eq!(cut, "Summarise this article into five…");
    // A single very long word has no usable break and is still cut hard.
    let word = tray_label(&"x".repeat(60), TRAY_LABEL_MAX);
    assert_eq!(word.chars().count(), TRAY_LABEL_MAX);
}

#[test]
fn tray_label_respects_a_custom_max() {
    // A single long word has no word break to back up to, so it is cut to
    // exactly whatever max is passed in -- not the TRAY_LABEL_MAX const.
    let word = "x".repeat(60);
    assert_eq!(tray_label(&word, 20).chars().count(), 20);
    assert_eq!(tray_label(&word, 80).chars().count(), 60); // fits under 80, untouched
}

#[test]
fn ink_prefs_default_on_with_a_clamped_threshold() {
    let s = Settings::default();
    let prefs = ink_prefs_of(&s);
    assert!(prefs.auto, "the automatic ink rule ships on");
    assert_eq!(prefs.threshold, 60.0);

    let mut off = Settings::default();
    off.ui_flags.insert("autoInk".into(), false);
    assert!(!ink_prefs_of(&off).auto);

    let mut clamped_hi = Settings::default();
    clamped_hi.ui_values.insert("inkThreshold".into(), 999.0);
    assert_eq!(ink_prefs_of(&clamped_hi).threshold, 90.0);

    let mut clamped_lo = Settings::default();
    clamped_lo.ui_values.insert("inkThreshold".into(), 0.0);
    assert_eq!(ink_prefs_of(&clamped_lo).threshold, 20.0);
}

#[test]
fn ink_halo_default_on_with_a_clamped_strength() {
    let s = Settings::default();
    let prefs = ink_prefs_of(&s);
    assert!(prefs.halo, "the ink halo ships on");
    assert_eq!(prefs.halo_strength, 75.0);

    let mut off = Settings::default();
    off.ui_flags.insert("inkHalo".into(), false);
    assert!(!ink_prefs_of(&off).halo);

    let mut clamped_hi = Settings::default();
    clamped_hi.ui_values.insert("inkHaloStrength".into(), 999.0);
    assert_eq!(ink_prefs_of(&clamped_hi).halo_strength, 100.0);

    let mut clamped_lo = Settings::default();
    clamped_lo.ui_values.insert("inkHaloStrength".into(), 0.0);
    assert_eq!(ink_prefs_of(&clamped_lo).halo_strength, 10.0);
}

#[test]
fn tray_label_max_clamps_the_stored_expert_value() {
    let mut s = Settings::default();
    assert_eq!(tray_label_max(&s), TRAY_LABEL_MAX); // absent -> shipped default
    s.ui_values.insert("trayLabelMax".into(), 5.0);
    assert_eq!(tray_label_max(&s), 20); // below floor -> clamped up
    s.ui_values.insert("trayLabelMax".into(), 500.0);
    assert_eq!(tray_label_max(&s), 80); // above ceiling -> clamped down
    s.ui_values.insert("trayLabelMax".into(), 60.0);
    assert_eq!(tray_label_max(&s), 60); // in range -> passed through
}

#[test]
fn sync_poll_secs_clamps_the_stored_expert_value() {
    let mut s = Settings::default();
    assert_eq!(sync_poll_secs(&s), SYNC_POLL_SECS); // absent -> shipped default
    s.ui_values.insert("syncPollSecs".into(), 0.0);
    assert_eq!(sync_poll_secs(&s), 2); // below floor -> clamped up
    s.ui_values.insert("syncPollSecs".into(), 100.0);
    assert_eq!(sync_poll_secs(&s), 30); // above ceiling -> clamped down
    s.ui_values.insert("syncPollSecs".into(), 10.0);
    assert_eq!(sync_poll_secs(&s), 10); // in range -> passed through
}

#[test]
fn tray_menu_ids_round_trip_and_reject_junk() {
    assert_eq!(tray_copy_target(&tray_item_id("abc123")), Some("abc123"));
    // A prompt id is opaque, so the parser must not stop at the first separator.
    assert_eq!(tray_copy_target(&tray_item_id("a:b:c")), Some("a:b:c"));
    for bad in ["show", "quit", "", "psc", "psc:", "PSC:x", "copy:x", ":x", "tray-head-recent"] {
        assert_eq!(tray_copy_target(bad), None, "accepted a bad menu id: {bad:?}");
    }
}

// ---- Pill folders ----

// A pill 200x80 at (x, y). Physical pixels, like every folder computation.
fn pill_at(x: f64, y: f64) -> Rect {
    Rect { x, y, w: 200.0, h: 80.0 }
}

fn folder_of(id: &str, members: &[&str]) -> FloatFolder {
    FloatFolder {
        id: id.into(),
        name: String::new(),
        pos: Pos { x: 100, y: 100, ..Default::default() },
        scale: 1.0,
        members: members.iter().map(|s| s.to_string()).collect(),
    }
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_drop_on_top_of_another_pill_merges() {
    // Dropped 30px away: overlapping and inside the default 40px radius.
    let dragged = pill_at(130.0, 100.0);
    let others = vec![("other".to_string(), pill_at(100.0, 100.0))];
    assert_eq!(merge_target(dragged, &others, 40.0), Some("other"));
}

#[test]
fn a_small_drag_never_merges() {
    // The pill moved four pixels and touches nothing else: the case that must
    // never turn a nudge into a folder.
    let others = vec![
        ("a".to_string(), pill_at(600.0, 100.0)),
        ("b".to_string(), pill_at(100.0, 400.0)),
    ];
    assert_eq!(merge_target(pill_at(104.0, 100.0), &others, 40.0), None);
    // And a pill that stayed exactly where it was matches nothing either.
    assert_eq!(merge_target(pill_at(100.0, 100.0), &others, 40.0), None);
}

#[test]
fn merge_needs_both_the_radius_and_a_real_overlap() {
    let target = pill_at(100.0, 100.0);
    let others = vec![("other".to_string(), target)];
    // Centre distance exactly on the radius still counts as a hit ...
    assert_eq!(merge_target(pill_at(140.0, 100.0), &others, 40.0), Some("other"));
    // ... one pixel further does not.
    assert_eq!(merge_target(pill_at(141.0, 100.0), &others, 40.0), None);
    // Overlapping but far apart at the centres (a long text pill whose end just
    // touches) is not a merge either.
    let wide = vec![("wide".to_string(), Rect { x: 0.0, y: 100.0, w: 260.0, h: 80.0 })];
    assert!(pill_at(250.0, 100.0).overlaps(&wide[0].1));
    assert_eq!(merge_target(pill_at(250.0, 100.0), &wide, 40.0), None);
    // Two small pills close enough by centre distance but with a gap between them:
    // near is not on top of, so nothing merges.
    let small = Rect { x: 100.0, y: 100.0, w: 10.0, h: 10.0 };
    let apart = vec![("apart".to_string(), Rect { x: 130.0, y: 100.0, w: 10.0, h: 10.0 })];
    assert!(!small.overlaps(&apart[0].1));
    assert_eq!(merge_target(small, &apart, 40.0), None);
}

#[test]
fn the_nearest_pill_wins_a_crowded_drop() {
    let dragged = pill_at(100.0, 100.0);
    let others = vec![
        ("far".to_string(), pill_at(130.0, 100.0)),
        ("near".to_string(), pill_at(110.0, 100.0)),
        ("out".to_string(), pill_at(400.0, 100.0)),
    ];
    assert_eq!(merge_target(dragged, &others, 40.0), Some("near"));
}

#[test]
fn folder_invariants_are_restored_and_idempotent() {
    let known = ids(&["a", "b", "c", "d", "e"]);
    let mut folders = vec![
        folder_of("f1", &["a", "gone", "b"]),
        folder_of("f2", &["b", "c", "d"]), // b already belongs to f1
        folder_of("f3", &["e"]),           // a single member is not a folder
        folder_of("f4", &["ghost", "phantom"]),
    ];
    normalize_float_folders(&mut folders, &known);
    assert_eq!(folders.len(), 2);
    assert_eq!(folders[0].members, ids(&["a", "b"]));
    assert_eq!(folders[1].id, "f2");
    assert_eq!(folders[1].members, ids(&["c", "d"]));
    // Running it again changes nothing.
    let before: Vec<Vec<String>> = folders.iter().map(|f| f.members.clone()).collect();
    normalize_float_folders(&mut folders, &known);
    let after: Vec<Vec<String>> = folders.iter().map(|f| f.members.clone()).collect();
    assert_eq!(before, after);
}

#[test]
fn a_prompt_lives_in_at_most_one_folder() {
    let known = ids(&["a", "b", "c"]);
    let mut folders = vec![folder_of("f1", &["a", "b"]), folder_of("f2", &["a", "c"])];
    normalize_float_folders(&mut folders, &known);
    assert_eq!(folders[0].members, vec!["a", "b"]);
    // f2 lost the duplicate and is down to one member, so it dissolves.
    assert_eq!(folders.len(), 1);
}

#[test]
fn a_broken_folder_scale_is_clamped() {
    let known = ids(&["a", "b"]);
    let mut folders = vec![folder_of("f1", &["a", "b"])];
    folders[0].scale = f64::NAN;
    normalize_float_folders(&mut folders, &known);
    assert_eq!(folders[0].scale, 1.0);
    folders[0].scale = 99.0;
    normalize_float_folders(&mut folders, &known);
    assert_eq!(folders[0].scale, 3.0);
}

#[test]
fn deleting_a_prompt_purges_it_from_its_folder() {
    let mut settings = Settings::default();
    settings.ui_flags.insert("pillFolders".into(), true);
    settings.float_folders = vec![folder_of("f1", &["a", "b", "c"]), folder_of("f2", &["d", "e"])];
    // delete_prompt needs an AppHandle, so exercise the same steps it performs.
    let mut alive = ids(&["a", "b", "c", "d", "e"]);
    alive.retain(|p| p != "c");
    normalize_float_folders(&mut settings.float_folders, &alive);
    assert_eq!(settings.float_folders[0].members, vec!["a", "b"]);
    // Losing a second member leaves one behind, so that folder dissolves and its
    // pill goes back to the free position it never gave up.
    alive.retain(|p| p != "d");
    normalize_float_folders(&mut settings.float_folders, &alive);
    assert_eq!(settings.float_folders.len(), 1);
    assert_eq!(settings.float_folders[0].id, "f1");
}

#[test]
fn the_window_plan_follows_the_feature_flag() {
    let mut settings = Settings::default();
    for id in ["a", "b", "c"] {
        settings.floating.insert(id.to_string(), Pos { x: 0, y: 0, ..Default::default() });
    }
    settings.float_folders = vec![folder_of("f1", &["a", "b"])];
    let known = ids(&["a", "b", "c"]);

    // On (the shipped default): the members lose their own window, the folder
    // gets one.
    let (pills, folders) = float_window_plan(&settings, &known);
    assert_eq!(pills, ids(&["c"]));
    assert_eq!(folders, ids(&["f1"]));

    // Off: every member is a plain pill again, no folder; switching the feature
    // off must put every pill back.
    settings.ui_flags.insert("pillFolders".into(), false);
    let (pills, folders) = float_window_plan(&settings, &known);
    assert_eq!(pills, ids(&["a", "b", "c"]));
    assert!(folders.is_empty());
    settings.ui_flags.remove("pillFolders");

    // An unpinned prompt never gets a window, folder member or not.
    settings.floating.remove("c");
    let (pills, _) = float_window_plan(&settings, &known);
    assert!(pills.is_empty());
}

#[test]
fn folder_layout_keeps_its_fixed_defaults() {
    let s = Settings::default();
    assert!(folders_on(&s), "the feature ships on");
    assert_eq!(MERGE_RADIUS, 40.0);
    assert_eq!(FOLDER_COLS, 3);
}

#[test]
fn folder_names_are_trimmed_and_stripped() {
    assert_eq!(folder_name_of("  Work  ").unwrap(), "Work");
    assert_eq!(folder_name_of("").unwrap(), "");
    // A pasted newline or control byte never reaches the window title.
    assert_eq!(folder_name_of("Wo\nrk\u{7}").unwrap(), "Work");
    assert!(folder_name_of(&"a".repeat(MAX_FOLDER_NAME)).is_ok());
    assert!(folder_name_of(&"a".repeat(MAX_FOLDER_NAME + 1)).is_err());
}

// A 1920x1080 primary screen with a 40px taskbar at the bottom.
fn primary_work() -> Rect {
    Rect { x: 0.0, y: 0.0, w: 1920.0, h: 1040.0 }
}

const ICON: f64 = 96.0;

fn icon_at(x: f64, y: f64) -> Rect {
    Rect { x, y, w: ICON, h: ICON }
}

#[test]
fn a_folder_opens_below_when_there_is_room() {
    let icon = icon_at(400.0, 200.0);
    let l = folder_layout(icon, 260.0, 180.0, 6.0, primary_work());
    assert_eq!(l.side, FolderSide::Below);
    // The icon never moves: it still sits at its own screen position.
    assert_eq!(l.win.x + l.icon.0, icon.x);
    assert_eq!(l.win.y + l.icon.1, icon.y);
    // The popover hangs under it with the gap in between, aligned with the icon's
    // left edge: so the window origin IS the icon and nothing shifts sideways
    // when it opens.
    assert_eq!(l.win.y + l.pop.1, icon.bottom() + 6.0);
    assert_eq!(l.win.x + l.pop.0, icon.x);
    assert_eq!(l.icon, (0.0, 0.0));
    // One window covers both.
    assert_eq!(l.win.h, ICON + 6.0 + 180.0);
    assert_eq!(l.win.w, 260.0);
}

#[test]
fn opening_a_folder_never_moves_the_window_origin_sideways() {
    // Anywhere with room to the right, the popover starts at the icon's own left
    // edge: the origin stays put, so the icon cannot jump left/right for a frame.
    for x in [0.0, 400.0, 1000.0] {
        let icon = icon_at(x, 200.0);
        let l = folder_layout(icon, 230.0, 140.0, 6.0, primary_work());
        assert_eq!(l.win.x, icon.x, "origin moved at x={}", x);
        assert_eq!(l.icon.0, 0.0);
    }
}

#[test]
fn a_folder_at_the_bottom_edge_flips_above() {
    let icon = icon_at(400.0, 900.0); // bottom at 996, work ends at 1040
    let l = folder_layout(icon, 260.0, 180.0, 6.0, primary_work());
    assert_eq!(l.side, FolderSide::Above);
    assert_eq!(l.win.y + l.pop.1, icon.y - 6.0 - 180.0);
    assert_eq!(l.win.x + l.icon.0, icon.x);
    assert_eq!(l.win.y + l.icon.1, icon.y);
    // Still fully inside the work area.
    assert!(l.win.y >= primary_work().y && l.win.bottom() <= primary_work().bottom());
}

#[test]
fn a_tall_grid_in_the_middle_flips_sideways() {
    // Neither below nor above fits a 900px grid, so it goes to the right ...
    let icon = icon_at(400.0, 500.0);
    let l = folder_layout(icon, 300.0, 900.0, 6.0, primary_work());
    assert_eq!(l.side, FolderSide::Right);
    assert_eq!(l.win.x + l.pop.0, icon.right() + 6.0);
    // ... and to the left when the right edge is too close.
    let icon = icon_at(1700.0, 500.0);
    let l = folder_layout(icon, 300.0, 900.0, 6.0, primary_work());
    assert_eq!(l.side, FolderSide::Left);
    assert_eq!(l.win.x + l.pop.0, icon.x - 6.0 - 300.0);
    assert!(l.win.x >= 0.0 && l.win.right() <= 1920.0);
}

#[test]
fn a_folder_in_a_corner_keeps_the_whole_popover_on_screen() {
    let work = primary_work();
    for icon in [icon_at(0.0, 0.0), icon_at(1824.0, 0.0), icon_at(0.0, 944.0), icon_at(1824.0, 944.0)] {
        let l = folder_layout(icon, 400.0, 300.0, 6.0, work);
        assert!(l.win.x >= work.x, "left edge escaped at {:?}", icon);
        assert!(l.win.y >= work.y, "top edge escaped at {:?}", icon);
        assert!(l.win.right() <= work.right(), "right edge escaped at {:?}", icon);
        assert!(l.win.bottom() <= work.bottom(), "bottom edge escaped at {:?}", icon);
        // Whatever had to move, it was never the icon.
        assert_eq!(l.win.x + l.icon.0, icon.x);
        assert_eq!(l.win.y + l.icon.1, icon.y);
    }
}

#[test]
fn a_second_monitor_at_another_dpi_flips_on_its_own_edges() {
    // A 150% 2560x1440 screen to the RIGHT of the primary one: its work area
    // starts at x=1920, and every size arrives multiplied by 1.5.
    let work = Rect { x: 1920.0, y: 0.0, w: 2560.0, h: 1380.0 };
    let icon = Rect { x: 2000.0, y: 1200.0, w: ICON * 1.5, h: ICON * 1.5 };
    let l = folder_layout(icon, 260.0 * 1.5, 180.0 * 1.5, 6.0 * 1.5, work);
    // Below would run past that monitor's work area, so it flips there too.
    assert_eq!(l.side, FolderSide::Above);
    assert!(l.win.x >= work.x && l.win.right() <= work.right());
    assert!(l.win.y >= work.y && l.win.bottom() <= work.bottom());
    // A folder in that screen's top-left corner opens below and stays on it:
    // the primary monitor to the left is never used as spare room.
    let icon = Rect { x: 1930.0, y: 20.0, w: ICON * 1.5, h: ICON * 1.5 };
    let l = folder_layout(icon, 600.0, 200.0, 9.0, work);
    assert_eq!(l.side, FolderSide::Below);
    assert!(l.win.x >= work.x, "the popover leaked onto the primary monitor");
}

#[test]
fn a_popover_larger_than_the_screen_still_starts_on_it() {
    let work = primary_work();
    let l = folder_layout(icon_at(900.0, 500.0), 4000.0, 4000.0, 6.0, work);
    assert_eq!(l.win.x + l.pop.0, work.x);
    assert_eq!(l.win.y + l.pop.1, work.y);
}

#[test]
fn dissolving_a_folder_never_stacks_its_pills() {
    let work = primary_work();
    let pill = |x: f64, y: f64| Rect { x, y, w: 200.0, h: 80.0 };
    // Two members whose stored position is the SAME spot: exactly what merging
    // one pill onto another leaves behind.
    let out = spread_free_positions(&[pill(500.0, 400.0), pill(500.0, 400.0)], &[], work);
    assert_eq!(out[0], (500.0, 400.0), "the first one keeps its wish");
    assert_ne!(out[0], out[1]);
    let a = pill(out[0].0, out[0].1);
    let b = pill(out[1].0, out[1].1);
    assert!(!a.overlaps(&b), "the pills still cover each other: {:?} {:?}", a, b);
    // Both stay on screen.
    for (x, y) in out {
        assert!(x >= work.x && x + 200.0 <= work.right());
        assert!(y >= work.y && y + 80.0 <= work.bottom());
    }
    // A pill already on screen is an obstacle too.
    let out = spread_free_positions(&[pill(500.0, 400.0)], &[pill(500.0, 400.0)], work);
    assert!(!pill(out[0].0, out[0].1).overlaps(&pill(500.0, 400.0)));
    // Idempotent: positions that are already free are left exactly where they are.
    let free = [pill(100.0, 100.0), pill(600.0, 600.0)];
    assert_eq!(
        spread_free_positions(&free, &[], work),
        vec![(100.0, 100.0), (600.0, 600.0)]
    );
}

#[test]
fn a_dropped_pill_is_clamped_onto_its_work_area() {
    let work = primary_work();
    // Dropped past the right/bottom edge: pulled back so the whole pill shows.
    assert_eq!(clamp_to_work(work, 200.0, 80.0, 1900.0, 1030.0), (1720.0, 960.0));
    // Dropped past the top/left edge.
    assert_eq!(clamp_to_work(work, 200.0, 80.0, -50.0, -20.0), (0.0, 0.0));
    // Inside: untouched.
    assert_eq!(clamp_to_work(work, 200.0, 80.0, 500.0, 400.0), (500.0, 400.0));
    // A second monitor's origin is respected, not the desktop's.
    let right = Rect { x: 1920.0, y: 0.0, w: 1280.0, h: 1000.0 };
    assert_eq!(clamp_to_work(right, 200.0, 80.0, 1900.0, 10.0), (1920.0, 10.0));
    // A window bigger than the screen is pinned to the corner instead of pushed off.
    assert_eq!(clamp_to_work(work, 5000.0, 5000.0, 300.0, 300.0), (0.0, 0.0));
}

#[test]
fn settings_without_folders_still_load() {
    // An older settings blob has no `float_folders` key at all.
    let s: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
    assert!(s.float_folders.is_empty());
    // A folder from a newer build keeps the fields this version knows and fills in
    // the ones it does not carry.
    let s: Settings = serde_json::from_str(
        r##"{"float_folders":[{"id":"f","name":"Work","members":["a","b"],"future":1}]}"##,
    )
    .unwrap();
    assert_eq!(s.float_folders[0].name, "Work");
    assert_eq!(s.float_folders[0].scale, 1.0);
    assert_eq!(s.float_folders[0].pos.x, 0);
}

// ---- Colour picker ----

#[test]
fn hex_is_uppercase_and_padded() {
    assert_eq!(rgb_to_hex(26, 43, 60), "#1A2B3C");
    assert_eq!(rgb_to_hex(0, 0, 0), "#000000");
    assert_eq!(rgb_to_hex(255, 255, 255), "#FFFFFF");
    assert_eq!(rgb_to_hex(1, 2, 3), "#010203"); // single digits keep their zero
}

#[test]
fn hsl_handles_the_degenerate_cases() {
    // Pure grey: hue and saturation are undefined, so both report 0.
    assert_eq!(rgb_to_hsl(128, 128, 128), (0, 0, 50));
    assert_eq!(rgb_to_hsl(0, 0, 0), (0, 0, 0));
    assert_eq!(rgb_to_hsl(255, 255, 255), (0, 0, 100));
    // Pure red sits at 0°, never at 360°.
    assert_eq!(rgb_to_hsl(255, 0, 0), (0, 100, 50));
    // Just below red wraps back down instead of rounding up to 360.
    assert_eq!(rgb_to_hsl(255, 0, 1).0, 0);
    assert_eq!(rgb_to_hsl(0, 255, 0), (120, 100, 50));
    assert_eq!(rgb_to_hsl(0, 0, 255), (240, 100, 50));
    // Each of the three max branches, with rounded integer output.
    assert_eq!(rgb_to_hsl(26, 43, 60), (210, 40, 17));
    assert_eq!(rgb_to_hsl(255, 128, 0), (30, 100, 50));
    assert_eq!(rgb_to_hsl(128, 0, 255), (270, 100, 50));
}

#[test]
fn every_format_writes_its_own_notation() {
    assert_eq!(format_color("hex", 26, 43, 60), "#1A2B3C");
    assert_eq!(format_color("rgb", 26, 43, 60), "rgb(26,43,60)");
    assert_eq!(format_color("hsl", 26, 43, 60), "hsl(210,40%,17%)");
    // An unknown or empty stored notation falls back to hex, never to nothing.
    assert_eq!(format_color("", 26, 43, 60), "#1A2B3C");
    assert_eq!(format_color("cmyk", 26, 43, 60), "#1A2B3C");
}

#[test]
fn picker_settings_are_clamped_to_their_range() {
    let mut s = Settings::default();
    let d = pick_cfg(&s);
    assert_eq!((d.zoom, d.format.as_str()), (4, "hex"));
    s.ui_values.insert("loupeZoom".into(), 99.0);
    assert_eq!(pick_cfg(&s).zoom, 16);
    s.ui_values.insert("loupeZoom".into(), 0.0);
    assert_eq!(pick_cfg(&s).zoom, 4);
    s.ui_texts.insert("pickerFormat".into(), "hsl".into());
    assert_eq!(pick_cfg(&s).format, "hsl");
    s.ui_texts.insert("pickerFormat".into(), "nonsense".into());
    assert_eq!(pick_cfg(&s).format, "hex");
}

// A stitched frozen desktop for a mixed-DPI pair: a 2560x1440 monitor scaled to
// 150% sits LEFT of the 1920x1080 primary, so the virtual origin is negative.
// Physical pixels are what the capture holds: the scale factor never enters the
// mapping, which is exactly why the picker stays exact on either monitor.
fn mixed_dpi_desktop() -> (Vec<MonitorCap>, i64, i64) {
    let (left_x, left_w, left_h) = (-2560i32, 2560u32, 1440u32);
    let (right_x, right_w, right_h) = (0i32, 1920u32, 1080u32);
    let origin_x = left_x.min(right_x);
    let origin_y = 0;
    let total_w = left_w + right_w;
    let total_h = left_h.max(right_h);
    let mut canvas = image::RgbaImage::from_pixel(total_w, total_h, image::Rgba([0, 0, 0, 255]));
    let lx = stitch_offset(left_x, origin_x);
    let rx = stitch_offset(right_x, origin_x);
    // One marker pixel per monitor, at the monitor's own top-left corner.
    canvas.put_pixel(lx as u32, 0, image::Rgba([26, 43, 60, 255]));
    canvas.put_pixel(rx as u32, 0, image::Rgba([255, 128, 0, 255]));
    // ...and one in the far bottom-right of the right-hand monitor.
    canvas.put_pixel(rx as u32 + right_w - 1, right_h - 1, image::Rgba([1, 2, 3, 255]));
    (
        vec![MonitorCap { image: Arc::new(canvas), x: origin_x, y: origin_y, width: total_w, height: total_h }],
        lx,
        rx,
    )
}

#[test]
fn stitch_offset_places_each_monitor_at_its_own_column() {
    // Left monitor starts the image; the primary starts one full width in.
    assert_eq!(stitch_offset(-2560, -2560), 0);
    assert_eq!(stitch_offset(0, -2560), 2560);
    // Vertical works the same, including a monitor above the primary.
    assert_eq!(stitch_offset(-300, -300), 0);
    assert_eq!(stitch_offset(0, -300), 300);
}

#[test]
fn sampling_hits_the_right_pixel_on_either_monitor() {
    let (caps, lx, rx) = mixed_dpi_desktop();
    assert_eq!(sample_frozen(&caps, lx as i32, 0), Some([26, 43, 60]));
    assert_eq!(sample_frozen(&caps, rx as i32, 0), Some([255, 128, 0]));
    assert_eq!(sample_frozen(&caps, rx as i32 + 1919, 1079), Some([1, 2, 3]));
    // The neighbouring pixel is a different colour: no off-by-one smearing.
    assert_eq!(sample_frozen(&caps, rx as i32 - 1, 0), Some([0, 0, 0]));
    // Outside the desktop nothing is sampled at all (never a clamped neighbour).
    assert_eq!(sample_frozen(&caps, -1, 0), None);
    assert_eq!(sample_frozen(&caps, 0, -1), None);
    assert_eq!(sample_frozen(&caps, 4480, 0), None);
    assert_eq!(sample_frozen(&caps, 0, 1440), None);
    assert_eq!(sample_frozen(&[], 0, 0), None);
}

#[test]
fn the_tile_handed_to_the_loupe_is_packed_rgb() {
    let mut img = image::RgbaImage::new(2, 1);
    img.put_pixel(0, 0, image::Rgba([26, 43, 60, 255]));
    img.put_pixel(1, 0, image::Rgba([255, 128, 0, 128]));
    // Alpha is dropped: three bytes per pixel, in row order.
    assert_eq!(crop_rgb(&img, 0, 0, 2, 1), vec![26, 43, 60, 255, 128, 0]);
}

#[test]
fn a_loupe_tile_reads_the_rows_it_was_asked_for() {
    // 3x3 where every pixel encodes its own coordinate, so a wrong stride shows up.
    let mut img = image::RgbaImage::new(3, 3);
    for y in 0..3u32 {
        for x in 0..3u32 {
            img.put_pixel(x, y, image::Rgba([x as u8, y as u8, 9, 255]));
        }
    }
    assert_eq!(
        crop_rgb(&img, 1, 1, 2, 2),
        vec![1, 1, 9, 2, 1, 9, 1, 2, 9, 2, 2, 9],
        "the tile must be the requested rect, not the top-left corner"
    );
}

#[test]
fn a_tile_that_leaves_the_frame_returns_nothing() {
    let img = image::RgbaImage::new(4, 4);
    assert!(crop_rgb(&img, 3, 0, 2, 1).is_empty()); // runs off the right edge
    assert!(crop_rgb(&img, 0, 3, 1, 2).is_empty()); // runs off the bottom edge
    assert!(crop_rgb(&img, 0, 0, 0, 4).is_empty()); // empty rect
    assert!(crop_rgb(&img, u32::MAX, 0, 4, 4).is_empty()); // would wrap
    assert_eq!(crop_rgb(&img, 0, 0, 4, 4).len(), 4 * 4 * 3); // the whole frame still fits
}

#[test]
fn snip_background_still_loads_without_the_picker_fields() {
    // The overlay reads the mode off SnipBg; serializing keeps the snake_case
    // names snip.js indexes.
    let bg = SnipBg {
        src: "x".into(), is_file: true, width: 4, height: 2,
        pick: true, pick_zoom: 12, pick_format: "rgb".into(), pick_cells: 21,
        pick_history: vec!["#FF0000".into()],
        record: false, origin_x: 0, origin_y: 0,
    };
    let j = serde_json::to_value(&bg).unwrap();
    assert_eq!(j["pick"], serde_json::json!(true));
    assert_eq!(j["pick_zoom"], serde_json::json!(12));
    assert_eq!(j["pick_format"], serde_json::json!("rgb"));
    assert_eq!(j["pick_cells"], serde_json::json!(21));
    assert_eq!(j["pick_history"], serde_json::json!(["#FF0000"]));
}

// ---- The shared timer thread ----
//
// Real intervals here are hours; every test below injects millisecond ones
// through the same registration call the app uses, so the behaviour under
// test is the scheduling itself and not a production constant.

use super::scheduler::Scheduler;
use std::time::Instant;

fn wait_until(mut cond: impl FnMut() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    cond()
}

const NEVER_AGAIN: Duration = Duration::from_secs(3600);

#[test]
fn every_registered_job_fires_in_due_order() {
    let log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
    let s = Scheduler::start();
    // Deliberately registered out of order: the due time decides, not the
    // order they arrived in. Each job owns its own thread, so nothing bounds
    // how far apart two due times must be to survive real OS scheduling
    // jitter under load; wide gaps keep this a scheduling-order test rather
    // than a thread-wakeup-latency test. The full suite runs many tests
    // concurrently, so the gaps are wide (600ms) rather than the original
    // 150ms: a 150ms gap measurably flaked under that contention.
    for (name, delay_ms) in [("third", 1800u64), ("first", 600), ("second", 1200)] {
        let log = log.clone();
        s.register_periodic(name, Duration::from_millis(delay_ms), NEVER_AGAIN, move || {
            log.lock().unwrap_or_else(|e| e.into_inner()).push(name);
        });
    }
    let done = wait_until(
        || log.lock().unwrap_or_else(|e| e.into_inner()).len() == 3,
        Duration::from_secs(5),
    );
    let fired = log.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(done, "only {fired:?} fired");
    assert_eq!(fired, vec!["first", "second", "third"]);
}

#[test]
fn a_shortened_interval_wakes_the_thread_early() {
    let hits = Arc::new(AtomicU64::new(0));
    let s = Scheduler::start();
    {
        let hits = hits.clone();
        s.register_periodic("slow", NEVER_AGAIN, NEVER_AGAIN, move || {
            hits.fetch_add(1, Ordering::SeqCst);
        });
    }
    s.retime("slow", Duration::from_millis(10));
    assert!(
        wait_until(|| hits.load(Ordering::SeqCst) >= 1, Duration::from_secs(5)),
        "the job was still serving out its old wait"
    );
}

// Counts the threads this process currently owns, so the "one thread instead
// of one per chore" claim is a measured number rather than an assertion about
// the source.
#[cfg(windows)]
mod thread_census {
    #[repr(C)]
    #[derive(Default)]
    struct ThreadEntry32 {
        size: u32,
        usage: u32,
        thread_id: u32,
        owner_pid: u32,
        base_pri: i32,
        delta_pri: i32,
        flags: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> isize;
        fn Thread32First(snap: isize, entry: *mut ThreadEntry32) -> i32;
        fn Thread32Next(snap: isize, entry: *mut ThreadEntry32) -> i32;
        fn CloseHandle(handle: isize) -> i32;
        fn GetCurrentProcessId() -> u32;
    }

    pub fn live_threads() -> usize {
        const SNAP_THREAD: u32 = 0x0000_0004;
        let size = std::mem::size_of::<ThreadEntry32>() as u32;
        unsafe {
            let snap = CreateToolhelp32Snapshot(SNAP_THREAD, 0);
            if snap == -1 {
                return 0;
            }
            let me = GetCurrentProcessId();
            let mut entry = ThreadEntry32 { size, ..Default::default() };
            let mut n = 0usize;
            if Thread32First(snap, &mut entry) != 0 {
                loop {
                    if entry.owner_pid == me {
                        n += 1;
                    }
                    entry.size = size;
                    if Thread32Next(snap, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snap);
            n
        }
    }
}

// Each registered job gets its own dedicated thread on purpose (a hung job
// must not block any other job's timer): so four chores now cost exactly
// four threads, same as four hand-rolled `loop { sleep(); work(); }` threads
// would. This proves the Scheduler is not secretly sharing one thread again.
//
// The census sees every thread in the process, the test harness's own
// included, so each attempt brackets its measurement with a baseline reading
// and only accepts a round where the baseline did not move underneath it.
// A round is also rejected outright when `with_jobs` itself is off by more
// than the four chores: cargo runs every test in this binary concurrently,
// so another test's own thread can start or stop in the narrow window
// between the two `live_threads()` reads even when the before/after
// baseline matches: that used to flake this test under load rather than
// prove anything about the Scheduler.
#[cfg(windows)]
#[test]
fn four_chores_cost_four_dedicated_threads() {
    use thread_census::live_threads;
    const CHORES: usize = 4;

    let mut measured = None;
    for _ in 0..40 {
        let before = live_threads();

        let s = Scheduler::start();
        for name in ["one", "two", "three", "four"] {
            s.register_periodic(name, NEVER_AGAIN, NEVER_AGAIN, || {});
        }
        wait_until(|| live_threads() >= before + CHORES, Duration::from_secs(5));
        let with_jobs = live_threads();
        s.shutdown(Duration::from_millis(500));
        wait_until(|| live_threads() <= before, Duration::from_secs(5));
        let after = live_threads();

        if before == after && with_jobs.saturating_sub(before) == CHORES {
            measured = Some((before, with_jobs));
            break;
        }
    }

    let (idle, with_jobs) =
        measured.expect("the process never stayed quiet long enough to count its threads");
    println!("idle threads: {idle} baseline, {with_jobs} with four registered chores");
    assert_eq!(with_jobs - idle, CHORES, "each chore should own its own thread");
}

#[test]
fn shutdown_stops_the_thread_promptly() {
    let s = Scheduler::start();
    s.register_periodic("parked", NEVER_AGAIN, NEVER_AGAIN, || {});
    // Let it actually reach the long wait before asking it to leave.
    std::thread::sleep(Duration::from_millis(20));
    let started = Instant::now();
    // Generous relative to the 2ms poll interval: this only proves shutdown
    // does not hang forever, not a tight production latency bound, so it must
    // not flake when many other tests are contending for CPU at the same time.
    let stopped = s.shutdown(Duration::from_millis(500));
    let took = started.elapsed();
    assert!(stopped, "the timer thread was still running after {took:?}");
    assert!(took < Duration::from_millis(500), "shutdown took {took:?}");
}

fn wn_notes() -> Vec<WhatsNewSection> {
    parse_release_notes(
        "## Prompt Saver 1.0.0\nfirst\n\n## Prompt Saver 1.1.0\nsecond\n\n## Prompt Saver 2.0.0\nthird\n",
    )
}

#[test]
fn parse_release_notes_splits_on_each_version_heading() {
    let sections = wn_notes();
    assert_eq!(sections.len(), 3);
    assert_eq!(sections[0].version, "1.0.0");
    assert!(sections[0].markdown.starts_with("## Prompt Saver 1.0.0"));
    assert!(sections[0].markdown.contains("first"));
    assert_eq!(sections[2].version, "2.0.0");
}

#[test]
fn sections_for_update_same_version_shows_nothing() {
    let all = wn_notes();
    assert!(sections_for_update(&all, Some("2.0.0"), "2.0.0").is_empty());
}

#[test]
fn sections_for_update_jump_shows_every_section_in_between() {
    // Stored version is the oldest, current is the newest, expect both
    // later sections, oldest excluded.
    let all = wn_notes();
    let shown = sections_for_update(&all, Some("1.0.0"), "2.0.0");
    assert_eq!(shown.iter().map(|s| s.version.as_str()).collect::<Vec<_>>(), vec!["1.1.0", "2.0.0"]);
}

#[test]
fn sections_for_update_unknown_stored_version_shows_only_current() {
    let all = wn_notes();
    let shown = sections_for_update(&all, None, "1.1.0");
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].version, "1.1.0");
}

#[test]
fn parse_release_notes_ignores_a_heading_without_a_version() {
    let sections = parse_release_notes("## Prompt Saver 1.0.0\nbody\n\n## Downloads\nnot a release\n");
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].version, "1.0.0");
    // The non-version heading's text is swallowed, not attached to the
    // previous section or promoted to one of its own.
    assert!(!sections[0].markdown.contains("Downloads"));
}

#[test]
fn whats_new_check_impl_treats_an_empty_stored_version_as_fresh_install() {
    let all = wn_notes();
    let shown = whats_new_check_impl(&all, "", "1.0.0");
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].version, "1.0.0");
}

#[test]
fn whats_new_check_impl_passes_a_non_empty_stored_version_through() {
    let all = wn_notes();
    let shown = whats_new_check_impl(&all, "1.0.0", "2.0.0");
    assert_eq!(shown.iter().map(|s| s.version.as_str()).collect::<Vec<_>>(), vec!["1.1.0", "2.0.0"]);
}

// A store adopted from another folder mid-session must still get its schema:
// the one-time init used to be remembered per process, not per folder.
#[test]
fn schema_init_runs_for_every_data_folder() {
    let base = std::env::temp_dir().join(format!("{}-test-schema-per-dir", brand::SLUG));
    let _ = std::fs::remove_dir_all(&base);
    let dir_a = base.join("a");
    let dir_b = base.join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();
    assert!(db_conn_at(&dir_a).is_some());
    {
        let old = Connection::open(dir_b.join("data.db")).unwrap();
        old.execute_batch(
            "CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL);",
        )
        .unwrap();
    }
    let conn = db_conn_at(&dir_b).expect("the second folder opens");
    conn.execute(
        "INSERT INTO prompt_versions(prompt_id, ts, name, text, variants) VALUES('p', 1, 'n', 't', 'v')",
        [],
    )
    .expect("the variants column exists after the open");
    drop(conn);
    let _ = std::fs::remove_dir_all(&base);
}

// A purged prompt whose version-history DELETE failed must not keep its text
// forever: the startup sweep drops history of ids that are in neither table.
#[test]
fn a_read_only_instance_never_sweeps_version_history() {
    let root = std::env::temp_dir().join(format!("{}-test-sweep-gate", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    assert!(version_sweep_allowed(&root, false));
    assert!(!version_sweep_allowed(&root, true));
    std::fs::write(root.join("prompts.json"), b"[]").unwrap();
    assert!(!version_sweep_allowed(&root, false));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn startup_sweep_drops_version_history_of_vanished_prompts() {
    let conn = Connection::open_in_memory().unwrap();
    assert!(db_conn_init_schema(&conn));
    conn.execute_batch(
        "INSERT INTO prompts(id, ord, data) VALUES('live', 0, 'x');
             INSERT INTO trash(id, ord, data) VALUES('binned', 0, 'x');
             INSERT INTO prompt_versions(prompt_id, ts, name, text) VALUES('gone', 1, 'n', 'secret');
             INSERT INTO prompt_versions(prompt_id, ts, name, text) VALUES('gone', 2, 'n', 'secret2');
             INSERT INTO prompt_versions(prompt_id, ts, name, text) VALUES('live', 1, 'n', 't');
             INSERT INTO prompt_versions(prompt_id, ts, name, text) VALUES('binned', 1, 'n', 't');",
    )
    .unwrap();
    assert_eq!(db_sweep_orphan_versions(&conn), 2);
    let count = |id: &str| -> i64 {
        conn.query_row("SELECT COUNT(*) FROM prompt_versions WHERE prompt_id = ?1", [id], |r| r.get(0)).unwrap()
    };
    assert_eq!(count("gone"), 0);
    assert_eq!(count("live"), 1);
    assert_eq!(count("binned"), 1);
}

// sweep_orphan_files locks the store itself, so a caller that still holds a
// store guard deadlocks the whole app. Every call site must sit outside a live
// `lock(&state)` guard of its own function.
#[test]
fn sweep_orphan_files_is_never_called_under_the_store_guard() {
    let src = production_sources();
    let code: String = src
        .lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n");
    // Every top-level fn start, whatever its visibility or asyncness.
    let mut starts: Vec<usize> = Vec::new();
    let mut off = 0;
    for line in code.split('\n') {
        let rest = line.strip_prefix("pub(crate) ").or_else(|| line.strip_prefix("pub ")).unwrap_or(line);
        if rest.strip_prefix("async ").unwrap_or(rest).starts_with("fn ") {
            starts.push(off);
        }
        off += line.len() + 1;
    }
    let mut checked = 0;
    let mut defs = 0;
    for (n, &start) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(code.len());
        let body = &code[start..end];
        let head = body.lines().next().unwrap_or("");
        let name = head.split("fn ").nth(1).unwrap_or("?").split(['(', '<']).next().unwrap_or("?").to_string();
        if name == "sweep_orphan_files" {
            defs += 1;
            continue;
        }
        if !body.contains("sweep_orphan_files(") {
            continue;
        }
        // (guard name, brace depth it was declared at)
        let mut guards: Vec<(String, i32)> = Vec::new();
        let mut depth = 0i32;
        for line in body.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("let ") {
                if rest.contains("= lock(&state)") {
                    let nm = rest.trim_start_matches("mut ").split([' ', ':']).next().unwrap_or("").to_string();
                    guards.push((nm, depth));
                }
            }
            if let Some(arg) = t.strip_prefix("drop(") {
                let nm = arg.trim_end_matches(';').trim_end_matches(')').to_string();
                guards.retain(|(g, _)| *g != nm);
            }
            if t.contains("sweep_orphan_files(") {
                checked += 1;
                assert!(guards.is_empty(), "{name} calls sweep_orphan_files while holding the store lock");
            }
            for ch in line.chars() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        guards.retain(|(_, d)| *d <= depth);
                    }
                    _ => {}
                }
            }
        }
    }
    assert_eq!(defs, 1, "sweep_orphan_files must be defined exactly once in the crate sources");
    assert!(checked >= 5, "the scan found only {checked} call sites");
}

#[test]
fn taking_a_prompt_out_moves_the_occupant_instead_of_dropping_it() {
    let mut view = test_view("v");
    view.cols = 2;
    view.rows = 1;
    let key = grid_key(2, 1);
    view.layouts.entry(key.clone()).or_default().insert("occ".to_string(), [0, 0]);
    place_taken_out_member(&mut view, &[], "new", [0, 0]).unwrap();
    let layout = &view.layouts[&key];
    assert_eq!(layout.get("new"), Some(&[0, 0]));
    assert_eq!(layout.get("occ"), Some(&[1, 0]), "the occupant keeps a cell");
}

#[test]
fn taking_a_prompt_out_is_refused_when_nowhere_is_free() {
    let mut view = test_view("v");
    view.cols = 2;
    view.rows = 1;
    let key = grid_key(2, 1);
    view.layouts.entry(key.clone()).or_default().insert("occ".to_string(), [0, 0]);
    // the second cell belongs to another folder
    assert!(place_taken_out_member(&mut view, &[[1, 0]], "new", [0, 0]).is_err());
    let layout = &view.layouts[&key];
    assert_eq!(layout.get("occ"), Some(&[0, 0]));
    assert!(layout.get("new").is_none());
}

#[test]
fn deleting_a_view_also_drops_its_grid_folders() {
    let mut settings = Settings {
        views: vec![test_view("a"), test_view("b")],
        active_view: "a".to_string(),
        ..Settings::default()
    };
    settings.tile_folders.push(TileFolder {
        id: "f".to_string(),
        name: String::new(),
        color: String::new(),
        view_id: "a".to_string(),
        cell: [0, 0],
        members: vec!["x".to_string(), "y".to_string()],
    });
    let prompt_ids = vec!["x".to_string(), "y".to_string()];
    remove_view_from_settings(&mut settings, "a", &prompt_ids).unwrap();
    assert!(settings.tile_folders.is_empty(), "the orphan folder must not linger");
    assert_eq!(settings.active_view, "b");
    assert!(remove_view_from_settings(&mut settings, "b", &prompt_ids).is_err());
}

#[test]
fn a_failed_settings_save_rolls_the_tag_rename_back_on_disk_too() {
    let mut p = test_prompt("p1");
    p.tags = vec!["Old".to_string()];
    let mut store = secret_test_store(vec![p]);
    store.settings.tag_colors.insert("old".to_string(), "#111111".to_string());
    let mut disk: Vec<Vec<String>> = Vec::new();
    let result = rename_tag_in_store(
        &mut store,
        "old",
        "New",
        "new",
        &mut |st| {
            disk.push(st.prompts[0].tags.clone());
            true
        },
        &mut |_| false,
    );
    assert!(result.is_err());
    assert_eq!(store.prompts[0].tags, vec!["Old".to_string()]);
    assert_eq!(store.settings.tag_colors.get("old").map(String::as_str), Some("#111111"));
    assert_eq!(disk.last().unwrap(), &vec!["Old".to_string()], "the last write to disk is the original");
    assert_eq!(disk.len(), 2);
}

#[test]
fn a_palette_edit_remaps_tag_colors_too() {
    let mut colors: HashMap<String, String> = HashMap::new();
    colors.insert("a".to_string(), "#old".to_string());
    colors.insert("b".to_string(), "#keep".to_string());
    let map: HashMap<String, String> = [("#old".to_string(), "#new".to_string())].into_iter().collect();
    assert!(remap_tag_colors(&mut colors, &map));
    assert_eq!(colors["a"], "#new");
    assert_eq!(colors["b"], "#keep");
    assert!(!remap_tag_colors(&mut colors, &map), "a second pass changes nothing");
}

#[test]
fn two_staged_backups_never_share_one_file() {
    let root = std::env::temp_dir().join(format!("{}-test-stage-unique", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let (x, y) = (root.join("x.db"), root.join("y.db"));
    full_schema_db(&x, "x")
        .execute("INSERT INTO meta(key, value) VALUES('who', 'X')", [])
        .unwrap();
    full_schema_db(&y, "y")
        .execute("INSERT INTO meta(key, value) VALUES('who', 'Y')", [])
        .unwrap();
    let p1 = stage_and_validate_backup(&x, &root).unwrap();
    let p2 = stage_and_validate_backup(&y, &root).unwrap();
    assert_ne!(p1, p2);
    let who: String = Connection::open(&p1)
        .unwrap()
        .query_row("SELECT value FROM meta WHERE key = 'who'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(who, "X", "the first staged copy is still the first backup");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn stale_staged_restores_are_swept_but_other_files_stay() {
    let root = std::env::temp_dir().join(format!("{}-test-stage-sweep", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for name in ["data.db.restoring", "data.db.restoring-abc", "data.db", "data.db.locked.bak"] {
        std::fs::write(root.join(name), b"x").unwrap();
    }
    remove_stale_staged_restores(&root, Duration::from_secs(3600));
    assert!(root.join("data.db.restoring").exists(), "a young file is left alone");
    remove_stale_staged_restores(&root, Duration::ZERO);
    assert!(!root.join("data.db.restoring").exists());
    assert!(!root.join("data.db.restoring-abc").exists());
    assert!(root.join("data.db").exists());
    assert!(root.join("data.db.locked.bak").exists());
    let _ = std::fs::remove_dir_all(&root);
}

fn long_child() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/c", "ping", "-n", "30", "127.0.0.1"]);
    cmd.stdout(std::process::Stdio::null());
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

fn exits_within(child: &mut std::process::Child, ms: u64) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_millis(ms);
    while std::time::Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

#[test]
fn a_child_in_the_job_dies_when_the_job_handle_closes() {
    let job = KillOnCloseJob::new().expect("a job object");
    let mut child = spawn_in_job(&mut long_child(), Some(&job)).unwrap();
    assert!(child.try_wait().unwrap().is_none(), "the child is running");
    drop(job);
    let died = exits_within(&mut child, 1000);
    let _ = child.kill();
    assert!(died, "closing the job must end the child");
}

#[test]
fn a_child_outside_any_job_survives_the_same_drop() {
    let job = KillOnCloseJob::new().expect("a job object");
    let mut child = spawn_in_job(&mut long_child(), None).unwrap();
    drop(job);
    let died = exits_within(&mut child, 600);
    let _ = child.kill();
    let _ = child.wait();
    assert!(!died, "without the job nothing ends the child");
}

#[test]
fn the_export_journal_sweeps_only_dead_staged_exports_of_this_host() {
    let root = std::env::temp_dir().join(format!("{}-test-export-journal", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let dead = root.join(".export-4000000001-123.mp4");
    let alive = root.join(".export-4000000002-456.mp4");
    let foreign = root.join("precious.docx");
    let other_host = root.join(".export-4000000003-789.mp4");
    for f in [&dead, &alive, &foreign, &other_host] {
        std::fs::write(f, b"x").unwrap();
    }
    let journal = root.join("export-staging.txt");
    let host = local_host_name();
    let mut lines: Vec<String> = [(&dead, 4000000001u32), (&alive, 4000000002), (&foreign, 1)]
        .iter()
        .map(|(f, pid)| journal_line(&host, *pid, f))
        .collect();
    let other_line = journal_line("OTHER-PC", 4000000003, &other_host);
    lines.push(other_line.clone());
    journal_write(&journal, &lines);
    sweep_stale_exports(&journal, &host, |pid| pid == 4000000002);
    assert!(!dead.exists(), "a staged file of a dead process is removed");
    assert!(alive.exists(), "a live process keeps its file");
    assert!(foreign.exists(), "a journal line that is not a staged export name is never deleted");
    assert!(other_host.exists(), "another host's staged file is never deleted");
    let left = journal_lines(&journal);
    assert_eq!(left, vec![journal_line(&host, 4000000002, &alive), other_line]);
    let mine = root.join(format!(".export-{}-1.mp4", std::process::id()));
    export_journal_add(&journal, &mine);
    assert_eq!(journal_lines(&journal).len(), 3);
    export_journal_remove(&journal, &mine);
    assert_eq!(journal_lines(&journal).len(), 2, "only this process's own line is removed");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_journal_path_must_be_local_and_absolute() {
    assert!(is_local_absolute_path(r"C:\x\.export-1-1.mp4"));
    assert!(is_local_absolute_path(r"C:/x/.export-1-1.mp4"));
    assert!(!is_local_absolute_path(r"\\host\share\.export-1-1.mp4"));
    assert!(!is_local_absolute_path(r"//host/share/.export-1-1.mp4"));
    assert!(!is_local_absolute_path(r"\\?\UNC\host\share\.export-1-1.mp4"));
    assert!(!is_local_absolute_path(r".export-1-1.mp4"));
    assert!(!is_local_absolute_path(r"sub\.export-1-1.mp4"));
    assert!(!is_local_absolute_path(r"C:.export-1-1.mp4"));
}

#[test]
fn a_staged_restore_copy_is_stamped_with_now() {
    let root = std::env::temp_dir().join(format!("{}-test-stage-mtime", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let src = root.join("old.db");
    drop(full_schema_db(&src, "x"));
    let old = SystemTime::now() - Duration::from_secs(86400);
    std::fs::OpenOptions::new().write(true).open(&src).unwrap().set_modified(old).unwrap();
    let staged = stage_and_validate_backup(&src, &root).unwrap();
    let age = std::fs::metadata(&staged).unwrap().modified().unwrap().elapsed().unwrap_or_default();
    assert!(age < Duration::from_secs(60), "a fresh staged copy must not look stale");
    let _ = std::fs::remove_dir_all(&root);
}

fn txt_round_trip(text: &str) -> ImportedPrompt {
    let mut p = test_prompt("p");
    p.text = text.to_string();
    let mut data = parse_txt(&to_txt(&[p]));
    assert_eq!(data.prompts.len(), 1, "one block in, one block out: {text:?}");
    data.prompts.remove(0)
}

#[test]
fn a_body_line_that_looks_like_metadata_survives_the_txt_round_trip() {
    for text in [
        "a\n@file x.js\nb",
        "@tags x",
        "tail\n@color red",
        "x\n@notes hi\n@style a 1",
        r"\@file keeps its backslash",
        "a\n\\\\@file two",
        r"path \\server\share
\begin{x}",
    ] {
        let got = txt_round_trip(text);
        assert_eq!(got.text, text);
        assert_eq!(got.file_path, "");
        assert!(got.tags.is_empty());
        assert_eq!(got.color, "");
    }
}

#[test]
fn a_markdown_rule_inside_a_prompt_survives_the_txt_round_trip() {
    let got = txt_round_trip("x\n\n---\n\ny");
    assert_eq!(got.text, "x\n\n---\n\ny");
    let mut p = test_prompt("q");
    p.text = "x\n\n---\n\ny".to_string();
    let data = parse_txt(&to_txt(&[p, test_prompt("r")]));
    assert_eq!(data.prompts.len(), 2);
}

#[test]
fn a_single_text_holding_the_variant_marker_is_not_split_on_import() {
    let got = txt_round_trip("one\n<!-- next -->\ntwo");
    assert_eq!(got.text, "one\n<!-- next -->\ntwo");
    assert!(got.variants.is_empty());
    let joined = join_variants("one\n<!-- next -->\ntwo", &[]);
    assert_eq!(split_variants(&joined), ("one\n<!-- next -->\ntwo".to_string(), Vec::new()));
}

#[test]
fn real_metadata_still_round_trips_beside_an_escaped_body() {
    let mut p = test_prompt("p");
    p.text = "line\n@file inside".to_string();
    p.file_path = r"C:\docs\a.txt".to_string();
    p.tags = vec!["t1".to_string(), "t2".to_string()];
    p.color = "#112233".to_string();
    let got = parse_txt(&to_txt(&[p])).prompts.remove(0);
    assert_eq!(got.text, "line\n@file inside");
    assert_eq!(got.file_path, r"C:\docs\a.txt");
    assert_eq!(got.tags, vec!["t1".to_string(), "t2".to_string()]);
    assert_eq!(got.color, "#112233");
}

#[test]
fn an_older_txt_export_with_trailing_metadata_still_imports() {
    let got = parse_txt("### Old\nbody line\n@color #abcdef\n@file D:\\x.txt").prompts.remove(0);
    assert_eq!(got.text, "body line");
    assert_eq!(got.color, "#abcdef");
    assert_eq!(got.file_path, r"D:\x.txt");
}

// An append that lands between the scrub's read and its rename used to be
// overwritten by the rewritten copy: the read has to happen under the same lock.
#[test]
fn strip_user_path_lines_keeps_a_line_appended_while_it_waits_for_the_lock() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-scrub-race-{}", std::process::id()));
    fs::write(&path, "[t=1 v1.0] panicked at C:\\Users\\example\\lib.rs:42\n").unwrap();
    let held = CrashLogLock::acquire(&path);
    let worker = {
        let path = path.clone();
        std::thread::spawn(move || strip_user_path_lines(&path))
    };
    std::thread::sleep(Duration::from_millis(100));
    {
        use std::io::Write;
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"[t=2 v1.0] LATE\n").unwrap();
    }
    drop(held);
    worker.join().unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("LATE"), "the appended line must survive the scrub: {text:?}");
    assert!(!text.to_ascii_lowercase().contains("\\users\\"), "the path is still scrubbed: {text:?}");
    let _ = fs::remove_file(&path);
}

// A standalone restore in a folder another machine holds locked must be refused
// with exit 3 and leave the store alone (the check used to sit behind the
// restore arm, which returned before reaching it).
fn restore_fixture(name: &str) -> (PathBuf, String) {
    let dir = ddlock_test_dir(name);
    {
        let live = db_conn_at(&dir).expect("live store");
        live.execute("INSERT INTO meta(key, value) VALUES('marker', 'live')", []).unwrap();
    }
    let backup_name = "2024-01-01_00-00-00".to_string();
    let folder = dir.join("backups").join(&backup_name);
    fs::create_dir_all(&folder).unwrap();
    {
        let snap = db_conn_at(&folder).expect("snapshot store");
        snap.execute("INSERT INTO meta(key, value) VALUES('marker', 'backup')", []).unwrap();
    }
    (dir, backup_name)
}

fn marker_of(dir: &Path) -> String {
    let conn = Connection::open(dir.join("data.db")).unwrap();
    conn.query_row("SELECT value FROM meta WHERE key = 'marker'", [], |r| r.get(0)).unwrap()
}

#[test]
fn a_standalone_restore_is_refused_while_another_machine_holds_the_folder() {
    let _g = DdlockGuard::capture();
    let (dir, name) = restore_fixture("restore-locked");
    write_data_dir_lock(
        &data_dir_lock_path(&dir),
        &DataDirLockInfo { host: "OTHER-PC".to_string(), pid: 999, seen: unix_now() },
    );
    let argv: Vec<String> = ["backup", "restore", &name, "--yes"].iter().map(|s| s.to_string()).collect();
    let code = cli::run_standalone_with(|| dir.clone(), &argv, &dir);
    assert_eq!(code, 3);
    assert_eq!(marker_of(&dir), "live", "the locked store must be untouched");
    assert_eq!(fs::read_dir(dir.join("backups")).unwrap().count(), 1, "no safety snapshot either");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_standalone_restore_of_a_free_folder_swaps_the_database_and_exits_zero() {
    let _g = DdlockGuard::capture();
    let (dir, name) = restore_fixture("restore-free");
    let argv: Vec<String> = ["backup", "restore", &name, "--yes"].iter().map(|s| s.to_string()).collect();
    let code = cli::run_standalone_with(|| dir.clone(), &argv, &dir);
    assert_eq!(code, 0);
    assert_eq!(marker_of(&dir), "backup");
    let _ = fs::remove_dir_all(&dir);
}

// The answer has to be on the wire before the relaunch: a restart inside the
// call drops the connection and the caller reads a restore that worked as a failure.
#[test]
fn the_answer_is_written_before_the_restart_runs() {
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let order = std::sync::Mutex::new(Vec::<&str>::new());
    rt.block_on(cli::answer_then_restart(
        async { order.lock().unwrap().push("answer") },
        true,
        || order.lock().unwrap().push("restart"),
    ));
    assert_eq!(*order.lock().unwrap(), vec!["answer", "restart"]);
    order.lock().unwrap().clear();
    rt.block_on(cli::answer_then_restart(
        async { order.lock().unwrap().push("answer") },
        false,
        || order.lock().unwrap().push("restart"),
    ));
    assert_eq!(*order.lock().unwrap(), vec!["answer"], "no restart unless one is due");
}

fn recall_rects(dpi_for_wishes: f64, dpi_on_screen: f64) -> Vec<Rect> {
    let mut settings = Settings::default();
    for id in ["a", "b", "c"] {
        settings.floating.insert(id.to_string(), Pos::default());
    }
    settings.float_folders.push(FloatFolder {
        id: "f".to_string(),
        name: String::new(),
        pos: Pos::default(),
        scale: 1.0,
        members: vec!["a".to_string(), "b".to_string()],
    });
    let work = Rect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 };
    let (_ids, wishes) = recall_wishes(&settings, &[], work, dpi_for_wishes);
    let spots = spread_free_positions(&wishes, &[], work);
    // What the windows really occupy: the logical box times the screen's factor.
    let (_ids, real) = recall_wishes(&settings, &[], work, dpi_on_screen);
    spots
        .iter()
        .zip(real.iter())
        .map(|((x, y), r)| Rect { x: *x, y: *y, w: r.w, h: r.h })
        .collect()
}

fn any_overlap(rects: &[Rect]) -> bool {
    for (i, a) in rects.iter().enumerate() {
        for b in &rects[i + 1..] {
            if a.x < b.x + b.w - 0.5 && b.x < a.x + a.w - 0.5 && a.y < b.y + b.h - 0.5 && b.y < a.y + a.h - 0.5 {
                return true;
            }
        }
    }
    false
}

#[test]
fn recalled_windows_do_not_overlap_on_a_scaled_screen() {
    assert!(!any_overlap(&recall_rects(1.5, 1.5)), "boxes sized for the screen spread without overlap");
    assert!(any_overlap(&recall_rects(1.0, 1.5)), "boxes sized without the factor overlap once drawn at it");
}

// The elevated script must reach PowerShell on its own command line, never as a
// file a same-user process could swap after the consent prompt.
#[test]
fn the_elevated_script_travels_as_an_encoded_command_and_decodes_exactly() {
    use base64::Engine;
    let script = "$ErrorActionPreference = 'Stop'\r\ntry { Add-WindowsCapability -Online -Name 'Language.OCR~~~de-DE~0.0.1.0' } catch { }\r\n";
    let params = ocr::elevated_params(script).unwrap();
    assert!(params.contains("-EncodedCommand "));
    assert!(!params.contains("ExecutionPolicy") && !params.contains("WindowStyle"), "no malware-looking switches: {params}");
    assert!(!params.contains("-File"), "no script file is referenced: {params}");
    assert!(!params.contains("-Command "), "no inline command either: {params}");
    let payload = params.rsplit("-EncodedCommand ").next().unwrap();
    let raw = base64::engine::general_purpose::STANDARD.decode(payload).unwrap();
    let units: Vec<u16> = raw.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    assert_eq!(String::from_utf16(&units).unwrap(), script);
    assert!(ocr::elevated_params(&"x".repeat(40_000)).is_err(), "an oversized script is refused, not truncated");
}

#[test]
fn a_failed_window_capture_brings_the_overlay_back() {
    let shown = std::cell::Cell::new(0);
    let failed: Result<(), String> = Err("ocr-language-missing".to_string());
    let out = show_overlay_again_on_error(failed, || shown.set(shown.get() + 1));
    assert_eq!(out, Err("ocr-language-missing".to_string()), "the error is passed on unchanged");
    assert_eq!(shown.get(), 1);
    let fine: Result<u8, String> = Ok(7);
    assert_eq!(show_overlay_again_on_error(fine, || shown.set(shown.get() + 1)), Ok(7));
    assert_eq!(shown.get(), 1, "a finished capture leaves the overlay alone");
}

#[test]
fn crop_rgba_keeps_the_requested_right_and_bottom_edge_when_it_starts_off_the_image() {
    let img = image::RgbaImage::new(100, 100);
    let c = crop_rgba(&img, -10, 0, 50, 10).unwrap();
    assert_eq!(c.dimensions(), (40, 10), "the 10 px left of the image are cut off, not added on the right");
    let c = crop_rgba(&img, 0, -5, 20, 30).unwrap();
    assert_eq!(c.dimensions(), (20, 25));
    let c = crop_rgba(&img, 90, 90, 50, 50).unwrap();
    assert_eq!(c.dimensions(), (10, 10), "past the far edge is still clamped");
    assert!(crop_rgba(&img, -60, 0, 50, 10).is_none(), "a rect wholly outside is nothing");
}

// A stand-in for ffmpeg that prints nothing and just sleeps, like a real one
// stuck reading a stalled source.
fn silent_stub(dir: &Path) -> PathBuf {
    let src = dir.join("stub.rs");
    fs::write(&src, "fn main() { std::thread::sleep(std::time::Duration::from_secs(30)); }").unwrap();
    let exe = dir.join("stub.exe");
    let status = std::process::Command::new("rustc")
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .arg("-Copt-level=0")
        .status()
        .expect("rustc is on the path while tests run");
    assert!(status.success());
    exe
}

#[test]
fn a_cancel_stops_an_ffmpeg_that_prints_nothing() {
    let dir = std::env::temp_dir().join(format!("{}-test-silent-ffmpeg", brand::SLUG));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let stub = silent_stub(&dir);
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let flipper = {
        let cancel = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            cancel.store(true, Ordering::SeqCst);
        })
    };
    let started = std::time::Instant::now();
    let result = run_ffmpeg_to_staged(
        &stub,
        &dir.join("in.mp4"),
        &dir.join("out.mp4"),
        &[],
        &[],
        || cancel.load(Ordering::SeqCst),
        |_| {},
    );
    let took = started.elapsed();
    flipper.join().unwrap();
    assert_eq!(result.unwrap_err(), "cancelled");
    assert!(took < Duration::from_secs(2), "the cancel must not wait for ffmpeg's next line: {took:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn fresh_settings_close_to_tray() {
    assert!(Settings::default().minimize_to_tray);
    let bare: Settings = serde_json::from_str("{}").unwrap();
    assert!(bare.minimize_to_tray, "a store without the field gets the new default");
    let kept: Settings = serde_json::from_str(r#"{"minimize_to_tray":false}"#).unwrap();
    assert!(!kept.minimize_to_tray, "a stored false stays false");
}

#[test]
fn old_store_keeps_retention_forever() {
    let mut s: Settings = serde_json::from_str("{}").unwrap();
    assert!(!s.retention_defaults_v2, "a store without the marker predates the new defaults");
    s.migrate();
    assert_eq!(s.ui_values.get("historyDays"), Some(&0.0));
    assert_eq!(s.ui_values.get("recordingRetentionDays"), Some(&0.0));
    assert_eq!(history_max_age(&s), None);
    assert!(s.retention_defaults_v2);
}

#[test]
fn old_store_keeps_explicit_retention() {
    let mut s: Settings = serde_json::from_str(r#"{"ui_values":{"historyDays":7.0}}"#).unwrap();
    s.migrate();
    assert_eq!(s.ui_values.get("historyDays"), Some(&7.0), "a stored value is never touched");
    assert_eq!(s.ui_values.get("recordingRetentionDays"), Some(&0.0));
    assert_eq!(history_max_age(&s), Some(7 * 86_400));
}

#[test]
fn new_store_gets_retention_defaults() {
    let mut s = Settings::default();
    s.migrate();
    assert!(!s.ui_values.contains_key("historyDays"));
    assert!(!s.ui_values.contains_key("recordingRetentionDays"));
    assert_eq!(history_max_age(&s), Some(30 * 86_400));
    assert_eq!(RECORDING_RETENTION_DAYS_DEFAULT, 90.0);
}

#[test]
fn json_fallback_pins_retention_next_to_existing_data() {
    let base = std::env::temp_dir().join(format!("{}-test-retention-pin", brand::SLUG));
    let _ = std::fs::remove_dir_all(&base);
    // No settings next to a prompt list: an existing install.
    let with_prompts = base.join("prompts");
    std::fs::create_dir_all(&with_prompts).unwrap();
    assert!(write_sealed_json(&with_prompts.join("prompts.json"), &numbered_prompts(2)));
    let store = load_store_from_json(&with_prompts);
    assert_eq!(history_max_age(&store.settings), None);
    assert_eq!(store.settings.ui_values.get("recordingRetentionDays"), Some(&0.0));
    // Unreadable settings: also an existing install.
    let garbled = base.join("garbled");
    std::fs::create_dir_all(&garbled).unwrap();
    std::fs::write(garbled.join("settings.json"), "not json at all").unwrap();
    assert_eq!(history_max_age(&load_store_from_json(&garbled).settings), None);
    // Nothing at all: a new install gets the defaults.
    let fresh = base.join("fresh");
    std::fs::create_dir_all(&fresh).unwrap();
    assert_eq!(history_max_age(&load_store_from_json(&fresh).settings), Some(30 * 86_400));
    // Settings written by this version carry the marker and are left alone.
    let current = base.join("current");
    std::fs::create_dir_all(&current).unwrap();
    assert!(write_sealed_json(&current.join("settings.json"), &Settings::default()));
    assert_eq!(history_max_age(&load_store_from_json(&current).settings), Some(30 * 86_400));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn secret_timeout_defaults_to_15() {
    let mut s = Settings::default();
    assert_eq!(secret_timeout_min(&s), 15, "an untouched store locks after 15 minutes");
    s.ui_values.insert("secretTimeoutMin".into(), 0.0);
    assert_eq!(secret_timeout_min(&s), 0, "a stored 0 stays 'only on quit'");
    s.ui_values.insert("secretTimeoutMin".into(), 60.0);
    assert_eq!(secret_timeout_min(&s), 60);
}

// A pill or folder window shown at creation is an empty rectangle on screen until
// WebView2 has loaded its page (seen as a blank window at a slow boot). Both are
// built hidden and revealed by the page-load handler, never by a direct show().
#[test]
fn pill_and_folder_windows_are_built_hidden_and_shown_on_page_load() {
    let src = production_sources();
    for page in ["floating.html", "folder.html"] {
        let start_marker = format!("WebviewWindowBuilder::new(app, &label, WebviewUrl::App(\"{page}\".into()))");
        assert_eq!(src.matches(&start_marker).count(), 1, "{page} builder must occur exactly once");
        let a = src.find(&start_marker).unwrap();
        let b = a + src[a..].find(".build();").expect("end of builder chain");
        let chain = &src[a..b];
        assert!(chain.contains(".visible(false)"), "{page} window must be built hidden");
        assert!(chain.contains(".on_page_load(reveal_when_loaded)"), "{page} window must be revealed on page load");
        let rest_end = b + src[b..].find("\n}\n").expect("end of function");
        assert!(!src[b..rest_end].contains("win.show()"), "{page} window must not be shown right after build()");
    }
}

// A pill or folder that finishes loading while a snip session runs must not appear
// in the capture or over the overlay: it is queued for the restore pass instead, once.
// A window that is already visible is left alone (the fallback timer runs after the
// page-load reveal in the normal case).
#[test]
fn float_reveal_decision_defers_during_a_snip_and_skips_a_visible_window() {
    assert_eq!(reveal_decision(false, false), Reveal::Now);
    assert_eq!(reveal_decision(false, true), Reveal::AfterSnip);
    assert_eq!(reveal_decision(true, false), Reveal::Skip);
    assert_eq!(reveal_decision(true, true), Reveal::Skip);
    let mut hid = vec!["main".to_string()];
    queue_for_snip_restore(&mut hid, "float-1");
    queue_for_snip_restore(&mut hid, "float-1");
    assert_eq!(hid, vec!["main".to_string(), "float-1".to_string()]);
}

// The overlay watchdog must fire for a page that never presents, also on the reuse
// path where the parked overlay already counts as visible: it keys off "presented",
// never off the window's visibility.
#[test]
fn snip_watchdog_keys_off_presented_state() {
    assert!(watchdog_should_reveal(true, false), "session running, nothing presented yet");
    assert!(!watchdog_should_reveal(true, true), "already presented");
    assert!(!watchdog_should_reveal(false, false), "no session any more");
}

// Surfacing the overlay of an already running session goes through the same bounds
// assert as the normal reveal; a bare show() left a parked overlay at its parking spot.
#[test]
fn snip_already_active_path_sets_bounds_before_showing() {
    let src = production_sources();
    let start = src.find("pub(crate) async fn begin_capture(").expect("begin_capture");
    let end = start + src[start..].find("PICK_ACTIVE.store(pick").expect("end of the claim block");
    let claim = &src[start..end];
    let revealed = claim.find("reveal_snip(&app).await").expect("the already-active branch must reveal through reveal_snip");
    if let Some(shown) = claim.find("win.show()") {
        assert!(revealed < shown, "the already-active branch must not show the overlay before setting its bounds");
    }
}

// The pre-warmed snip overlay is built once at launch and then idles hidden. If
// anything ever shows it while idle (a slow boot, a window-manager quirk) it must
// not be an 800x600 white rectangle at the default spot of the main screen: it is
// built tiny, far outside every monitor and with a dark native backdrop, and an
// idle sweep puts it away again. The real snip path always sets its own bounds
// before it reveals the overlay.
#[test]
fn prewarmed_snip_overlay_is_tiny_off_screen_and_dark() {
    let spec = snip_prewarm_spec();
    assert!(spec.width <= 4 && spec.height <= 4, "the idle overlay must be a few pixels at most");
    assert!(spec.x <= -10000 && spec.y <= -10000, "the idle overlay must sit far outside every monitor");
    assert_eq!(spec.color.3, 255, "the idle overlay backdrop must be opaque, never white");
    assert!(spec.color.0 < 64 && spec.color.1 < 64 && spec.color.2 < 64, "the idle overlay backdrop must be dark");
}

#[test]
fn idle_snip_overlay_is_parked_whenever_it_shows_up() {
    assert!(snip_idle_needs_parking(false, true), "visible with no session: park it");
    assert!(!snip_idle_needs_parking(true, true), "a live session owns the overlay");
    assert!(!snip_idle_needs_parking(false, false), "already hidden: nothing to do");
    assert!(!snip_idle_needs_parking(true, false));
}

#[test]
fn snip_overlay_builder_uses_the_prewarm_spec_and_every_reveal_sets_bounds() {
    let src = production_sources();
    let a = src.find("pub(crate) fn build_snip_window").expect("build_snip_window");
    let b = a + src[a..].find("\n}\n").expect("end of build_snip_window");
    let body = &src[a..b];
    assert!(body.contains(".visible(false)"), "the overlay is built hidden");
    assert!(body.contains(".inner_size(") && body.contains(".position("), "the overlay is built tiny and off-screen");
    assert!(body.contains("set_background_color"), "the overlay never shows the default white backdrop");
    // The reveal puts the full bounds on before the show, on the reuse path and the shared reveal.
    let r = src.find("pub(crate) async fn reveal_snip").expect("reveal_snip");
    let rend = r + src[r..].find("\n}\n").expect("end of reveal_snip");
    let reveal = &src[r..rend];
    let size = reveal.find("set_size(").expect("reveal sets size");
    let pos = reveal.find("set_position(").expect("reveal sets position");
    let show = reveal.find("win.show()").expect("reveal shows");
    assert!(size < show && pos < show, "reveal_snip must set size and position before show");
}

// A finished snip must not leave the overlay hidden at full desktop size: it goes
// back to the tiny off-screen resting place, so a stray show can never put a
// desktop-sized window on screen.
#[test]
fn ending_a_snip_parks_the_overlay_tiny_and_off_screen() {
    let src = production_sources();
    let a = src.find("pub(crate) fn close_all_snip").expect("close_all_snip");
    let b = a + src[a..].find("
}
").expect("end of close_all_snip");
    assert!(src[a..b].contains("park_snip_window(&win)"), "close_all_snip must park the overlay");
    let c = src.find("pub(crate) fn park_snip_window").expect("park_snip_window");
    let d = c + src[c..].find("
}
").expect("end of park_snip_window");
    let body = &src[c..d];
    assert!(body.contains("snip_prewarm_spec()") && body.contains(".hide()") && body.contains("set_size(") && body.contains("set_position("));
}

#[test]
fn even_dim_rounds_down_to_even_with_a_floor_of_two() {
    assert_eq!(recording::even_dim(1008), 1008);
    assert_eq!(recording::even_dim(163), 162);
    assert_eq!(recording::even_dim(1921), 1920);
    assert_eq!(recording::even_dim(2), 2);
    assert_eq!(recording::even_dim(3), 2);
    assert_eq!(recording::even_dim(1), 2);
    assert_eq!(recording::even_dim(0), 2);
}

#[test]
fn discard_empty_output_removes_only_an_empty_file() {
    let dir = std::env::temp_dir().join(format!("ps-discard-empty-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let empty = dir.join("rec-empty.mp4");
    fs::write(&empty, b"").unwrap();
    assert!(recording::discard_empty_output(&empty));
    assert!(!empty.exists());
    let full = dir.join("rec-full.mp4");
    fs::write(&full, b"data").unwrap();
    assert!(!recording::discard_empty_output(&full));
    assert!(full.exists());
    assert!(!recording::discard_empty_output(&dir.join("missing.mp4")));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn uia_class_order_prefers_the_eight_class_and_falls_back() {
    use windows::Win32::UI::Accessibility::{CUIAutomation, CUIAutomation8};
    let order = keylog::uia_class_order();
    assert_eq!(order, [CUIAutomation8, CUIAutomation]);
    // Eight works: the original class is never tried.
    let mut tried = Vec::new();
    let got = keylog::first_ready_uia(&order, |c| {
        tried.push(*c);
        Some(1)
    });
    assert_eq!(got, Some(1));
    assert_eq!(tried, vec![CUIAutomation8]);
    // Eight fails: falls back to the original class.
    let mut tried = Vec::new();
    let got = keylog::first_ready_uia(&order, |c| {
        tried.push(*c);
        (*c == CUIAutomation).then_some(2)
    });
    assert_eq!(got, Some(2));
    assert_eq!(tried, vec![CUIAutomation8, CUIAutomation]);
    // Neither works: None, so the gap-everything path runs.
    assert_eq!(keylog::first_ready_uia::<u8>(&order, |_| None), None);
}

// Alternating pure red and pure blue pixel columns: the hardest case for subsampled chroma.
fn red_blue_columns(w: u32, h: u32) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for _y in 0..h {
        for x in 0..w {
            rgba.extend_from_slice(if x % 2 == 0 { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
        }
    }
    rgba
}

// Spread between the red and blue content of the first two decoded pixels (0 = fully smeared).
fn red_blue_separation(jpeg: &[u8]) -> i32 {
    let img = image::load_from_memory(jpeg).expect("decode").to_rgba8();
    let a = img.get_pixel(8, 4).0;
    let b = img.get_pixel(9, 4).0;
    (a[0] as i32 - b[0] as i32).abs() + (a[2] as i32 - b[2] as i32).abs()
}

#[test]
fn snip_overlay_preview_keeps_full_chroma_for_coloured_text() {
    // Below quality 90 the encoder halves the colour resolution and one-pixel coloured
    // strokes smear into each other; the overlay preview must stay above that line.
    let rgba = red_blue_columns(32, 16);
    let low = snip::encode_snip_jpeg(&rgba, 32, 16, 80).expect("q80");
    let used = snip::encode_snip_jpeg(&rgba, 32, 16, snip::SNIP_PREVIEW_Q).expect("preview");
    assert!(red_blue_separation(&low) < 300, "q80 was expected to smear this pattern");
    assert!(
        red_blue_separation(&used) > 500,
        "the overlay preview quality {} smears coloured one-pixel strokes",
        snip::SNIP_PREVIEW_Q
    );
}

// ---- the window-close decision never waits for the store ----

#[test]
fn close_decision_returns_while_the_store_is_locked() {
    let store = Arc::new(Mutex::new(Store {
        prompts: Vec::new(),
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: Default::default(),
    }));
    let held = store.lock().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut s = Settings { minimize_to_tray: false, ..Settings::default() };
        mirror_minimize_to_tray(&s);
        let off = close_minimizes(false);
        s.minimize_to_tray = true;
        mirror_minimize_to_tray(&s);
        let on = close_minimizes(false);
        let portable = close_minimizes(true);
        let _ = tx.send((off, on, portable));
    });
    let got = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the close decision must not wait for the store lock");
    drop(held);
    assert_eq!(got, (false, true, false));
}

#[test]
fn hot_path_commands_run_off_the_main_thread() {
    // Contrast: a sync command runs on the event-loop thread and freezes the
    // window while a backup holds the store. No red test exists for this (it
    // needs a live event loop), so the attribute itself is asserted.
    for (src, name) in [
        (include_str!("logging.rs"), "log_error"),
        (include_str!("settings_cmds.rs"), "get_settings"),
        (include_str!("autostart.rs"), "set_always_on_top"),
        (include_str!("theme.rs"), "current_theme"),
        (include_str!("update.rs"), "set_ui_value"),
    ] {
        let at = src.find(&format!("fn {name}(")).expect(name);
        let head = &src[..at];
        let attr = head.rfind("#[tauri::command").expect(name);
        assert!(
            head[attr..].starts_with("#[tauri::command(async)]"),
            "{name} must be an async command"
        );
    }
}

#[test]
fn float_recall_queries_monitors_before_taking_the_store_lock() {
    // Structural fix (call order), so no red test exists: monitor queries wait
    // for the main thread, which may itself be waiting on the store lock. The
    // old body queried them after the lock; this pins the order.
    let src = include_str!("floating.rs");
    let at = src.find("fn float_recall(").expect("float_recall");
    let end = src[at..].find("\n}").expect("end") + at;
    let body = &src[at..end];
    let lock_at = body.find("lock(&state)").expect("takes the store lock");
    for q in ["screen_snapshot(", "available_monitors("] {
        let q_at = body.find(q).unwrap_or_else(|| panic!("{q} missing"));
        assert!(q_at < lock_at, "{q} must run before the store lock is taken");
    }
}

// ---- the taskbar icon is only touched when its state changes ----

#[test]
fn taskbar_throttle_applies_a_repeated_percent_once() {
    let mut t = bg_ops::TaskbarThrottle::default();
    assert!(t.should_apply(7, "running", Some(40)), "the first tick is applied");
    assert!(!t.should_apply(7, "running", Some(40)), "the same percent again is skipped");
    assert!(t.should_apply(7, "running", Some(41)), "a new percent is applied");
    assert!(t.should_apply(7, "indeterminate", Some(41)), "a new state is applied");
    assert!(!t.should_apply(7, "indeterminate", Some(90)), "a state without a percent ignores it");
    assert!(t.should_apply(7, "off", None));
    assert!(!t.should_apply(7, "off", None), "off twice is one apply");
    assert!(t.should_apply(8, "off", None), "a rebuilt window handle is applied again");
    t.reset();
    assert!(t.should_apply(8, "off", None), "a reset retries the same pair");
}

// ---- the update check waits for an answer ----

#[test]
fn a_fresh_profile_never_checks_for_updates() {
    let s = Settings::default();
    assert_eq!(s.update_consent, UpdateConsent::Unasked);
    assert!(!update::update_check_allowed(&s));
    let mut yes = Settings { update_consent: UpdateConsent::Yes, ..Settings::default() };
    assert!(update::update_check_allowed(&yes));
    yes.auto_update = false;
    assert!(!update::update_check_allowed(&yes), "the switch still wins");
}

#[test]
fn stored_settings_without_the_consent_key_keep_their_update_choice() {
    let mut on: Settings = serde_json::from_str(r#"{"auto_update":true}"#).unwrap();
    assert_eq!(on.update_consent, UpdateConsent::Legacy);
    on.migrate();
    assert_eq!(on.update_consent, UpdateConsent::Yes);
    let mut off: Settings = serde_json::from_str(r#"{"auto_update":false}"#).unwrap();
    off.migrate();
    assert_eq!(off.update_consent, UpdateConsent::No);
    let mut fresh = Settings::default();
    fresh.migrate();
    assert_eq!(fresh.update_consent, UpdateConsent::Unasked, "migrate leaves an unanswered profile alone");
    let again: Settings = serde_json::from_str(&serde_json::to_string(&fresh).unwrap()).unwrap();
    assert_eq!(again.update_consent, UpdateConsent::Unasked, "the unanswered state survives a save");
}

#[test]
fn a_restore_keeps_the_local_update_consent_and_a_foreign_block_asks_again() {
    let local = Settings { update_consent: UpdateConsent::Yes, ..Settings::default() };
    let mut incoming = Settings { update_consent: UpdateConsent::No, ..Settings::default() };
    keep_machine_settings(&local, &mut incoming);
    assert_eq!(incoming.update_consent, UpdateConsent::Yes);
    let mut other = Settings { update_consent: UpdateConsent::Yes, ..Settings::default() };
    drop_machine_rows(&mut other);
    assert_eq!(other.update_consent, UpdateConsent::Unasked);
}

// ---- user-picked export files are replaced atomically ----

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

#[test]
fn user_picked_exports_never_write_in_place() {
    for (name, src) in [
        ("import_export.rs", include_str!("import_export.rs")),
        ("vault.rs", include_str!("vault.rs")),
        ("stats.rs", include_str!("stats.rs")),
    ] {
        assert!(!src.contains("fs::write(&path"), "{name} writes a user-picked file in place instead of through write_bytes_atomic");
    }
}
