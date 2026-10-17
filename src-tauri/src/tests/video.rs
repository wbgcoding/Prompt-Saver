use super::*;

// ---- Video editor (242-245): real ffmpeg/ffprobe, no capture hardware needed ----

pub(super) fn test_ffprobe() -> Option<PathBuf> {
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
pub(super) fn make_test_mp4_with_audio(ffmpeg: &Path, dir: &Path, name: &str, seconds: u32, size: &str) -> PathBuf {
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

pub(super) fn make_test_wav(dir: &Path, name: &str, seconds: u32) -> PathBuf {
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
pub(super) fn checkerboard_rgba() -> Vec<u8> {
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
    make_test_mp4(&ffmpeg, &dir, "rec-a.mp4", 1, "64x48");
    make_test_mp4(&ffmpeg, &dir, "rec-b.mp4", 2, "64x48");
    let names = vec!["rec-a.mp4".to_string(), "rec-b.mp4".to_string(), "rec-missing.mp4".to_string()];
    let durations = video_folder_file_durations_ms_impl(Some(&ffprobe), &dir, &names, &dir);
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
        media_paths: Vec::new(),
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
        media_paths: Vec::new(),
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
        media_paths: Vec::new(),
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

pub(super) fn ffprobe_format_and_video_codec(ffprobe: &Path, path: &Path) -> (String, String, usize) {
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

pub(super) fn run_format_export(ffmpeg: &Path, src: &Path, dest: &Path, format: &str) -> String {
    let req = ExportRequest {
        src_path: src.to_string_lossy().to_string(),
        dest_path: dest.to_string_lossy().to_string(),
        filter_chain: None,
        audio_map: None,
        media_paths: Vec::new(),
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

pub(super) fn sample_pixel_rgb24(ffmpeg: &Path, path: &Path, x: u32, y: u32) -> [u8; 3] {
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

pub(super) fn run_overlay_export(ffmpeg: &Path, src: &Path, dest: &Path, filter_chain: String) {
    let req = ExportRequest {
        src_path: src.to_string_lossy().to_string(),
        dest_path: dest.to_string_lossy().to_string(),
        filter_chain: Some(filter_chain),
        audio_map: None,
        media_paths: Vec::new(),
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
        media_paths: Vec::new(),
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
        media_paths: Vec::new(),
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
        media_paths: Vec::new(),
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
pub(super) fn ff_path_for_test(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").replace(':', "\\:")
}

pub(super) fn ffprobe_stream_durations_secs(ffprobe: &Path, path: &Path) -> (Option<f64>, Option<f64>) {
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

pub(super) fn sample_pixel_rgb24_at(ffmpeg: &Path, path: &Path, seconds: f64, x: u32, y: u32) -> [u8; 3] {
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
        media_paths: Vec::new(),
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
        media_paths: Vec::new(),
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
        media_paths: Vec::new(),
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
