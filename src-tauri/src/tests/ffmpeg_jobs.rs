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
    // decision (see FFMPEG_STALL_SECS), never something ffmpeg's own log
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

// Digests computed outside this program (Python hashlib) pin every SHA-256 call site, so the
// swap of the hashing library cannot change a stored id, a pinned check or an import marker.
#[test]
fn sha256_call_sites_keep_their_known_digests() {
    let hex = |b: &[u8]| crate::ffmpeg::hex_lower(&crate::ffmpeg::sha256(b));
    assert_eq!(hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(hex(b"pinned build bytes"), "f27e2019333096dcc037f8ef842178e39b222f4962467ae417fee1cbefb08cee");
    let big = vec![b'a'; 3 * 1024 * 1024 + 17];
    assert_eq!(hex(&big), "8cb91458835f2fefeb12cf01e67a7ea18523a0567d4835a974dc04c2ddbb955b");
    // The streamed file hash agrees with the one-shot hash (the file spans several read buffers).
    let dir = std::env::temp_dir().join(format!("ps-sha-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("big.bin"), &big).unwrap();
    assert_eq!(sha256_file_hex(&dir.join("big.bin")).unwrap(), hex(&big));
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(
        crate::drafts::edit_state_id("video", "D:/clips/a.mp4"),
        "video:2f0193ff1cb41c38789b7ff922b9f7584c9ce6a29d8ea6d185ef93cd8efbd902"
    );
    assert_eq!(
        crate::import_export::import_file_hash(b"name,text\r\nA,B\r\n"),
        "8897fabf711f5de7d98da5b79217b0f5fff79db2b59e6552f17aff3bb58abdce"
    );
}

#[test]
fn ffmpeg_matching_hash_is_accepted() {
    let zip_bytes = ffmpeg_test_zip();
    let real_hash = crate::ffmpeg::hex_lower(&crate::ffmpeg::sha256(&zip_bytes));
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

#[test]
fn a_progress_tick_reads_both_settings_in_one_pass() {
    let mut s = Settings::default();
    assert_eq!(bg_tick_settings(&s), (10.0, true), "defaults: keep ten minutes, taskbar progress on");
    s.ui_values.insert("bgDoneKeepMin".into(), -3.0);
    s.ui_flags.insert("taskbarProgress".into(), false);
    assert_eq!(bg_tick_settings(&s), (0.0, false), "a negative keep time clamps to zero");
}

// The checks below compare multi-line source text; a checkout with Windows line endings
// must read the same as one with Unix endings.
macro_rules! source {
    ($path:literal) => {
        include_str!($path).replace("\r\n", "\n")
    };
}

#[test]
fn an_export_graph_may_only_read_the_source_and_the_media_the_request_names() {
    let dir = std::env::temp_dir().join(format!("ps-export-graph-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec.mp4");
    let badge = dir.join("badge.png");
    let secret = dir.join("notes.txt");
    for p in [&src, &badge, &secret] {
        fs::write(p, b"x").unwrap();
    }
    // A path spelled the way ui/video.js writes it into a graph.
    let ff = |p: &Path| {
        let c = fs::canonicalize(p).unwrap().to_string_lossy().to_string();
        c.trim_start_matches(r"\\?\").replace('\\', "/").replace(':', r"\:")
    };
    let media = vec![badge.to_string_lossy().to_string()];
    let check = |g: &str| validate_export_request_graphs(&src, &media, &[g]);
    // The shapes ui/video.js builds.
    assert!(check("[0:v]fps=30,trim=start=0.000:end=2.000,setpts=PTS-STARTPTS[v0];[v0]scale=320:240,fade=t=in:st=0:d=1.000[vout]").is_ok());
    assert!(check("[0:v]crop=32:32:0:0[vout]").is_ok());
    assert!(check(r"[0:v]drawtext=fontfile='C\:/fonts/a.ttf':x=1:y=2:fontsize=20:fontcolor=0xffffff@1.00:expansion=none:text='hi, there'[vout]").is_ok());
    assert!(check("[0:a]volume=1.00,atempo=1.3[aout]").is_ok());
    assert!(check(&format!("movie='{}'[i0];[0:v][i0]overlay=1:2:enable='between(t,0.000,2.000)'[vout]", ff(&badge))).is_ok());
    assert!(check(&format!("amovie='{}':loop=2[a1];[a1]anull[aout]", ff(&badge))).is_ok());
    assert!(check(&format!("movie='{}'[i0]", ff(&src))).is_ok(), "the source itself is fine");
    assert!(check(&format!("movie=filename='{}'[i0]", ff(&badge))).is_ok());
    // File-reading filters and options, and filters the editor never builds.
    assert!(check(r"[0:v]drawtext=textfile='C\:/Users/x/.ssh/config':fontsize=20[vout]").is_err());
    assert!(check("[0:v]drawtext=fontsize=20:TextFile=x[vout]").is_err());
    assert!(check(r"[0:v]drawtext=/text=C\:/Users/x/secrets.txt[vout]").is_err());
    assert!(check("[0:v]drawtext=fontsize=20:/fontfile=x[vout]").is_err());
    assert!(check("[0:v]scale=/w=x[vout]").is_err());
    assert!(check(r"[0:v]drawtext=a.ttf:hi:C\:/Users/x/secrets.txt[vout]").is_err());
    assert!(check("[0:v]drawtext=a.ttf:hi[vout]").is_ok());
    assert!(check(r"[0:v]subtitles='C\:/x/y.srt'[vout]").is_err());
    assert!(check("[0:v]sendcmd=f=cmds.txt[vout]").is_err());
    assert!(check("[0:v]zmq[vout]").is_err());
    assert!(check(r"[0:v]null[vout];[0:a]ametadata=mode=print:file=C\:/x[aout]").is_err());
    // A movie source that is not the source or a named media file, or has extra options.
    assert!(check(&format!("movie='{}'[i0]", ff(&secret))).is_err());
    assert!(check(r"movie='C\:/Users/x/.ssh/config'[i0]").is_err());
    assert!(check(&format!("movie='{}':lavfi[i0]", ff(&badge))).is_err());
    assert!(check(&format!("movie='{}':format_name=tty[i0]", ff(&badge))).is_err());
    // Quoting cannot hide a second filter: a quoted comma stays part of the value.
    assert!(check("[0:v]scale='iw,textfile=x'[vout]").is_ok());
    assert!(check("[0:v]scale=1:1,movie=x[vout]").is_err());
    // Media named by the request must carry a media extension.
    assert!(validate_export_request_graphs(&src, &[secret.to_string_lossy().to_string()], &["[0:v]null[vout]"]).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_second_run_under_the_same_op_id_gets_its_own_start_time() {
    let mut ops: HashMap<String, BgOp> = HashMap::new();
    let first_start = entry_for_run(&mut ops, "backup", "backup").started_at_ms;
    ops.get_mut("backup").unwrap().phase = OpPhase::Done;
    ops.get_mut("backup").unwrap().finished_at_ms = Some(now_ms());
    std::thread::sleep(std::time::Duration::from_millis(5));
    let second = entry_for_run(&mut ops, "backup", "backup");
    assert!(second.started_at_ms > first_start, "a finished entry is replaced");
    assert!(second.finished_at_ms.is_none());
    let start = second.started_at_ms;
    // A run still going keeps its entry across progress reports.
    assert_eq!(entry_for_run(&mut ops, "backup", "backup").started_at_ms, start);
}

#[test]
fn ffmpeg_download_claim_admits_one_caller_at_a_time() {
    static FLAG: AtomicBool = AtomicBool::new(false);
    let first = FlagClaim::take(&FLAG).expect("the first caller gets the claim");
    assert!(FlagClaim::take(&FLAG).is_none(), "a second start is refused while the first runs");
    drop(first);
    assert!(FlagClaim::take(&FLAG).is_some(), "the claim is free again once the holder is done");
}

#[test]
fn quit_counts_a_live_recording_as_a_running_job() {
    assert_eq!(quit_blockers(0, false), 0, "nothing running quits at once");
    assert_eq!(quit_blockers(0, true), 1, "a recording alone asks first");
    assert_eq!(quit_blockers(2, true), 3);
}

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

#[test]
fn taskbar_throttle_retries_after_a_failed_apply_or_a_recreated_button() {
    let retry = AtomicBool::new(false);
    let mut t = bg_ops::TaskbarThrottle::default();
    assert!(t.decide(&retry, 5, "indeterminate", None));
    assert!(!t.decide(&retry, 5, "indeterminate", None), "unchanged state is skipped");
    // A failed apply (or the button coming back) asks for a retry: the very same
    // state goes out once more, then is skipped again.
    retry.store(true, Ordering::SeqCst);
    assert!(t.decide(&retry, 5, "indeterminate", None));
    assert!(!t.decide(&retry, 5, "indeterminate", None));
    assert!(!retry.load(Ordering::SeqCst), "the request is consumed");
}

pub(super) fn slow_child_command() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    // ping -n 11 runs for about ten seconds; the tests allow a loaded machine a few
    // seconds of slack and still tell a killed child from one that ran to the end.
    let mut cmd = std::process::Command::new("ping");
    cmd.args(["-n", "11", "127.0.0.1"]).creation_flags(0x0800_0000);
    cmd.stdin(std::process::Stdio::null());
    cmd
}

#[test]
fn status_in_job_kills_a_stalled_child_after_the_limit() {
    let began = std::time::Instant::now();
    let err = status_in_job(&mut slow_child_command(), Duration::from_secs(1)).expect_err("the limit must end the wait");
    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    assert!(began.elapsed() < Duration::from_secs(7), "returned late: {:?}", began.elapsed());
    assert_eq!(tool_run_error("ffmpeg", &err), "ffmpeg-timeout");
}

#[test]
fn output_in_job_times_out_and_still_returns_a_quick_childs_output() {
    let began = std::time::Instant::now();
    let err = output_in_job(&mut slow_child_command(), Duration::from_secs(1)).expect_err("the limit must end the wait");
    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    assert!(began.elapsed() < Duration::from_secs(7), "returned late: {:?}", began.elapsed());

    use std::os::windows::process::CommandExt;
    let mut quick = std::process::Command::new("cmd");
    quick.args(["/C", "echo hello"]).creation_flags(0x0800_0000);
    quick.stdin(std::process::Stdio::null());
    let out = output_in_job(&mut quick, Duration::from_secs(20)).expect("a quick child finishes");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("hello"));
}

#[test]
fn wait_with_deadline_leaves_no_child_running() {
    let mut child = slow_child_command().spawn().unwrap();
    let err = wait_with_deadline(&mut child, Duration::from_millis(300)).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    // Killed and reaped: the status is already known.
    assert!(child.try_wait().unwrap().is_some(), "the child was not reaped");
}

#[test]
fn media_tools_are_searched_only_in_app_owned_folders() {
    let exe = PathBuf::from("C:/app");
    let local = PathBuf::from("C:/local/app");
    assert_eq!(media_tool_dirs(Some(exe.clone()), Some(local.clone())), vec![exe.clone(), local.clone()]);
    assert_eq!(media_tool_dirs(None, Some(local.clone())), vec![local]);
    assert!(media_tool_dirs(None, None).is_empty());
    // The data folder is not a parameter any more: a planted copy there cannot be found.
    let src = source!("../ffmpeg.rs");
    let discovery = src.split("pub(crate) fn find_media_tool").nth(1).unwrap().split("pub(crate) fn ffmpeg_search_path").next().unwrap();
    assert!(!discovery.contains("data_dir"));
}

#[test]
fn only_a_silent_ffmpeg_is_declared_stalled() {
    assert!(!ffmpeg_stalled(0));
    assert!(!ffmpeg_stalled(FFMPEG_STALL_SECS));
    assert!(ffmpeg_stalled(FFMPEG_STALL_SECS + 1));
    // Structural: the check must be fed by the time since the last progress tick, never by the
    // total run time, and every tick must restart that clock.
    let src = source!("../ffmpeg.rs");
    assert!(src.contains("ffmpeg_stalled(last_progress.elapsed().as_secs())"));
    assert!(src.contains("last_progress = std::time::Instant::now();"));
    assert!(!src.contains("start.elapsed().as_secs() >"));
}

#[test]
fn the_export_filter_graph_travels_in_a_swept_file() {
    let dst = Path::new(r"C:\clips\talk-edit.mp4");
    let script = filter_script_path(dst);
    assert_eq!(script.parent(), dst.parent());
    assert!(staged_export_pid(&script) == Some(std::process::id()), "the startup sweep must recognise the file");
    let long = "drawbox=x=1:y=2:w=3:h=4:enable='between(t,1,2)'".repeat(2000);
    let root = std::env::temp_dir().join(format!("ps-graph-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let written = write_filter_script(&root.join("out.mp4"), &long).unwrap();
    assert_eq!(fs::read_to_string(&written).unwrap(), long);
    assert!(long.len() > 32767, "this graph is longer than a command line may be");
    remove_filter_script(&written);
    assert!(!written.exists());
    let _ = fs::remove_dir_all(&root);
    // Contrast: the old inline argument is gone from the export path.
    let src = source!("../video_editor.rs");
    assert!(!src.contains("push(\"-filter_complex\".to_string())"));
    assert!(src.contains("\"-/filter_complex\""));
}

#[test]
fn the_ffmpeg_log_buffer_keeps_only_its_newest_bytes() {
    let mut buf = Vec::new();
    for i in 0..1000u32 {
        push_bounded_tail(&mut buf, format!("warning {i}
").as_bytes(), 100);
        assert!(buf.len() <= 100);
    }
    let text = String::from_utf8(buf).unwrap();
    assert!(text.ends_with("warning 999
"));
    assert!(!text.contains("warning 1
"));
    let mut small = b"abc".to_vec();
    push_bounded_tail(&mut small, b"de", 100);
    assert_eq!(small, b"abcde");
}

// Structural fix, no red unit test: child.wait() cannot be made to fail on a real child.
#[test]
fn a_failed_wait_removes_the_staged_export() {
    let src = source!("../ffmpeg.rs");
    let after = src.split("let status = match child.wait() {").nth(1).unwrap();
    let err_arm = after.split("let stderr_text").next().unwrap();
    assert!(err_arm.contains("forget_staged();") && err_arm.contains("fs::remove_file(&staged)"));
}

#[test]
fn an_ffmpeg_older_than_the_export_graph_syntax_is_skipped_on_path() {
    assert_eq!(ffmpeg_major_version("ffmpeg version 7.1-essentials_build-www.gyan.dev Copyright (c) 2000-2024"), Some(7));
    assert_eq!(ffmpeg_major_version("ffmpeg version n6.1.1 Copyright (c) 2000-2023"), Some(6));
    assert_eq!(ffmpeg_major_version("ffmpeg version 9.0.1-essentials_build"), Some(9));
    assert_eq!(ffmpeg_major_version("ffmpeg version N-113000-gabcdef Copyright"), None);
    assert_eq!(ffmpeg_major_version("not an ffmpeg banner"), None);
    assert_eq!(ffmpeg_major_version(""), None);

    let root = std::env::temp_dir().join(format!("ps-pathff-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (old, new) = (root.join("old"), root.join("new"));
    fs::create_dir_all(&old).unwrap();
    fs::create_dir_all(&new).unwrap();
    fs::write(old.join("ffmpeg.exe"), b"x").unwrap();
    fs::write(new.join("ffmpeg.exe"), b"x").unwrap();
    let paths = std::env::join_paths([&old, &new]).unwrap();
    let only_new = |p: &Path| p.starts_with(&new);
    assert_eq!(first_usable_on_path(&paths, "ffmpeg.exe", only_new), Some(new.join("ffmpeg.exe")));
    assert_eq!(first_usable_on_path(&paths, "ffmpeg.exe", |_| false), None);
    assert_eq!(first_usable_on_path(&paths, "missing.exe", |_| true), None);
    let _ = fs::remove_dir_all(&root);

    // The real lookup applies that check to ffmpeg only; ffprobe has no version floor.
    let src = source!("../ffmpeg.rs");
    let finder = src.split("pub(crate) fn find_media_tool").nth(1).unwrap().split("pub(crate) fn ffmpeg_search_path").next().unwrap();
    assert!(finder.contains("path_ffmpeg_is_current"));
}

#[test]
fn an_old_data_folder_ffmpeg_moves_only_when_it_is_the_pinned_build() {
    let root = std::env::temp_dir().join(format!("ps-oldtool-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (data, local) = (root.join("data"), root.join("local"));
    fs::create_dir_all(&data).unwrap();
    let bytes = b"pinned build bytes";
    let want: String = crate::ffmpeg::hex_lower(&crate::ffmpeg::sha256(bytes));

    // The pinned copy moves, and the old place is empty afterwards.
    fs::write(data.join("ffmpeg.exe"), bytes).unwrap();
    assert_eq!(migrate_old_media_tool(&data.join("ffmpeg.exe"), &local, &want), OldToolFate::Moved);
    assert_eq!(fs::read(local.join("ffmpeg.exe")).unwrap(), bytes);
    assert!(!data.join("ffmpeg.exe").exists());

    // Any other file under the name is never moved to the per-user folder and no longer sits
    // under a program name, but it is kept (it may be the user's own ffmpeg), not deleted.
    fs::write(data.join("ffprobe.exe"), b"something else").unwrap();
    assert_eq!(migrate_old_media_tool(&data.join("ffprobe.exe"), &local, &want), OldToolFate::SetAside);
    assert!(!data.join("ffprobe.exe").exists());
    assert!(!local.join("ffprobe.exe").exists());
    assert_eq!(fs::read(data.join("ffprobe.exe.unused")).unwrap(), b"something else");
    // A second one never overwrites the first set-aside file.
    fs::write(data.join("ffprobe.exe"), b"another one").unwrap();
    assert_eq!(migrate_old_media_tool(&data.join("ffprobe.exe"), &local, &want), OldToolFate::SetAside);
    assert_eq!(fs::read(data.join("ffprobe.exe.unused")).unwrap(), b"something else");
    assert_eq!(fs::read(data.join("ffprobe.exe.unused2")).unwrap(), b"another one");

    // A copy already at the new place wins; the old one is removed.
    fs::write(data.join("ffmpeg.exe"), bytes).unwrap();
    assert_eq!(migrate_old_media_tool(&data.join("ffmpeg.exe"), &local, &want), OldToolFate::Dropped);
    assert!(!data.join("ffmpeg.exe").exists());

    // Nothing there, nothing to do.
    assert_eq!(migrate_old_media_tool(&data.join("ffmpeg.exe"), &local, &want), OldToolFate::Absent);

    // The pass leaves a data folder that is also the exe folder (portable) alone.
    fs::write(data.join("ffmpeg.exe"), b"portable copy").unwrap();
    migrate_old_media_tools(&data, Some(&data), Some(&local));
    assert!(data.join("ffmpeg.exe").exists());

    // The pinned hashes are the shipped executables' hashes.
    assert_eq!(FFMPEG_EXE_SHA256.len(), 64);
    assert_eq!(FFPROBE_EXE_SHA256.len(), 64);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn start_up_moves_old_media_tools_off_the_ui_thread() {
    let lib = source!("../lib.rs");
    assert!(lib.contains("migrate_old_media_tools"));
    let at = lib.find("migrate_old_media_tools").unwrap();
    assert!(lib[..at].rfind("std::thread::spawn").is_some_and(|t| at - t < 400), "hashing two 100 MB files must not block setup");
}

#[test]
fn every_progress_block_keeps_a_silent_looking_export_alive() {
    // GIF palettegen prints out_time_ms=N/A until the whole input is read; the block's closing
    // "progress=continue" line is the heartbeat that still arrives.
    assert_eq!(progress_tick("out_time_ms=2500000"), ProgressTick::Time(2500));
    assert_eq!(progress_tick("out_time_ms=-9223372036854775807"), ProgressTick::Time(0));
    assert_eq!(progress_tick("out_time_ms=N/A"), ProgressTick::Alive);
    assert_eq!(progress_tick("progress=continue"), ProgressTick::Alive);
    assert_eq!(progress_tick("progress=end"), ProgressTick::Alive);
    assert_eq!(progress_tick("frame=12"), ProgressTick::None);
    assert_eq!(progress_tick(""), ProgressTick::None);
    let src = source!("../ffmpeg.rs");
    let run = src.split("pub(crate) fn run_ffmpeg_to_staged").nth(1).unwrap();
    assert!(run.contains("progress_tick(&line)"), "the reader thread classifies every line");
}

#[test]
fn bg_folder_open_only_for_a_recorded_target() {
    let mut ops: HashMap<String, BgOp> = HashMap::new();
    let mut job = bg_op(OpPhase::Done);
    job.target = Some(r"D:\Videos\clip.mp4".to_string());
    ops.insert("convert-1".to_string(), job);
    ops.insert("export-2".to_string(), bg_op(OpPhase::Running(10)));
    assert!(is_recorded_bg_target(&ops, r"D:\Videos\clip.mp4"));
    assert!(!is_recorded_bg_target(&ops, r"D:\Videos"), "only the exact recorded target counts");
    assert!(!is_recorded_bg_target(&ops, r"\\evil-host\share"), "a path no job recorded is refused");
    assert!(!is_recorded_bg_target(&HashMap::new(), r"D:\Videos\clip.mp4"));
    // The command also refuses a recorded network target outside the app's own folders.
    let src = source!("../bg_ops.rs");
    let cmd = src.split("pub(crate) fn open_bg_op_folder").nth(1).unwrap();
    let cmd = cmd.split("open_explorer_at").next().unwrap();
    assert!(cmd.contains("is_recorded_bg_target") && cmd.contains("is_foreign_network_path"), "registry lookup and network gate both run before Explorer");
}

#[test]
fn ffmpeg_download_needs_a_fresh_one_shot_consent() {
    use std::sync::atomic::AtomicU64;
    let slot = AtomicU64::new(0);
    assert!(!take_ffmpeg_consent(&slot, 1_000), "no accepted dialog, no download");
    slot.store(1_000, Ordering::SeqCst);
    assert!(take_ffmpeg_consent(&slot, 1_500), "a fresh consent is accepted");
    assert!(!take_ffmpeg_consent(&slot, 1_600), "and used up by the one download it allowed");
    slot.store(1_000, Ordering::SeqCst);
    assert!(!take_ffmpeg_consent(&slot, 1_000 + 120_001), "a stale consent expires");
    assert!(!take_ffmpeg_consent(&slot, 1_000), "an expired one is gone too");
    // The command checks it after claiming the download slot, before any network work.
    let src = source!("../ffmpeg.rs");
    let cmd = src.split("pub(crate) async fn download_ffmpeg").nth(1).unwrap();
    let cmd = cmd.split("push_bg_op").next().unwrap();
    assert!(cmd.contains("take_ffmpeg_consent(&FFMPEG_CONSENT_AT_MS"), "download_ffmpeg consumes the consent before starting");
    // Only the confirm dialogs' accept paths grant it.
    for (name, js) in [("expertMenu.js", source!("../../../ui/expertMenu.js")), ("video.js", source!("../../../ui/video.js"))] {
        let at = js.find("grant_ffmpeg_download_consent").unwrap_or_else(|| panic!("{name} grants consent"));
        assert!(js[..at].rfind("confirmDialog").is_some_and(|c| at - c < 600), "{name}: consent is granted right after the dialog");
    }
}
