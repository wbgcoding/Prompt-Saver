use super::*;

// ---- Recording conversion: pure guard logic, no ffmpeg/hardware needed ----

#[test]
fn convert_is_noop_rejects_a_plain_mp4_to_mp4_request() {
    assert!(convert_is_noop(true, "mp4", false));
}

#[test]
fn convert_is_noop_allows_the_lossless_mp4_reencode_despite_the_same_extension() {
    // This is the one case where "same extension" must NOT short-circuit
    // the conversion: lossless re-encodes mp4 in place on purpose.
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
//: see the README's "Building from source" section). A clean checkout
// without it present skips these rather than failing.

pub(super) fn test_ffmpeg() -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("ffmpeg.exe");
    p.is_file().then_some(p)
}

// A tiny synthetic clip, generated entirely by ffmpeg's own test-pattern
// source: no real recording needed to exercise the conversion path.
pub(super) fn make_test_mp4(ffmpeg: &Path, dir: &Path, name: &str, seconds: u32, size: &str) -> PathBuf {
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
    // WebM is a Matroska/EBML container: its files start with this fixed
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
    // A cancel flag local to this test: never the process-global
    // REC_CONVERT_CANCEL: so flipping it can never race an unrelated
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
    // Not a real video: ffmpeg exits non-zero, exercising the failure path.
    // ffmpeg's mp4 demuxer reports this exact case ("no valid moov box at
    // all", whether from garbage bytes or a real truncation) as the same
    // "moov atom not found" line as a genuinely cut-short recording, so
    // this lands on ffErrTruncated too: confirmed empirically here
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
// flag against each other: cargo test runs tests on separate threads by
// default, and one test flipping that flag mid-run would otherwise cancel
// whatever unrelated crop another thread happened to be running at the same
// moment.
pub(super) static REC_CROP_TEST_LOCK: Mutex<()> = Mutex::new(());

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
    // lossless_quality on mp4 burns 320k AAC instead of 192k: the estimate
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
    // panic or report zero: it falls back to a reasonable assumed bitrate.
    let est = estimate_export_bytes("webm", None, false, 10_000);
    assert!(est.bytes.unwrap() > 0);
}

// ---- Disk space check (recording start, mid-recording, export) ----

#[test]
fn estimate_recording_seconds_remaining_uses_bitrate_times_time() {
    // 8000 kbps = 1,000,000 bytes/sec with no audio: 10,000,000 bytes
    // should buy exactly 10 seconds, an exact match like the export
    // formula test above, not just "roughly right".
    assert_eq!(estimate_recording_seconds_remaining(10_000_000, 8000, false), 10);
}

#[test]
fn estimate_recording_seconds_remaining_adds_the_audio_surcharge() {
    // Same free space either way: the higher combined bitrate with
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
    // session once it returns from a blocking thread: a second start
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
    // 8000 MB ceiling: shown, but with no ETA to a limit this far off.
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
    // region's offset within it, exactly what resolve_region_clip already crops
    // each frame to.
    assert_eq!(recording::region_outline_rect((1920, 0), 100, 50, 400, 300), (2020, 50, 400, 300));
    assert_eq!(recording::region_outline_rect((0, 0), 0, 0, 1920, 1080), (0, 0, 1920, 1080));
}

#[test]
fn region_outline_rect_clamps_a_negative_offset_to_the_monitor_origin() {
    // Same defensive clamp resolve_region_clip already applies to a region whose
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

#[test]
fn free_space_bytes_accepts_an_existing_file() {
    // The recording loop passes the live output file, not its folder.
    let file = std::env::temp_dir().join(format!("{}-test-free-space-file.tmp", brand::SLUG));
    std::fs::write(&file, b"x").unwrap();
    let result = free_space_bytes(&file);
    let _ = std::fs::remove_file(&file);
    assert!(result.expect("free space for a file path") > 0);
}

// ---- Recording end: the main window comes forward with the result ----

#[test]
fn every_recording_captured_emit_is_preceded_by_the_main_window_reveal() {
    let src = include_str!("../rec_cmds.rs");
    let lines: Vec<&str> = src.lines().collect();
    let mut emits = 0;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.starts_with("//") || !t.contains("emit(\"recording-captured\"") {
            continue;
        }
        emits += 1;
        assert!(
            lines[i - 1].trim_start().starts_with("reveal_main_for_result("),
            "the emit on line {} has no reveal_main_for_result call right before it",
            i + 1
        );
    }
    assert_eq!(emits, 2, "expected the stop path and the failed-continue path");
}

#[test]
fn the_recording_state_event_names_idle_recording_and_paused() {
    assert_eq!(rec_state_name(None), "idle");
    assert_eq!(rec_state_name(Some(RecPhase::Idle)), "idle");
    assert_eq!(rec_state_name(Some(RecPhase::Recording)), "recording");
    assert_eq!(rec_state_name(Some(RecPhase::Paused)), "paused");
}

// The checks below compare multi-line source text; a checkout with Windows line endings
// must read the same as one with Unix endings.
macro_rules! source {
    ($path:literal) => {
        include_str!($path).replace("\r\n", "\n")
    };
}

#[test]
fn cancelling_interim_crops_does_not_deadlock_on_their_own_list_lock() {
    let list: Arc<Mutex<InterimCrops>> = Arc::new(Mutex::new(Vec::new()));
    let flag = Arc::new(AtomicBool::new(false));
    let (flag2, list2) = (flag.clone(), list.clone());
    // Mirrors a crop thread: runs until signalled, then takes the list lock to drop its own entry.
    let handle = std::thread::spawn(move || {
        while !flag2.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let me = std::thread::current().id();
        list2.lock().unwrap().retain(|(_, h)| h.thread().id() != me);
    });
    list.lock().unwrap().push((flag, handle));
    let (tx, rx) = std::sync::mpsc::channel();
    let list3 = list.clone();
    std::thread::spawn(move || {
        stop_and_join_interim_crops(&list3);
        let _ = tx.send(());
    });
    assert!(rx.recv_timeout(std::time::Duration::from_secs(10)).is_ok(), "joining the crops hung");
    assert!(list.lock().unwrap().is_empty());
}

#[test]
fn follow_thread_keeps_the_bar_while_a_stop_is_refused_mid_switch() {
    assert!(follow_stop_settled(true, false, false), "a stop that went through closes the bar");
    assert!(!follow_stop_settled(false, false, true), "a continue in flight keeps it for a retry");
    assert!(!follow_stop_settled(false, true, false), "a session that refused the stop keeps it");
    assert!(follow_stop_settled(false, false, false), "nothing recording any more closes it");
}

#[test]
fn recording_status_stats_the_file_after_the_session_lock_is_released() {
    let src = include_str!("../rec_bar.rs").replace("\r\n", "\n");
    let start = src.find("pub(crate) async fn recording_status(").unwrap();
    let body = &src[start..src[start..].find("\n}\n").map(|i| start + i).unwrap()];
    let lock_at = body.find("rstate.0.lock()").unwrap();
    let scope_end = body.find("\n    };\n    let bytes").expect("the lock lives in its own block");
    let stat_at = body.find("metadata(&output_path)").expect("stats a copied path");
    assert!(lock_at < scope_end && scope_end < stat_at, "metadata must run after the lock block ends");
    assert!(!body.contains("metadata(&session"), "never stat through the locked session");
}

// Structural fix, no red unit test: only comments and an attribute moved.
#[test]
fn the_follow_comment_and_cfg_sit_on_the_follow_function() {
    let bar = source!("../rec_bar.rs");
    assert!(bar.contains("// periodic jobs.
#[cfg(windows)]
pub(crate) fn spawn_rec_bar_follow("));
    assert!(!bar.contains("// periodic jobs.
#[cfg(windows)]
// Bumped"));
}

#[test]
fn waiting_for_interim_crops_returns_only_after_them_and_never_cancels() {
    let flag = Arc::new(AtomicBool::new(false));
    let list: Mutex<InterimCrops> = Mutex::new(Vec::new());
    assert!(wait_for_interim_crops(&list, Duration::from_millis(10)), "an empty list is done at once");
    let handle = std::thread::spawn(|| std::thread::sleep(Duration::from_millis(300)));
    list.lock().unwrap().push((flag.clone(), handle));
    assert!(!wait_for_interim_crops(&list, Duration::from_millis(20)), "a running crop outlasts a short wait");
    assert!(wait_for_interim_crops(&list, Duration::from_secs(5)), "the wait ends once the crop did");
    assert!(!flag.load(Ordering::SeqCst), "waiting never signals a cancel");
    assert_eq!(list.lock().unwrap().len(), 1, "the entries stay for a cancel to find");
}

#[test]
fn placing_or_discarding_a_part_waits_for_its_background_crop() {
    let mf = source!("../media_files.rs");
    let save = &mf[mf.find("pub(crate) async fn save_recording_parts").unwrap()..];
    let save = &save[..save.find("with_parts_outside_lock").unwrap()];
    assert!(save.contains("wait_for_interim_crops"));
    let disc = &mf[mf.find("pub(crate) fn discard_recording_parts(").unwrap()..];
    let disc = &disc[..disc.find("discard_recording_parts_in(").unwrap()];
    assert!(disc.contains("wait_for_interim_crops"));
    let quit = &mf[mf.find("wait_for_part_saves(REC_PARTS_QUIT_WAIT)").unwrap()..];
    let quit = &quit[..quit.find("place_recording_parts(&mut parts").unwrap()];
    assert!(quit.contains("wait_for_interim_crops"));
}
