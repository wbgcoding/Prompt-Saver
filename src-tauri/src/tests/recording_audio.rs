use super::*;

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
    // where the screen recording's own output actually is: only the
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
    // the two oldest (ts 0, 1) were dropped: what remains starts at ts 2.
    let first = shared.pop_or_stop().expect("queue should still have items");
    assert_eq!(first.ts, 2);
}

#[test]
fn rec_feeder_encode_error_latches_the_first_message_and_peek_does_not_consume() {
    let shared = RecFeederShared::new();
    assert_eq!(shared.peek_encode_error(), None);
    shared.set_encode_error("disk full".to_string());
    shared.set_encode_error("a later, different failure".to_string());
    // First failure wins: a failing encoder tends to repeat the same
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
    // sizes and later timestamps: only the FIRST one's timestamp may
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
    // fully drained: a second pull is pure silence.
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

pub(super) fn mp4_box(typ: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = ((8 + body.len()) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(typ);
    out.extend_from_slice(body);
    out
}
pub(super) fn mp4_mdhd_v0(timescale: u32, duration: u32) -> Vec<u8> {
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
// the whole path end to end: WASAPI capture, the feeder's audio tick,
// send_audio_buffer (without needing a Media Foundation reader); an MP4
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
    let (path, _dropped, _encode_error, _mic_wav, _canvas_note, _crop_rect, _canvas_size, _resize_crop_at_ms) = recording::stop(session);
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
    let (path, _dropped, _encode_error, _mic_wav, _canvas_note, _crop_rect, _canvas_size, _resize_crop_at_ms) = recording::stop(session);
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
    let (path, _dropped, _encode_error, _mic_wav, _canvas_note, _crop_rect, _canvas_size, _resize_crop_at_ms) = recording::stop(session);
    let bytes = std::fs::read(&path).expect("read output file");
    assert!(bytes.len() > 10_000, "output suspiciously small: {} bytes", bytes.len());
    let track_count = bytes.windows(4).filter(|w| *w == b"trak").count();
    let _ = std::fs::remove_file(&path);
    assert_eq!(track_count, 2, "expected 1 video + 1 mixed-audio track box, found {track_count}");
}

// The acceptance number this was written against: "drift under
// 50ms over 10 minutes, clocked from WASAPI timestamps... measure it
// over a real 10-minute recording, do not assume it." Runs for a real
// ~10 real minutes: not part of any normal or CI run.
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
    let (path, _dropped, _encode_error, _mic_wav, _canvas_note, _crop_rect, _canvas_size, _resize_crop_at_ms) = recording::stop(session);
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
// The original code, unchanged. Still 2 "trak" boxes though: a
// real-hardware run of this test is what surfaced that windows-capture's
// MediaStreamSource is built with both a video AND an audio descriptor
// regardless of AudioSettingsBuilder::disabled (see encoder.rs's
// VideoEncoder::new): "disabled" only makes send_audio_buffer refuse to
// be called, which our audio-off feeder_loop never does anyway: so the
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
    let (path, _dropped, _encode_error, _mic_wav, _canvas_note, _crop_rect, _canvas_size, _resize_crop_at_ms) = recording::stop(session);
    let bytes = std::fs::read(&path).expect("read output file");
    let track_count = bytes.windows(4).filter(|w| *w == b"trak").count();
    let _ = std::fs::remove_file(&path);
    assert_eq!(track_count, 2, "expected 1 video + 1 empty audio track box, found {track_count}");
}

// Pure naming/numbering check, no capture hardware needed: runs on
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
    // stem, never from an already-numbered one: callers are expected to
    // always pass the first part's own path in, matching how
    // finish_and_continue_recording resolves its own base_path.
    assert_eq!(
        recording::continuation_output_path(base, 3).file_stem().unwrap(),
        "rec-2026-09-11_120000_part3"
    );
}

// Two files come out of a "finish and continue" style sequence, both
// playable, correctly numbered, and started from the exact same settings
// values (the same variables handed to `start` twice: the actual
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
    // The session retains exactly what it was built with: this is what
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
    let (stopped1_path, _d1, _e1, _m1, _c1, _r1, _s1, _rc1) = recording::stop(session1);
    assert_eq!(stopped1_path, part1_path);

    let part2_path = recording::continuation_output_path(&part1_path, 2);
    let session2 = recording::start(
        target, part2_path.clone(), audio, canvas_mode,
        recording::RecStartOptions { show_cursor, video_kbps, video_fps, canvas_ceiling, disk_warn_mb, max_min, max_mb },
    )
    .expect("start part 2 with the SAME settings as part 1");
    std::thread::sleep(std::time::Duration::from_secs(2));
    let (stopped2_path, _d2, _e2, _m2, _c2, _r2, _s2, _rc2) = recording::stop(session2);
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
    // The placement helper must reproduce the existing (0, 0) case
    // byte-for-byte: letterbox_frame_top_down is only ever this now.
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
    // still inside the canvas, landing at (0,0): the "display placed
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
// separators around it: inside a longer name it is just two dots and
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

// The checks below compare multi-line source text; a checkout with Windows line endings
// must read the same as one with Unix endings.
macro_rules! source {
    ($path:literal) => {
        include_str!($path).replace("\r\n", "\n")
    };
}

#[test]
fn visible_canvas_rect_never_leaves_the_canvas() {
    // Fully inside.
    assert_eq!(visible_canvas_rect(100, 50, 800, 600, 1920, 1080), Some((100, 50, 800, 600)));
    // Hanging over the right and bottom edge: only the visible part counts.
    assert_eq!(visible_canvas_rect(1500, 900, 800, 600, 1920, 1080), Some((1500, 900, 420, 180)));
    // Hanging over the left and top edge.
    assert_eq!(visible_canvas_rect(-300, -100, 1000, 500, 1920, 1080), Some((0, 0, 700, 400)));
    // Entirely off the canvas.
    assert_eq!(visible_canvas_rect(2000, 10, 800, 600, 1920, 1080), None);
    assert_eq!(visible_canvas_rect(-900, 10, 800, 600, 1920, 1080), None);
}

#[test]
fn a_paused_tick_does_not_count_toward_the_recording_minute_limit() {
    let mut elapsed = 0u64;
    for paused in [false, false, true, true, true, false] {
        elapsed += active_tick_secs(paused);
    }
    assert_eq!(elapsed, 3);
    assert!(!recording_limit_reached(elapsed, 0, 1.0, 0.0), "a long pause must not reach the limit");
}

#[test]
fn the_disk_check_gap_cannot_fill_the_drive_at_the_highest_bitrate() {
    // 50 Mbps video plus generous audio, against the hard stop at a quarter of the 500 MB default warning.
    let bytes_per_sec = (50_000u64 + 512) * 1000 / 8;
    let critical = 500u64 * 1024 * 1024 / 4;
    assert!(bytes_per_sec * DISK_CHECK_SECS < critical / 4, "a check gap of {DISK_CHECK_SECS}s writes too much");
}

#[test]
fn a_refused_start_removes_the_mic_sidecar_with_the_empty_output() {
    let dir = std::env::temp_dir().join(format!("ps-refused-start-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let out = dir.join("rec-x.mp4");
    fs::write(&out, b"").unwrap();
    fs::write(out.with_extension("mic.wav"), b"RIFF").unwrap();
    recording::discard_start_leftovers(&out);
    assert!(!out.exists());
    assert!(!out.with_extension("mic.wav").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn recording_timestamps_never_go_backwards() {
    let mut last = None;
    assert_eq!(recording::strictly_increasing_ts(&mut last, 100), 100);
    // A hold resend stamped ahead of a frame that arrived meanwhile.
    assert_eq!(recording::strictly_increasing_ts(&mut last, 400), 400);
    assert_eq!(recording::strictly_increasing_ts(&mut last, 330), 401);
    assert_eq!(recording::strictly_increasing_ts(&mut last, 401), 402);
    assert_eq!(recording::strictly_increasing_ts(&mut last, 900), 900);
}

#[test]
fn a_converted_recordings_sidecars_stay_referenced_through_the_sweep() {
    let dir = std::env::temp_dir().join(format!("ps-test-rec-converted-sidecars-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let video = dir.join("2026-09-04_07-00-00.webm");
    let mic = dir.join("2026-09-04_07-00-00.mic.wav");
    let keys = dir.join("2026-09-04_07-00-00.keys.json");
    for f in [&video, &mic, &keys] {
        fs::write(f, b"data").unwrap();
        backdate(f, Duration::from_secs(45 * 60));
    }
    assert_eq!(mic_sidecar_video_path(&mic), Some(video.clone()));
    assert_eq!(keys_sidecar_video_path(&keys), Some(video.clone()));
    let mut referenced = std::collections::HashSet::new();
    referenced.insert(video.to_string_lossy().replace('/', "\\").to_lowercase());
    let (count, _) = sweep_stale_recordings_in(&dir, &referenced, &[], false, Duration::from_secs(600), Some(Duration::from_secs(30 * 60)));
    assert_eq!(count, 0, "the sidecars of a kept converted recording must survive");
    assert!(mic.exists() && keys.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(windows)]
#[test]
fn a_paused_recording_does_not_spend_the_switch_first_frame_budget() {
    assert_eq!(recording::switch_wait_budget_after_tick(2000, true), 2000);
    assert_eq!(recording::switch_wait_budget_after_tick(2000, false), 1990);
    assert_eq!(recording::switch_wait_budget_after_tick(5, false), 0);
}

#[test]
fn clip_region_to_frame_keeps_the_pixels_that_lie_on_the_monitor() {
    // Inside: unchanged.
    assert_eq!(recording::clip_region_to_frame(100, 50, 400, 300, 1920, 1080), Some((100, 50, 400, 300)));
    // Negative origin: the visible part keeps its size minus the overhang.
    assert_eq!(recording::clip_region_to_frame(-100, -20, 400, 300, 1920, 1080), Some((0, 0, 300, 280)));
    // Overhang past the right/bottom edge.
    assert_eq!(recording::clip_region_to_frame(1800, 1000, 400, 300, 1920, 1080), Some((1800, 1000, 120, 80)));
    // No overlap at all.
    assert_eq!(recording::clip_region_to_frame(2000, 0, 100, 100, 1920, 1080), None);
    assert_eq!(recording::clip_region_to_frame(-500, 0, 100, 100, 1920, 1080), None);
    assert_eq!(recording::clip_region_to_frame(0, 0, 0, 100, 1920, 1080), None);
}

#[test]
fn crop_and_flip_rows_bottom_up_copies_the_overlap_of_a_crop_that_overhangs_the_frame() {
    // 2x2 frame, pixels 1..4 (first byte distinct), crop 3 wide starting at x=1.
    let raw: Vec<u8> = vec![1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
    let out = crop_and_flip_rows_bottom_up(&raw, 2, 2, 8, 1, 0, 3, 2);
    // Bottom-up: row 1 (pixel 4) first, then row 0 (pixel 2); the missing columns stay black.
    assert_eq!(out, vec![4, 4, 4, 4, 0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn record_clock_never_steps_backwards_while_another_thread_pauses_and_resumes() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let clock = std::sync::Arc::new(RecordClock::new());
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let toggler = {
        let (clock, stop) = (clock.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                clock.pause();
                clock.resume();
            }
        })
    };
    let mut last = 0i64;
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_millis(400) {
        let now = clock.now_ticks();
        assert!(now >= last, "ticks went back from {last} to {now}");
        last = now;
    }
    stop.store(true, Ordering::SeqCst);
    toggler.join().unwrap();
}

#[test]
fn reencode_refuses_a_png_header_claiming_huge_dimensions() {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(b"IHDR");
    ihdr.extend_from_slice(&60_000u32.to_be_bytes());
    ihdr.extend_from_slice(&60_000u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut crc = flate2::Crc::new();
    crc.update(&ihdr);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&13u32.to_be_bytes());
    png.extend_from_slice(&ihdr);
    png.extend_from_slice(&crc.sum().to_be_bytes());
    let began = std::time::Instant::now();
    assert!(reencode_screenshot_bytes(&png, "png", 90).is_err());
    assert!(began.elapsed() < Duration::from_secs(2), "took {:?}", began.elapsed());
}

pub(super) fn start_guard_spinner(flag: &Arc<AtomicBool>, done: &Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    let (flag, done) = (flag.clone(), done.clone());
    std::thread::spawn(move || {
        while !flag.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(5));
        }
        done.store(true, std::sync::atomic::Ordering::Relaxed);
    })
}

#[test]
fn start_guard_drop_ends_every_thread_it_holds_within_a_second() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let flag = Arc::new(AtomicBool::new(false));
    let (done_a, done_b, done_c) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
    // The third thread blocks on a channel only the wake action closes, the way the
    // feeder waits on its queue until a stop is requested.
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let blocked = {
        let done = done_c.clone();
        std::thread::spawn(move || {
            let _ = rx.recv();
            done.store(true, Ordering::Relaxed);
        })
    };
    let mut guard = recording::StartGuard::new(move || drop(tx));
    guard.add_flag(flag.clone());
    guard.audio_threads.push(start_guard_spinner(&flag, &done_a));
    guard.audio_threads.push(start_guard_spinner(&flag, &done_b));
    guard.feeder = Some(blocked);
    let began = std::time::Instant::now();
    drop(guard);
    assert!(began.elapsed() < Duration::from_secs(1), "dropping the guard waited too long");
    assert!(done_a.load(Ordering::Relaxed) && done_b.load(Ordering::Relaxed), "the flagged threads ended");
    assert!(done_c.load(Ordering::Relaxed), "the thread blocked on the channel was woken and ended");
}

#[test]
fn start_guard_after_take_joins_and_stops_nothing() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let flag = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let woke = Arc::new(AtomicBool::new(false));
    let mut guard = recording::StartGuard::new({
        let woke = woke.clone();
        move || woke.store(true, Ordering::Relaxed)
    });
    guard.add_flag(flag.clone());
    guard.audio_threads.push(start_guard_spinner(&flag, &done));
    let (audio, feeder, mic) = guard.take();
    drop(guard);
    assert!(!flag.load(Ordering::Relaxed), "a taken guard sets no stop flag");
    assert!(!woke.load(Ordering::Relaxed), "a taken guard runs no wake action");
    assert!(!done.load(Ordering::Relaxed), "the thread now belongs to the caller and still runs");
    assert_eq!((audio.len(), feeder.is_some(), mic.is_some()), (1, false, false));
    flag.store(true, Ordering::Relaxed);
    for t in audio {
        t.join().unwrap();
    }
    assert!(done.load(Ordering::Relaxed));
}

#[test]
fn a_new_recording_never_takes_an_existing_name() {
    let root = std::env::temp_dir().join(format!("ps-recname-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let name = rec_filename(2026, 10, 18, 12, 0, 0);
    assert_eq!(free_rec_path(&root, &name), root.join(&name));
    fs::write(root.join(&name), b"x").unwrap();
    assert_eq!(free_rec_path(&root, &name), root.join("rec-2026-10-18_120000-2.mp4"));
    fs::write(root.join("rec-2026-10-18_120000-2.mp4"), b"x").unwrap();
    assert_eq!(free_rec_path(&root, &name), root.join("rec-2026-10-18_120000-3.mp4"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_mic_sidecar_that_cannot_be_created_marks_the_mic_lost() {
    let queue = Arc::new(AudioQueueShared::new());
    let bad = std::env::temp_dir().join(format!("ps-nodir-{}", std::process::id())).join("x").join("rec.mic.wav");
    crate::recording::mic_wav_writer_loop(
        queue.clone(),
        Arc::new(RecordClock::new()),
        Arc::new(RecFeederShared::new()),
        Arc::new(AtomicBool::new(false)),
        bad,
    );
    assert!(queue.is_lost(), "the bar must hear about a mic track that is not being written");
}

// Structural fix, no red unit test: a refused thread spawn cannot be provoked in a test.
#[test]
fn a_refused_audio_thread_is_a_start_error_not_a_panic() {
    let audio = source!("../audio.rs");
    assert!(audio.contains("-> std::io::Result<thread::JoinHandle<()>>"));
    assert!(!audio.contains("expect(\"spawn rec-audio thread\")"));
    let rec = source!("../recording.rs");
    assert_eq!(rec.matches("audio_stop.clone().unwrap(),\n        ).map_err(|e| e.to_string())?").count(), 2);
}

// Structural fix, no red unit test: the encoder needs a real media session to fail on demand.
#[test]
fn the_feeder_stops_feeding_after_the_first_encoder_error() {
    let rec = source!("../recording.rs");
    let drain = rec.split("fn drain_pending_frames(").nth(1).unwrap().split("
}
").next().unwrap();
    assert!(drain.contains("shared.request_stop();") && drain.contains("return;"), "the drain must leave at the first error");
    assert!(rec.contains("!clock.is_paused() && shared.peek_encode_error().is_none()"), "hold and audio sends must stop after an error");
}

// Structural fix, no red unit test: the old shape could not fail at run time, which is the finding.
#[test]
fn stopping_and_cancelling_a_recording_return_plain_values() {
    let rec = source!("../recording.rs");
    assert!(rec.contains("pub fn stop(session: RecSession) -> StopResult {"));
    assert!(rec.contains("pub fn cancel(session: RecSession) {"));
    assert!(rec.contains("fn finish_and_stop(mut session: RecSession) -> (u64, Option<String>) {"));
    let cmds = source!("../rec_cmds.rs");
    assert!(!cmds.contains("recording::cancel(session)?"));
    assert!(!cmds.contains("= stopped?;"));
}

#[test]
fn saving_the_parts_does_not_hold_the_shared_list_lock() {
    let part = |n: &str| RecordingPartInfo { path: n.to_string(), mic_wav_path: None };
    let shared = std::sync::Mutex::new(vec![part("a"), part("b")]);
    let seen = with_parts_outside_lock(&shared, |mine| {
        assert!(shared.try_lock().is_ok(), "the lock is free while the parts are being moved");
        assert_eq!(mine.len(), 2);
        // "a" moved, "b" could not be: it must come back, ahead of a part added meanwhile.
        mine.retain(|p| p.path == "b");
        shared.lock().unwrap().push(part("c"));
        7
    });
    assert_eq!(seen, 7);
    let left: Vec<String> = shared.lock().unwrap().iter().map(|p| p.path.clone()).collect();
    assert_eq!(left, vec!["b".to_string(), "c".to_string()]);
}
