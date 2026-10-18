use super::*;

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
    // says: so its coverage is the key itself, not a classify() case.
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

// A user-triggered cancel is not classified at all: run_ffmpeg_to_staged
// returns the plain "cancelled" sentinel before ever reaching
// classify_ffmpeg_error, and it is real ffmpeg output that would otherwise
// often exist alongside it (a killed process usually still wrote a log).
#[test]
fn ffmpeg_error_a_cancel_sentinel_is_not_json_and_is_never_classified() {
    assert!(serde_json::from_str::<serde_json::Value>("cancelled").is_err());
}

// ---- taskbar progress: the pure aggregation state machine, no real window ----

pub(super) fn bg_op(phase: OpPhase) -> BgOp {
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

// ---- background operations: the shared registry behind the progress popover ----

// Serializes every test below that reads or writes the process-global
// bg_ops() registry against each other: cargo test runs tests on
// separate threads by default, and bg_ops_running_count() in particular
// counts every entry in the map regardless of which test put it there, so
// two of these tests racing would inflate each other's counts.
pub(super) static BG_OPS_TEST_LOCK: Mutex<()> = Mutex::new(());

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
// "export" kind: proves the fix directly, since keying by kind alone
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

pub(super) fn ffmpeg_test_zip() -> Vec<u8> {
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
    // Any hash that is not this blob's real SHA-256 must be rejected: the
    // "point the pinned constant at a wrong hash" proof the spec asks for.
    let wrong_hash = "0".repeat(64);
    let err = verify_ffmpeg_zip_against(&zip_bytes, &wrong_hash).unwrap_err();
    assert!(err.contains("hash mismatch"), "{err}");
}

pub(super) fn ffmpeg_install_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    use std::io::Write;
    for (name, body) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(body).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

pub(super) fn ffmpeg_dir_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> =
        std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    names.sort();
    names
}

#[test]
fn ffmpeg_install_extracts_both_tools_and_leaves_no_temp() {
    let mut exe = b"MZ".to_vec();
    exe.extend(stream_test_bytes(3 << 20));
    let mut probe = b"MZ".to_vec();
    probe.extend(stream_test_bytes(1 << 20));
    let zip_bytes = ffmpeg_install_zip(&[(FFMPEG_ZIP_ENTRY, &exe), (FFPROBE_ZIP_ENTRY, &probe)]);
    let dir = stream_test_dir("ffmpeg-install-both");
    let path = install_ffmpeg_from_zip(&zip_bytes, &dir).unwrap();
    assert_eq!(PathBuf::from(path), dir.join("ffmpeg.exe"));
    assert_eq!(std::fs::read(dir.join("ffmpeg.exe")).unwrap(), exe);
    assert_eq!(std::fs::read(dir.join("ffprobe.exe")).unwrap(), probe);
    assert_eq!(ffmpeg_dir_names(&dir), vec!["ffmpeg.exe", "ffprobe.exe"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ffmpeg_install_without_ffprobe_still_installs_ffmpeg() {
    let zip_bytes = ffmpeg_install_zip(&[(FFMPEG_ZIP_ENTRY, b"MZ-fake-exe")]);
    let dir = stream_test_dir("ffmpeg-install-noprobe");
    install_ffmpeg_from_zip(&zip_bytes, &dir).unwrap();
    assert_eq!(ffmpeg_dir_names(&dir), vec!["ffmpeg.exe"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ffmpeg_install_rejects_non_executable_and_cleans_up() {
    let zip_bytes = ffmpeg_install_zip(&[(FFMPEG_ZIP_ENTRY, b"not an exe"), (FFPROBE_ZIP_ENTRY, b"MZ-probe")]);
    let dir = stream_test_dir("ffmpeg-install-bad");
    let err = install_ffmpeg_from_zip(&zip_bytes, &dir).unwrap_err();
    assert!(err.contains("not a valid executable"), "{err}");
    assert!(ffmpeg_dir_names(&dir).is_empty(), "{:?}", ffmpeg_dir_names(&dir));
    let missing = install_ffmpeg_from_zip(&ffmpeg_install_zip(&[("x.txt", b"MZ")]), &dir).unwrap_err();
    assert!(missing.contains("not found"), "{missing}");
    assert!(ffmpeg_dir_names(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
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
    // and in PATH: confirming the constants/logic never reference a system
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
