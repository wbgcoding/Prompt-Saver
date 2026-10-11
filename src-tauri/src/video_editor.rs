use super::*;

// ---------- Video editor (242-245) ----------
//
// probe/mux/filmstrip/waveform/still-frame are read-only or one-shot helper
// commands the editor window calls while a recording is open; export_video is
// the ONE additional entry point into ffmpeg beyond convert_recording above,
// sharing run_ffmpeg_to_staged so there is never a second argument-building /
// progress / cancel implementation.

#[derive(serde::Serialize)]
pub(crate) struct RecordingProbe {
    pub(crate) duration_ms: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) fps: f64,
    pub(crate) has_audio: bool,
    pub(crate) has_mic_track: bool,
    pub(crate) mic_sidecar: Option<String>,
    // The lossless "-c copy" fast path only ever produces a valid mp4 if
    // every mapped stream is already a codec mp4 can carry as-is -- a source
    // dragged in from elsewhere (e.g. a webm/VP9 file) fails ffmpeg outright
    // when the export path blindly assumes "trim-only" means "safe to copy".
    pub(crate) stream_copy_safe: bool,
    // Whether this recording has a key-overlay sidecar (see `keylog`) --
    // the video editor only offers the "keys" overlay's presets once it
    // knows there is something to show.
    pub(crate) has_keys_sidecar: bool,
}

// The text-overlay font list used to hardcode "C:/Windows/Fonts/..." --
// every export failed outright on a system where Windows itself is installed
// on a different drive. `SystemRoot` (or `windir`, same value under a different
// name) is what Windows itself sets this from, so it is the one thing that can
// never disagree with where the machine actually put its Fonts folder.
#[tauri::command]
pub(crate) fn windows_fonts_dir() -> String {
    let root = std::env::var("SystemRoot")
        .or_else(|_| std::env::var("windir"))
        .unwrap_or_else(|_| "C:\\Windows".to_string());
    format!("{}/Fonts", root.replace('\\', "/"))
}

#[tauri::command]
pub(crate) async fn probe_recording(app: AppHandle, path: String) -> Result<RecordingProbe, String> {
    let src = PathBuf::from(&path);
    if !src.is_file() {
        return Err("recording file not found".to_string());
    }
    let ffprobe = ffprobe_search_path(&app).ok_or_else(|| "ffmpeg-missing".to_string())?;
    tauri::async_runtime::spawn_blocking(move || probe_recording_blocking(&ffprobe, &src))
        .await
        .map_err(|e| e.to_string())?
}

// Checks one extra video source (an inserted overlay or section clip, the
// video editor's "Add a video" path) before the user builds anything on top
// of it. Unlike probe_recording above, a failure here comes back as the same
// classified JSON envelope export_video's own errors use, so the editor's
// existing ffErr* messages (see classify_ffmpeg_error) cover this path too
// instead of a second, separate wording for the same three failure shapes.
#[tauri::command]
pub(crate) async fn probe_extra_video_source(app: AppHandle, path: String) -> Result<RecordingProbe, String> {
    let src = PathBuf::from(&path);
    if !src.is_file() {
        return Err(ffmpeg_error_json(FfmpegErrorKey::Missing, None, ""));
    }
    let ffprobe = ffprobe_search_path(&app).ok_or_else(|| "ffmpeg-missing".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        probe_recording_blocking(&ffprobe, &src).map_err(|e| {
            if e == FFMPEG_TIMEOUT_ERR {
                e
            } else {
                ffmpeg_error_json(FfmpegErrorKey::Unreadable, None, &e)
            }
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

pub(crate) fn parse_ffprobe_rate(s: &str) -> Option<f64> {
    let (num, den) = s.split_once('/')?;
    let num: f64 = num.parse().ok()?;
    let den: f64 = den.parse().ok()?;
    if den == 0.0 { None } else { Some(num / den) }
}

pub(crate) fn probe_recording_blocking(ffprobe: &Path, src: &Path) -> Result<RecordingProbe, String> {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new(ffprobe);
    cmd.args(["-v", "quiet", "-print_format", "json", "-show_format", "-show_streams"]).arg(src);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.stdin(std::process::Stdio::null());
    let output = output_in_job(&mut cmd, Duration::from_secs(30)).map_err(|e| tool_run_error("ffprobe", &e))?;
    if !output.status.success() {
        return Err("ffprobe failed to read the recording".to_string());
    }
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("ffprobe output: {e}"))?;
    let duration_ms = json["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .map(|s| (s * 1000.0) as u64)
        .unwrap_or(0);
    let streams = json["streams"].as_array().cloned().unwrap_or_default();
    let video = streams.iter().find(|s| s["codec_type"] == "video");
    let width = video.and_then(|v| v["width"].as_u64()).unwrap_or(0) as u32;
    let height = video.and_then(|v| v["height"].as_u64()).unwrap_or(0) as u32;
    let fps = video
        .and_then(|v| v["avg_frame_rate"].as_str())
        .and_then(parse_ffprobe_rate)
        .filter(|f| *f > 0.0)
        .unwrap_or(30.0);
    let audio_count = streams.iter().filter(|s| s["codec_type"] == "audio").count();
    let sidecar = src.with_extension("mic.wav");
    let mic_sidecar = if audio_count < 2 && sidecar.is_file() {
        Some(sidecar.to_string_lossy().to_string())
    } else {
        None
    };
    let video_codec = video.and_then(|v| v["codec_name"].as_str()).unwrap_or("");
    let non_aac_audio =
        streams.iter().any(|s| s["codec_type"] == "audio" && s["codec_name"].as_str() != Some("aac"));
    let stream_copy_safe = matches!(video_codec, "h264" | "hevc") && !non_aac_audio;
    #[cfg(windows)]
    let has_keys_sidecar = sidecar_path(src, KEYS_SIDECAR_EXT).is_file();
    #[cfg(not(windows))]
    let has_keys_sidecar = false;
    Ok(RecordingProbe {
        duration_ms,
        width,
        height,
        fps,
        has_audio: audio_count >= 1,
        has_mic_track: audio_count >= 2,
        mic_sidecar,
        stream_copy_safe,
        has_keys_sidecar,
    })
}

// The key-overlay editor's own data fetch -- a plain read of the sidecar
// JSON `keylog::write_sidecar` produced, or None when this recording never
// captured one (the setting was off, or it predates this feature). Read via
// a command like every other file access the frontend makes, never a direct
// filesystem call from the webview.
// The one place a key-sidecar JSON file is turned into a KeySidecar --
// shared by the actual read command below and by the copy-safety check that
// runs before a sidecar follows its recording into an editor working copy,
// so both ever accept exactly the same shape of file.
#[cfg(windows)]
pub(crate) fn parse_key_sidecar(data: &str) -> Option<keylog::KeySidecar> {
    // dec_str passes an old unsealed file through unchanged.
    serde_json::from_str(&dec_str(data)).ok()
}

// Reads through the same size cap the copy check uses, so an oversized or
// replaced file is refused instead of being read into memory whole.
#[cfg(windows)]
pub(crate) fn read_key_sidecar_at(sidecar: &Path) -> Option<keylog::KeySidecar> {
    let bytes = read_file_capped(sidecar, KEY_SIDECAR_MAX_COPY_BYTES).ok()?;
    parse_key_sidecar(std::str::from_utf8(&bytes).ok()?)
}

#[cfg(windows)]
#[tauri::command]
pub(crate) async fn read_key_sidecar(path: String) -> Option<keylog::KeySidecar> {
    tauri::async_runtime::spawn_blocking(move || read_key_sidecar_at(&sidecar_path(Path::new(&path), KEYS_SIDECAR_EXT)))
        .await
        .ok()
        .flatten()
}
#[cfg(not(windows))]
#[tauri::command]
pub(crate) async fn read_key_sidecar(path: String) -> Option<()> {
    let _ = path;
    None
}

// Duration/width/height/size for a media attachment (file_path or icon_path),
// measured once when the attachment is set and stored on the Prompt -- never
// recomputed on every render. A still image goes through the
// `image` crate (already a dependency), a video through ffprobe the same way
// probe_recording_blocking does. Nothing here returns an error: a value that
// could not be determined comes back as 0, which the caller and the frontend
// both already treat as "unknown, show nothing" rather than a placeholder.
pub(crate) fn probe_media_meta(ffprobe: Option<&Path>, path: &Path) -> (u64, u32, u32, u64) {
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if let Ok(img) = image::image_dimensions(path) {
        return (0, img.0, img.1, size);
    }
    if let Some(ffprobe) = ffprobe {
        if let Ok(p) = probe_recording_blocking(ffprobe, path) {
            return (p.duration_ms, p.width, p.height, size);
        }
    }
    (0, 0, 0, size)
}

// The one-time catch-up for an entry that has never been measured, or whose
// first measurement failed (the source briefly locked, a network drive asleep): the
// library/history call this the first time such a row is actually shown, never
// during the render itself, so a long list is never held up waiting on ffprobe.
// A no-op (Ok(None), nothing saved) when the entry already carries a
// measurement, has no video/image attachment, or still cannot be read.
#[tauri::command]
pub(crate) async fn backfill_media_meta(app: AppHandle, state: State<'_, Db>, id: String) -> Result<Option<Prompt>, String> {
    let media_path = {
        let store = lock(&state);
        if !store.settings.ui_flags.get("mediaMetaBackfill").copied().unwrap_or(true) {
            return Ok(None);
        }
        match store.prompts.iter().find(|p| p.id == id) {
            Some(p) if p.media_duration_ms == 0 && p.media_width == 0 && p.media_size_bytes == 0 => {
                if !p.file_path.is_empty() {
                    Some(p.file_path.clone())
                } else if !p.icon_path.is_empty() {
                    Some(p.icon_path.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    };
    let Some(media_path) = media_path else { return Ok(None) };
    let ffprobe = ffprobe_search_path(&app);
    let (duration_ms, width, height, size) =
        tauri::async_runtime::spawn_blocking(move || probe_media_meta(ffprobe.as_deref(), Path::new(&media_path)))
            .await
            .map_err(|e| e.to_string())?;
    if duration_ms == 0 && width == 0 && size == 0 {
        return Ok(None); // still unreadable -- nothing changed, nothing to save
    }
    let mut store = lock(&state);
    let updated = match store.prompts.iter_mut().find(|p| p.id == id) {
        Some(p) => {
            p.media_duration_ms = duration_ms;
            p.media_width = width;
            p.media_height = height;
            p.media_size_bytes = size;
            Some(p.clone())
        }
        None => None,
    };
    if updated.is_some() {
        save_prompts(&app, &store);
    }
    Ok(updated)
}

// Consolidates a recording's mic sidecar (see mic_sidecar_video_path / the
// recording engine's mic_wav_writer_loop) into a genuine second AAC track,
// once, the first time the editor opens a recording that still has one.
// Non-fatal to the caller: the video keeps playing system-audio-only if this
// fails, and the sidecar is left in place to retry on the next open.
#[tauri::command]
pub(crate) async fn mux_mic_track(app: AppHandle, state: State<'_, Db>, video_path: String, sidecar_path: String) -> Result<(), String> {
    let video = PathBuf::from(&video_path);
    let sidecar = PathBuf::from(&sidecar_path);
    if !video.is_file() || !sidecar.is_file() {
        return Err("recording or mic sidecar not found".to_string());
    }
    // mux_mic_track_blocking below replaces `video` in place and deletes
    // `sidecar` on success -- both need to be proven to belong to this app
    // before either is touched. The video must be one of our own
    // recordings, and the sidecar must be exactly THAT video's own mic
    // capture, never an unrelated second path that just happens to sit in
    // the same allowed folder.
    if !is_own_recording_source(&recording_source_dirs(&app, &state), &video) {
        return Err("source-not-allowed".to_string());
    }
    if !is_own_mic_sidecar(&video, &sidecar) {
        return Err("source-not-allowed".to_string());
    }
    let ffmpeg = ffmpeg_search_path(&app).ok_or_else(|| "ffmpeg-missing".to_string())?;
    let audio_kbps = crate::rec_cmds::REC_AUDIO_KBPS;
    tauri::async_runtime::spawn_blocking(move || mux_mic_track_blocking(&ffmpeg, &video, &sidecar, audio_kbps))
        .await
        .map_err(|e| e.to_string())?
}

pub(crate) fn mux_mic_track_blocking(ffmpeg: &Path, video: &Path, sidecar: &Path, audio_kbps: u32) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let staged = video.with_file_name(format!(".mux-{}-{}.mp4", std::process::id(), stamp));
    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(video)
        .arg("-i")
        .arg(sidecar)
        .args(["-map", "0:v", "-map", "0:a?", "-map", "1:a", "-c:v", "copy", "-c:a:0", "copy", "-c:a:1", "aac", "-b:a:1"])
        .arg(format!("{audio_kbps}k"))
        // No -shortest -- the mic writer can stop a moment before the
        // video does (its own thread, its own shutdown timing), and -shortest
        // takes the SHORTEST mapped stream as the output length, which
        // permanently truncated the video's tail to match. Without it ffmpeg
        // instead runs until every mapped stream is exhausted, so the mic
        // track just ends a little early inside a video-length file -- the
        // one-sided failure mode that actually matches "the mic dropped out",
        // instead of "the recording lost its ending".
        .arg(&staged);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::piped());
    let output = output_in_job(&mut cmd, Duration::from_secs(600)).map_err(|e| tool_run_error("ffmpeg", &e))?;
    if !output.status.success() || !staged.is_file() {
        let _ = fs::remove_file(&staged);
        let tail: String =
            String::from_utf8_lossy(&output.stderr).chars().rev().take(800).collect::<Vec<_>>().into_iter().rev().collect();
        return Err(format!("mic track mux failed: {}", tail.trim()));
    }
    fs::rename(&staged, video).map_err(|e| {
        let _ = fs::remove_file(&staged);
        format!("could not finalize the muxed file: {e}")
    })?;
    let _ = fs::remove_file(sidecar);
    Ok(())
}

// Thumbnail cache keyed by (path, mtime, requested count) so re-opening the
// same, unchanged recording at the same zoom level never re-extracts. `count`
// is part of the key because each thumbnail's timestamp is spaced across
// the clip AS A FRACTION OF count -- thumb 0 of 40 and thumb 0 of 12 are
// different moments in the recording, so reusing one count's files for
// another silently produced a filmstrip that only covered part of the clip.
// Swept by age in sweep_stale_temp_files, not here.
pub(crate) fn filmstrip_cache_dir(src: &Path, count: u32) -> Option<PathBuf> {
    let mtime = fs::metadata(src).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    src.to_string_lossy().hash(&mut hasher);
    mtime.hash(&mut hasher);
    count.hash(&mut hasher);
    let key = hasher.finish();
    Some(std::env::temp_dir().join(brand::TEMP_DIR).join("video-thumbs").join(format!("{key:x}")))
}

// No duration parameter -- the frontend calls this with just {path, count};
// the editor already ran probe_recording before ever reaching the timeline,
// so ffprobe is known to be present by the time this can be called.
#[tauri::command]
pub(crate) async fn generate_filmstrip(app: AppHandle, path: String, count: u32) -> Result<Vec<String>, String> {
    let src = PathBuf::from(&path);
    if !src.is_file() {
        return Err("recording file not found".to_string());
    }
    let count = count.clamp(1, 300);
    let ffmpeg = ffmpeg_search_path(&app).ok_or_else(|| "ffmpeg-missing".to_string())?;
    let ffprobe = ffprobe_search_path(&app).ok_or_else(|| "ffmpeg-missing".to_string())?;
    tauri::async_runtime::spawn_blocking(move || generate_filmstrip_blocking(&ffmpeg, &ffprobe, &src, count))
        .await
        .map_err(|e| e.to_string())?
}

// One short seek-and-grab ffmpeg call per thumbnail rather than a single
// `-vf select` pass: a recording's variable frame rate (WGC capture never
// guarantees CFR) makes frame-index-based selection unreliable, while a
// wall-clock `-ss` seek before `-i` is exact regardless of frame timing.
pub(crate) fn generate_filmstrip_blocking(ffmpeg: &Path, ffprobe: &Path, src: &Path, count: u32) -> Result<Vec<String>, String> {
    use std::os::windows::process::CommandExt;
    let duration_ms = probe_recording_blocking(ffprobe, src)?.duration_ms;
    let Some(cache_dir) = filmstrip_cache_dir(src, count) else {
        return Err("could not read the recording".to_string());
    };
    let mut out = Vec::with_capacity(count as usize);
    let mut all_cached = true;
    for i in 0..count {
        let p = cache_dir.join(format!("thumb-{i:04}.jpg"));
        if !p.is_file() {
            all_cached = false;
        }
        out.push(p);
    }
    if !all_cached {
        fs::create_dir_all(&cache_dir).map_err(|e| format!("thumbnail cache: {e}"))?;
        let dur_s = (duration_ms as f64 / 1000.0).max(0.1);
        for (i, p) in out.iter().enumerate() {
            let t = dur_s * (i as f64 + 0.5) / count as f64;
            let mut cmd = std::process::Command::new(ffmpeg);
            cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-ss"])
                .arg(format!("{t:.3}"))
                .arg("-i")
                .arg(src)
                .args(["-frames:v", "1", "-vf", "scale=160:-2"])
                .arg(p);
            cmd.creation_flags(CREATE_NO_WINDOW);
            cmd.stdin(std::process::Stdio::null());
            cmd.stdout(std::process::Stdio::null());
            cmd.stderr(std::process::Stdio::null());
            let status = status_in_job(&mut cmd, Duration::from_secs(20)).map_err(|e| tool_run_error("ffmpeg", &e))?;
            if !status.success() || !p.is_file() {
                return Err("thumbnail extraction failed".to_string());
            }
        }
    }
    Ok(out.into_iter().map(|p| p.to_string_lossy().to_string()).collect())
}

pub(crate) const WAVEFORM_SAMPLE_RATE: u32 = 8_000;

#[tauri::command]
pub(crate) async fn generate_waveform_peaks(app: AppHandle, path: String, track: u32, buckets: u32) -> Result<Vec<f32>, String> {
    let src = PathBuf::from(&path);
    if !src.is_file() {
        return Err("recording file not found".to_string());
    }
    let buckets = buckets.clamp(8, 4000);
    let ffmpeg = ffmpeg_search_path(&app).ok_or_else(|| "ffmpeg-missing".to_string())?;
    let ffprobe = ffprobe_search_path(&app);
    tauri::async_runtime::spawn_blocking(move || generate_waveform_peaks_blocking(&ffmpeg, ffprobe.as_deref(), &src, track, buckets))
        .await
        .map_err(|e| e.to_string())?
}

pub(crate) const WAVEFORM_TIMEOUT: Duration = Duration::from_secs(120);

// Peaks built while the decoded audio streams in: one running maximum per bucket, so
// the memory used is the bucket count, not the length of the recording (8 kHz mono
// s16 is 57.6 MB per hour). The bucket edges are the ones `pcm_s16le_to_peaks` uses
// for a buffer of `total_samples`, so both give the same peaks for the same audio.
// Samples past the announced total (a duration that was a little short) fold into the
// last bucket instead of being lost.
pub(crate) struct PeakAccumulator {
    buckets: usize,
    per_bucket: f64,
    peaks: Vec<u16>,
    bucket: usize,
    bucket_end: u64,
    index: u64,
    carry: Option<u8>,
}

impl PeakAccumulator {
    pub(crate) fn new(total_samples: u64, buckets: u32) -> Self {
        let buckets = buckets.max(1) as usize;
        let per_bucket = (total_samples as f64 / buckets as f64).max(1.0);
        let mut acc = Self { buckets, per_bucket, peaks: vec![0; buckets], bucket: 0, bucket_end: 0, index: 0, carry: None };
        acc.bucket_end = acc.end_of(0);
        acc
    }

    fn end_of(&self, bucket: usize) -> u64 {
        if bucket + 1 >= self.buckets {
            u64::MAX
        } else {
            ((bucket + 1) as f64 * self.per_bucket) as u64
        }
    }

    fn add(&mut self, sample: i16) {
        while self.index >= self.bucket_end {
            self.bucket += 1;
            self.bucket_end = self.end_of(self.bucket);
        }
        let slot = &mut self.peaks[self.bucket];
        *slot = (*slot).max(sample.unsigned_abs());
        self.index += 1;
    }

    // Feeds raw little-endian s16 bytes in any chunking; an odd trailing byte waits
    // for the next chunk.
    pub(crate) fn push(&mut self, mut bytes: &[u8]) {
        if let Some(lo) = self.carry {
            let Some((&hi, rest)) = bytes.split_first() else { return };
            self.carry = None;
            self.add(i16::from_le_bytes([lo, hi]));
            bytes = rest;
        }
        let mut pairs = bytes.chunks_exact(2);
        for p in &mut pairs {
            self.add(i16::from_le_bytes([p[0], p[1]]));
        }
        if let [lo] = pairs.remainder() {
            self.carry = Some(*lo);
        }
    }

    pub(crate) fn finish(self) -> Vec<f32> {
        if self.index == 0 {
            return vec![0.0; self.buckets];
        }
        self.peaks.iter().map(|&m| (m as f32 / 32768.0).min(1.0)).collect()
    }
}

pub(crate) fn generate_waveform_peaks_blocking(ffmpeg: &Path, ffprobe: Option<&Path>, src: &Path, track: u32, buckets: u32) -> Result<Vec<f32>, String> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    let map = format!("0:a:{track}");
    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(src)
        .args(["-map", &map, "-f", "s16le", "-ac", "1", "-ar"])
        .arg(WAVEFORM_SAMPLE_RATE.to_string())
        .arg("pipe:1");
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::null());
    // The duration fixes the bucket edges up front. Without one (no ffprobe, or a file
    // that reports none) the audio is collected whole and reduced afterwards.
    let total_samples = ffprobe
        .and_then(|p| probe_recording_blocking(p, src).ok())
        .map(|p| p.duration_ms * WAVEFORM_SAMPLE_RATE as u64 / 1000)
        .unwrap_or(0);
    if total_samples == 0 {
        let output = output_in_job(&mut cmd, WAVEFORM_TIMEOUT).map_err(|e| tool_run_error("ffmpeg", &e))?;
        if !output.status.success() {
            return Err(format!("no audio on track {track}"));
        }
        return Ok(pcm_s16le_to_peaks(&output.stdout, buckets));
    }
    let mut child = spawn_in_job(&mut cmd, app_job()).map_err(|e| tool_run_error("ffmpeg", &e))?;
    let mut stdout = child.stdout.take().ok_or_else(|| "could not read the recording".to_string())?;
    let mut acc = PeakAccumulator::new(total_samples, buckets);
    let reader = std::thread::spawn(move || {
        let mut chunk = vec![0u8; 64 * 1024];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => acc.push(&chunk[..n]),
            }
        }
        acc
    });
    let status = wait_with_deadline(&mut child, WAVEFORM_TIMEOUT).map_err(|e| tool_run_error("ffmpeg", &e))?;
    let acc = reader.join().map_err(|_| format!("no audio on track {track}"))?;
    if !status.success() {
        return Err(format!("no audio on track {track}"));
    }
    Ok(acc.finish())
}

// Running max tracked as u16: i16::MIN.unsigned_abs() is 32768, one past
// i16::MAX, so a signed running max would silently wrap on exactly that
// sample -- normalizing by dividing the u16 max by 32768.0 sidesteps it.
pub(crate) fn pcm_s16le_to_peaks(pcm: &[u8], buckets: u32) -> Vec<f32> {
    let sample_count = pcm.len() / 2;
    if sample_count == 0 || buckets == 0 {
        return vec![0.0; buckets.max(1) as usize];
    }
    let mut peaks = vec![0.0f32; buckets as usize];
    let per_bucket = (sample_count as f64 / buckets as f64).max(1.0);
    for (i, peak) in peaks.iter_mut().enumerate() {
        let start = (i as f64 * per_bucket) as usize;
        let end = (((i + 1) as f64 * per_bucket) as usize).min(sample_count);
        let mut m: u16 = 0;
        for s in start..end {
            let v = i16::from_le_bytes([pcm[s * 2], pcm[s * 2 + 1]]);
            m = m.max(v.unsigned_abs());
        }
        *peak = (m as f32 / 32768.0).min(1.0);
    }
    peaks
}

#[tauri::command]
pub(crate) async fn extract_still_frame(app: AppHandle, state: State<'_, Db>, path: String, at_ms: u64) -> Result<Option<String>, String> {
    let src = PathBuf::from(&path);
    if !src.is_file() {
        return Err("recording file not found".to_string());
    }
    let ffmpeg = ffmpeg_search_path(&app).ok_or_else(|| "ffmpeg-missing".to_string())?;
    let (custom, format) = {
        let g = lock(&state);
        (
            g.settings.ui_texts.get("screenshotDir").cloned().unwrap_or_default(),
            g.settings.ui_texts.get("screenshotFormat").cloned().unwrap_or_else(|| "png".to_string()),
        )
    };
    let format_for_ffmpeg = format.clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || extract_still_frame_blocking(&ffmpeg, &src, at_ms, &format_for_ffmpeg))
        .await
        .map_err(|e| e.to_string())??;
    let saved = save_screenshot(&bytes, &screenshot_dir(&custom), &format!("Screenshot-{}", local_stamp()), screenshot_format_ext(&format));
    Ok(if saved.is_empty() { None } else { Some(saved) })
}

// Same screenshotFormat setting the snip tool uses -- "Einzelbild exportieren"
// is a screenshot in every sense but where its pixels come from. ffmpeg
// encodes the still directly (no round trip through the image crate): the
// format-appropriate extension picks its encoder, -q:v is JPEG's own quality
// knob (meaningless, and unset, for every lossless target), and -lossless 1
// keeps WebP consistent with encode_screenshot's own image-crate WebP, which
// has no lossy mode to diverge from in the first place.
pub(crate) fn extract_still_frame_blocking(ffmpeg: &Path, src: &Path, at_ms: u64, format: &str) -> Result<Vec<u8>, String> {
    use std::os::windows::process::CommandExt;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let tmp_dir = std::env::temp_dir().join(brand::TEMP_DIR);
    fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let tmp = tmp_dir.join(format!("{}-still-{}-{}.{}", brand::TEMP_PREFIX, std::process::id(), stamp, screenshot_format_ext(format)));
    let secs = at_ms as f64 / 1000.0;
    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-ss"])
        .arg(format!("{secs:.3}"))
        .arg("-i")
        .arg(src)
        .args(["-frames:v", "1"]);
    match format {
        "jpeg" => { cmd.args(["-q:v", "2"]); }
        "webp" => { cmd.args(["-lossless", "1"]); }
        _ => {}
    }
    cmd.arg(&tmp);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    let status = match status_in_job(&mut cmd, Duration::from_secs(60)) {
        Ok(s) => s,
        Err(e) => {
            let _ = take_still_output(&tmp, false);
            return Err(tool_run_error("ffmpeg", &e));
        }
    };
    take_still_output(&tmp, status.success())
}

// Reads the extracted frame and removes it; on a failed run the file is removed
// too, since ffmpeg can leave a partial one behind.
pub(crate) fn take_still_output(tmp: &Path, succeeded: bool) -> Result<Vec<u8>, String> {
    let bytes = if succeeded && tmp.is_file() {
        fs::read(tmp).map_err(|e| e.to_string())
    } else {
        Err("still-frame extraction failed".to_string())
    };
    let _ = fs::remove_file(tmp);
    bytes
}

// Export request built entirely by the frontend from its own operation list:
// filter_chain/audio_map are raw -filter_complex fragments the
// frontend compiles, ending in the fixed output labels below when present --
// this backend never inspects or builds a filter graph itself, only wires the
// two fragments together and maps their outputs.
//   filter_chain: ends with [vout] if present, else the source video stream
//                 is mapped unfiltered.
//   audio_map:    ends with [aout] if present, else the source audio (if any)
//                 is mapped unfiltered. Never sent (and never mapped) for gif
//                 -- the GIF container carries no audio at all.
// trim_start_ms/trim_end_ms apply BEFORE any filter graph: trim_start_ms
// becomes an input-side "-ss" (exact and, for the lossless_copy fast path,
// keyframe-snapped by ffmpeg itself), trim_end_ms becomes an output-side
// "-t <duration>" -- so a plain trim/cut with no other operation and
// lossless_copy=true never touches a filter graph or re-encodes at all.
#[derive(serde::Deserialize)]
pub(crate) struct ExportRequest {
    pub(crate) src_path: String,
    pub(crate) dest_path: String,
    pub(crate) filter_chain: Option<String>,
    pub(crate) audio_map: Option<String>,
    pub(crate) lossless_copy: bool,
    // Distinct from lossless_copy (the stream-copy fast path) -- this is
    // "Lossless" picked as a re-encode QUALITY tier, which has no bitrate of
    // its own to map to and used to silently fall back to a fixed, lossy
    // 8000kbps. True whenever the quality dropdown is on "lossless", even
    // when some other op (crop, overlay, ...) still forces a real re-encode.
    pub(crate) lossless_quality: bool,
    pub(crate) format: String,
    pub(crate) video_kbps: Option<u32>,
    pub(crate) trim_start_ms: Option<u64>,
    pub(crate) trim_end_ms: Option<u64>,
    // The finished export's own expected length, in the caller's own words --
    // never read by export_video_blocking itself, only by export_video's own
    // progress wrapper, to turn a raw "ms encoded so far" reading into the
    // percent the header's running-jobs list actually shows.
    pub(crate) total_ms: u64,
}

// Each export owns the generation number it was handed at start. Cancelling
// used to store a single "cancel whichever generation is current" value
// (a plain AtomicU64) -- correct as long as at most one export ever ran at
// once. A batch export can run several generations at the same time
// (the batch window can be wider than one), so a single slot is no longer enough: two jobs
// running together would fight over it, and cancelling one would either miss
// or wrongly cancel the other. A set of cancelled generations fixes this --
// each generation is cancelled (or not) independently, and a finished job
// removes its own entry so the set never grows unbounded.
pub(crate) static VIDEO_EXPORT_GEN: AtomicU64 = AtomicU64::new(0);
// The generation of the export the video editor itself has in flight (0 =
// none). The editor's Cancel must hit exactly this one, not whichever
// generation a batch export reserved last.
pub(crate) static EDITOR_EXPORT_GEN: AtomicU64 = AtomicU64::new(0);

pub(crate) fn video_export_cancelled() -> &'static Mutex<std::collections::HashSet<u64>> {
    use std::sync::OnceLock;
    static SET: OnceLock<Mutex<std::collections::HashSet<u64>>> = OnceLock::new();
    SET.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

// The one folder an export command is allowed to write into this session,
// remembered from the last real folder dialog the backend itself ran
// (`pick_export_folder`'s picked directory) -- an export command never trusts
// a `dest_path` string from the IPC call on its own, since that call could in
// principle be made with any argument at all rather than the one a dialog
// actually returned.
pub(crate) static LAST_EXPORT_DEST_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

pub(crate) fn remember_export_dest_dir(dir: &Path) {
    if let Ok(canon) = fs::canonicalize(dir) {
        *LAST_EXPORT_DEST_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(canon);
    }
}

// A bounded set of files this session's own export_video calls have actually
// written -- the backend's own proof that a given path is one of its own
// outputs, so `save_recording_as` can also accept moving one of THESE without
// widening the "rec-" recording-folder rule that command otherwise enforces
// (an export can land anywhere the export-destination gate authorized, not
// only inside a recording folder, so that rule alone would refuse "Save as"
// after every export whose target was not itself a recording folder).
// Bounded so a very long session cannot grow this without limit; the oldest
// entry is dropped once the cap is hit, same shape as PICKER_GRANTS above.
pub(crate) const EXPORT_OUTPUT_MAX: usize = 64;
pub(crate) static EXPORT_OUTPUTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

pub(crate) fn remember_export_output(path: &Path) {
    let Ok(canon) = fs::canonicalize(path) else { return };
    let mut outputs = EXPORT_OUTPUTS.lock().unwrap_or_else(|e| e.into_inner());
    if outputs.len() >= EXPORT_OUTPUT_MAX {
        outputs.remove(0);
    }
    outputs.push(canon);
}

pub(crate) fn is_known_export_output(path: &Path) -> bool {
    let Ok(canon) = fs::canonicalize(path) else { return false };
    EXPORT_OUTPUTS.lock().unwrap_or_else(|e| e.into_inner()).contains(&canon)
}

// Moves a file and, when it was one of this session's own export outputs,
// carries that proof over to its new place (and drops the old entry), so the
// same export can be moved again from where it landed.
pub(crate) fn move_tracked_export_output(src: &Path, dst: &Path) -> std::io::Result<()> {
    let old = if is_known_export_output(src) { fs::canonicalize(src).ok() } else { None };
    move_file(src, dst)?;
    if let Some(old) = old {
        EXPORT_OUTPUTS.lock().unwrap_or_else(|e| e.into_inner()).retain(|p| p != &old);
        remember_export_output(dst);
    }
    Ok(())
}

// What export_video should add to EXPORT_OUTPUTS once ffmpeg finishes: the
// path export_video_blocking's own Ok result names, never the caller's
// originally requested dest_path. When that name was already taken,
// export_video_blocking's own unique_dest_path rename lands the real file at
// a sibling instead -- recording the requested path there would remember a
// file this export never wrote, while the file that actually exists stays
// unrecognized by save_recording_as.
pub(crate) fn export_output_to_remember(result: &Result<String, String>) -> Option<PathBuf> {
    result.as_ref().ok().map(PathBuf::from)
}

// Whether `dest`'s containing folder is one this session is actually allowed
// to export into: the folder from the last save dialog the backend ran for
// this purpose, the standing `videoExportDir` preference (itself only ever
// set from that same dialog, in an earlier session), or the source file's own
// folder -- but ONLY when that source file already lives inside a folder this
// app controls (its recording folder or the configured export folder).
// Without that restriction an IPC-supplied src_path naming any existing file
// at all would authorize that file's own parent as an export destination,
// handing out write authority over whatever folder happens to contain it;
// with it, the fallback only ever covers the ordinary "convert/export a
// recording back next to itself" case. Everything is compared canonicalized
// so a relative segment or a different letter case cannot be used to step
// outside the folder that was actually picked.
// A name typed into the export panel's "File name" field must stay one
// path segment -- none of Windows' own reserved filename characters, and no
// ".." (which needs no separator of its own to still mean "walk up a level"
// on top of a picked/typed folder). Empty or "."/".." alone are refused the
// same way. The frontend already blocks all of this live as the field is
// typed; this is the same rule re-applied to whatever dest_path arrives.
// Windows reserved device names, matched case-insensitively against the part
// of the stem before its first dot -- "CON.take2" is just as reserved as
// "CON" itself. Kept in sync by hand with ui/exportName.js's own copy.
pub(crate) const EXPORT_RESERVED_STEMS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4",
    "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub(crate) fn export_file_name_is_valid(name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return false;
    }
    // A bare ".." inside a longer name (e.g. "v1..2") cannot climb anywhere --
    // it only means "walk up a level" without separators of its own around
    // it, which the exact "." / ".." checks above already cover on their own.
    if trimmed.chars().any(|c| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || (c as u32) < 0x20) {
        return false;
    }
    // Trailing whitespace is already gone after the trim above; a trailing
    // dot survives it and Windows silently drops it from the real file name,
    // so it gets its own check here.
    if trimmed.ends_with('.') {
        return false;
    }
    let first_segment = trimmed.split('.').next().unwrap_or("").to_ascii_uppercase();
    !EXPORT_RESERVED_STEMS.contains(&first_segment.as_str())
}

// Exporting a file over itself would ask ffmpeg to read and overwrite the
// same file mid-run; refused outright rather than relying on unique_dest_path
// to save it by silently picking a different name. dest need not exist yet
// (that is the normal case for a fresh export), so only its parent folder is
// canonicalized; the file name itself is compared case-insensitively, the way
// Windows treats two names that differ only in case as the same file.
pub(crate) fn export_dest_is_same_as_src(dest: &Path, src: &Path) -> bool {
    let Ok(src_canon) = fs::canonicalize(src) else { return false };
    let dest_parent = dest.parent().unwrap_or_else(|| Path::new("."));
    let Ok(dest_parent_canon) = fs::canonicalize(dest_parent) else { return false };
    let Some(dest_name) = dest.file_name() else { return false };
    let combined = dest_parent_canon.join(dest_name);
    combined.to_string_lossy().eq_ignore_ascii_case(&src_canon.to_string_lossy())
}

// Split from `export_dest_dir_is_authorized` below so the actual decision is
// a plain function of paths and can be unit-tested without a running app.
pub(crate) fn export_dest_dir_is_authorized_against(
    dest: &Path,
    src: &Path,
    last_picked: Option<&Path>,
    configured_dir: Option<&str>,
    trusted_src_dirs: &[PathBuf],
    own_temp_dir: Option<&Path>,
) -> bool {
    let Some(dest_dir) = dest.parent() else { return false };
    let Ok(canon_dest_dir) = fs::canonicalize(dest_dir) else { return false };
    // The app's own temp folder, where clip_export_path puts the throwaway
    // file a "copy to clipboard" export writes before it goes on the clipboard.
    if own_temp_dir.is_some_and(|t| fs::canonicalize(t).is_ok_and(|c| c == canon_dest_dir)) {
        return true;
    }
    if let Some(picked) = last_picked {
        if fs::canonicalize(picked).is_ok_and(|c| c == canon_dest_dir) {
            return true;
        }
    }
    let canon_export_dir = configured_dir.filter(|c| !c.is_empty()).and_then(|c| fs::canonicalize(c).ok());
    if let Some(cfg) = &canon_export_dir {
        if *cfg == canon_dest_dir {
            return true;
        }
    }
    if let Some(src_dir) = src.parent() {
        if let Ok(canon_src_dir) = fs::canonicalize(src_dir) {
            if canon_src_dir == canon_dest_dir {
                let src_is_trusted = trusted_src_dirs.iter().any(|d| fs::canonicalize(d).is_ok_and(|c| c == canon_src_dir))
                    || canon_export_dir.is_some_and(|c| c == canon_src_dir);
                if src_is_trusted {
                    return true;
                }
            }
        }
    }
    false
}

pub(crate) fn export_dest_dir_is_authorized(app: &AppHandle, state: &State<'_, Db>, dest: &Path, src: &Path) -> bool {
    let last_picked = LAST_EXPORT_DEST_DIR.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let configured = lock(state).settings.ui_texts.get("videoExportDir").cloned();
    // A fresh capture still sits in <data>/recordings when the editor opens it,
    // so that folder counts as a recording folder here just like recordingDir.
    let trusted_src_dirs = recording_source_dirs(app, state);
    let own_temp_dir = std::env::temp_dir().join(brand::TEMP_DIR);
    export_dest_dir_is_authorized_against(
        dest,
        src,
        last_picked.as_deref(),
        configured.as_deref(),
        &trusted_src_dirs,
        Some(&own_temp_dir),
    )
}

// The single-editor "Cancel" button: cancels the export the editor itself
// started (it only ever has one in flight), never a batch job that reserved a
// later generation in the meantime. Nothing in flight means nothing to cancel,
// so no stale entry is left behind either.
pub(crate) fn cancel_editor_export_in(editor_gen: &AtomicU64) {
    let gen = editor_gen.load(Ordering::SeqCst);
    if gen != 0 {
        video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).insert(gen);
    }
}

#[tauri::command]
pub(crate) fn cancel_export() {
    cancel_editor_export_in(&EDITOR_EXPORT_GEN);
}

// The batch-export list's per-row cancel button: targets one specific
// job by the generation number embedded in its bg-ops id ("export-<gen>"),
// leaving every other job in the batch running.
#[tauri::command]
pub(crate) fn cancel_export_job(generation: u64) {
    video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).insert(generation);
}

// Global cancel (app quit, cancel_all_bg_ops below) must stop every export
// currently in flight, not just the most recent one -- unlike cancel_export()
// above, whose "whichever is current" is correct ONLY because the editor
// never has more than one export of its own running. A batch can have
// several generations running at once (a wider batch window); scanning the
// bg-ops registry for every still-running "export" row is how every one of
// them gets cancelled instead of just the last one, which is what the app-quit
// path actually needs. The current generation is also inserted directly, in
// case a brand new export's own bg-ops row has not been written yet.
pub(crate) fn cancel_all_exports() {
    let running_gens: Vec<u64> = bg_ops()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|(_, op)| op.kind == "export" && matches!(op.phase, OpPhase::Running(_) | OpPhase::Indeterminate))
        .filter_map(|(id, _)| id.strip_prefix("export-").and_then(|s| s.parse::<u64>().ok()))
        .collect();
    let mut cancelled = video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner());
    cancelled.insert(VIDEO_EXPORT_GEN.load(Ordering::SeqCst));
    for gen in running_gens {
        cancelled.insert(gen);
    }
}

// Progress reuses "recording-convert-progress" / ConvertProgressPayload --
// convert_recording's own event -- rather than a second progress channel; the
// editor window listens to the same event name while an export is in flight.
// The taskbar/running-jobs registry is updated from RIGHT HERE, inside the
// progress closure and around the result, rather than by the editor window's
// own JS calling set_taskbar_progress -- so a job keeps reporting correctly
// even once the window that started it has closed.
#[tauri::command]
pub(crate) async fn export_video(app: AppHandle, state: State<'_, Db>, req: ExportRequest) -> Result<String, String> {
    if !is_video_format(&req.format) {
        return Err("unsupported format".to_string());
    }
    let src = PathBuf::from(&req.src_path);
    if !src.is_file() {
        return Err(ffmpeg_error_json(FfmpegErrorKey::Missing, None, ""));
    }
    let dst = PathBuf::from(&req.dest_path);
    // The export panel's own "File name" field types straight into
    // req.dest_path's own file name -- checked again here rather than
    // trusted from the frontend's own matching validation, same reasoning as
    // export_dest_dir_is_authorized just below: the path ultimately
    // comes from user input.
    let file_name_ok = dst.file_name().and_then(|n| n.to_str()).is_some_and(export_file_name_is_valid);
    if !file_name_ok {
        return Err("export destination not recognized".to_string());
    }
    if export_dest_is_same_as_src(&dst, &src) {
        return Err("export destination not recognized".to_string());
    }
    if !export_dest_dir_is_authorized(&app, &state, &dst, &src) {
        return Err("export destination not recognized".to_string());
    }
    let ffmpeg = ffmpeg_search_path(&app).ok_or_else(|| "ffmpeg-missing".to_string())?;
    let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    EDITOR_EXPORT_GEN.store(my_gen, Ordering::SeqCst);
    // Own registry key per job (not just "export"), so a batch of exports
    // running together each show as their own row instead of overwriting one
    // shared entry -- "export" itself stays the kind the frontend groups
    // and labels by.
    let id = format!("export-{my_gen}");
    let target = req.dest_path.clone();
    let total_ms = req.total_ms.max(1);
    let progress_app = app.clone();
    let progress_target = target.clone();
    let progress_id = id.clone();
    push_bg_op(&app, &id, "export", "running", Some(0), Some(target.clone()));
    let result = tauri::async_runtime::spawn_blocking(move || {
        export_video_blocking(&ffmpeg, &src, &dst, &req, my_gen, move |ms| {
            let _ = progress_app.emit("recording-convert-progress", ConvertProgressPayload { ms });
            let pct = ((ms as u64 * 100) / total_ms).min(100) as u32;
            push_bg_op(&progress_app, &progress_id, "export", "running", Some(pct), Some(progress_target.clone()));
        })
    })
    .await;
    let _ = EDITOR_EXPORT_GEN.compare_exchange(my_gen, 0, Ordering::SeqCst, Ordering::SeqCst);
    let result = result.map_err(|e| e.to_string())?;
    match &result {
        Ok(_) => {
            push_bg_op(&app, &id, "export", "done", None, None);
            if let Some(actual) = export_output_to_remember(&result) {
                remember_export_output(&actual);
            }
        }
        Err(e) if e == "cancelled" => push_bg_op(&app, &id, "export", "done", None, None),
        Err(_) => push_bg_op(&app, &id, "export", "error", None, None),
    }
    video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).remove(&my_gen);
    result
}

// ---------- Batch export: several whole recordings, one dialog ----------

#[derive(serde::Deserialize)]
pub(crate) struct BatchExportItem {
    pub(crate) src_path: String,
    pub(crate) dest_path: String,
    pub(crate) total_ms: u64,
}

#[derive(serde::Serialize)]
pub(crate) struct BatchExportResult {
    pub(crate) ok: usize,
    pub(crate) failed: usize,
}

// One batch item's whole outcome: missing source is rejected the same way
// export_video itself rejects it, otherwise it runs through the exact same
// export_video_blocking every single export uses. AppHandle-free like that
// function, on purpose -- a batch of several recordings needs a real, fast,
// deterministic test proving three in gives three files out and one
// deliberately broken source does not stop the other two, with no Tauri app
// needed, and this is the one place both that test and the real async
// command call into, so neither can drift from the other.
// `my_gen` is reserved by the caller (VIDEO_EXPORT_GEN.fetch_add), not by this
// function, so a batch caller can build its bg-ops id and register the
// "running" row before the job even starts -- same reasoning as
// export_video_blocking's own `my_gen` parameter.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_batch_job_blocking(
    ffmpeg: &Path, src_path: &str, dest_path: &str, total_ms: u64, format: &str, lossless_quality: bool, video_kbps: Option<u32>, my_gen: u64,
    on_progress: impl FnMut(u32),
) -> Result<String, String> {
    let src = PathBuf::from(src_path);
    if !src.is_file() {
        return Err(ffmpeg_error_json(FfmpegErrorKey::Missing, None, ""));
    }
    let dst = PathBuf::from(dest_path);
    let req = ExportRequest {
        src_path: src_path.to_string(),
        dest_path: dest_path.to_string(),
        filter_chain: None,
        audio_map: None,
        lossless_copy: false,
        lossless_quality,
        format: format.to_string(),
        video_kbps,
        trim_start_ms: None,
        trim_end_ms: None,
        total_ms: total_ms.max(1),
    };
    let result = export_video_blocking(ffmpeg, &src, &dst, &req, my_gen, on_progress);
    video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).remove(&my_gen);
    result
}

pub(crate) type ExportJoinHandle = tauri::async_runtime::JoinHandle<Result<String, String>>;

// Awaits one in-flight export and records its outcome in the bg-ops registry
// -- pulled out of export_recordings_batch below so the concurrency-window
// loop and the final drain (after the last item is spawned) share the exact
// same bookkeeping instead of two near-identical copies.
pub(crate) async fn await_batch_export(handle: ExportJoinHandle, id: String, generation: u64, app: AppHandle) -> Result<bool, String> {
    let res = handle.await.map_err(|e| e.to_string())?;
    let ok = match &res {
        Ok(_) => {
            push_bg_op(&app, &id, "export", "done", None, None);
            true
        }
        Err(_) => {
            push_bg_op(&app, &id, "export", "error", None, None);
            false
        }
    };
    video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).remove(&generation);
    Ok(ok)
}

// How many recordings a batch export encodes at once.
pub(crate) const EXPORT_PARALLEL: usize = 1;

// One format/quality/target choice (from the one export dialog the batch
// opens up front instead of one per file) applied to every selected
// recording. Runs them with a concurrency window (EXPORT_PARALLEL, one at a
// time) rather than all at once -- ffmpeg is single-threaded-decode-bound
// enough that four truly parallel encodes already make a normal machine
// unpleasant to use. A source file that fails to open, or an encode that
// fails partway, only counts against that one file -- every other job in the
// batch still runs; one bad recording must never take the rest of the batch
// down with it.
#[tauri::command]
pub(crate) async fn export_recordings_batch(
    app: AppHandle,
    state: State<'_, Db>,
    items: Vec<BatchExportItem>,
    format: String,
    lossless_quality: bool,
    video_kbps: Option<u32>,
) -> Result<BatchExportResult, String> {
    if !is_video_format(&format) {
        return Err("unsupported format".to_string());
    }
    let ffmpeg = ffmpeg_search_path(&app).ok_or_else(|| "ffmpeg-missing".to_string())?;
    let parallel = EXPORT_PARALLEL;

    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut in_flight: std::collections::VecDeque<(ExportJoinHandle, String, u64, AppHandle)> = std::collections::VecDeque::new();

    for item in items {
        if in_flight.len() >= parallel {
            let (handle, id, generation, job_app) = in_flight.pop_front().unwrap();
            if await_batch_export(handle, id, generation, job_app).await? { ok += 1 } else { failed += 1 }
        }
        let item_src = PathBuf::from(&item.src_path);
        if !item_src.is_file() {
            failed += 1;
            continue;
        }
        if !export_dest_dir_is_authorized(&app, &state, &PathBuf::from(&item.dest_path), &item_src) {
            failed += 1;
            continue;
        }
        // Reserved up front (not inside run_batch_job_blocking) so this
        // command's own bg-ops "running" row appears the instant the job is
        // queued, before its spawn_blocking thread is even scheduled.
        let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
        let id = format!("export-{my_gen}");
        let target = item.dest_path.clone();
        let total_ms = item.total_ms.max(1);
        let ffmpeg_job = ffmpeg.clone();
        let src_path = item.src_path.clone();
        let dest_path = item.dest_path.clone();
        let format_job = format.clone();
        let progress_app = app.clone();
        let progress_target = target.clone();
        let progress_id = id.clone();
        push_bg_op(&app, &id, "export", "running", Some(0), Some(target));
        let handle = tauri::async_runtime::spawn_blocking(move || {
            run_batch_job_blocking(&ffmpeg_job, &src_path, &dest_path, total_ms, &format_job, lossless_quality, video_kbps, my_gen, move |ms| {
                let pct = ((ms as u64 * 100) / total_ms).min(100) as u32;
                push_bg_op(&progress_app, &progress_id, "export", "running", Some(pct), Some(progress_target.clone()));
            })
        });
        in_flight.push_back((handle, id, my_gen, app.clone()));
    }
    while let Some((handle, id, generation, job_app)) = in_flight.pop_front() {
        if await_batch_export(handle, id, generation, job_app).await? { ok += 1 } else { failed += 1 }
    }
    Ok(BatchExportResult { ok, failed })
}

// ---------- Export size estimate: "roughly this big" in the format's own hint ----------

#[derive(serde::Serialize)]
pub(crate) struct ExportSizeEstimate {
    // A plain bitrate-based format gets a single number; the three
    // image-based formats below get a low/high bracket instead, since their
    // real size depends far more on motion and color count than on a fixed
    // bitrate. Exactly one of the two shapes is ever filled in.
    pub(crate) bytes: Option<u64>,
    pub(crate) low_bytes: Option<u64>,
    pub(crate) high_bytes: Option<u64>,
}

// Mirrors export_video_blocking's own per-format "-b:a" settings above
// exactly, so the estimate can never quietly drift from what the encoder
// actually burns into the file. gif is always silent (-an),
// hence 0 -- they never reach this function anyway (see the match below).
pub(crate) fn export_audio_kbps(format: &str, lossless_quality: bool) -> u32 {
    match format {
        "mp4" if lossless_quality => 320,
        "mp4" | "avi" => 192,
        "webm" => 128, // libopus with no explicit -b:a: ffmpeg's own default range
        _ => 0,
    }
}

// Pure and window-free so the formula itself has a real unit test, not just
// its wiring. A plain stream-copy ("lossless" trim-only) export has no
// bitrate of its own to estimate from at all -- the frontend never calls
// this for that case (see isLosslessCopy() in video.js), the same way this
// function never claims to know what a copy would weigh.
pub(crate) fn estimate_export_bytes(format: &str, video_kbps: Option<u32>, lossless_quality: bool, duration_ms: u64) -> ExportSizeEstimate {
    let secs = duration_ms as f64 / 1000.0;
    match format {
        "gif" => {
            // A static, few-color clip can run under 5 KB/s; fast full-frame
            // motion easily exceeds 300 kbps once palette dithering kicks in.
            let low = (secs * 30_000.0 / 8.0).round() as u64;
            let high = (secs * 300_000.0 / 8.0).round() as u64;
            ExportSizeEstimate { bytes: None, low_bytes: Some(low), high_bytes: Some(high) }
        }
        _ => {
            let kbps = video_kbps.unwrap_or(8000) as f64 + export_audio_kbps(format, lossless_quality) as f64;
            let bytes = (kbps * 1000.0 / 8.0 * secs).round() as u64;
            ExportSizeEstimate { bytes: Some(bytes), low_bytes: None, high_bytes: None }
        }
    }
}

#[tauri::command]
pub(crate) fn estimate_export_size(format: String, video_kbps: Option<u32>, lossless_quality: bool, duration_ms: u64) -> ExportSizeEstimate {
    estimate_export_bytes(&format, video_kbps, lossless_quality, duration_ms)
}

// The "-t" length for a trim: from the start (0 when only an end is given) to the
// end; None when there is no end or nothing is left.
pub(crate) fn trim_duration_secs(start_ms: Option<u64>, end_ms: Option<u64>) -> Option<f64> {
    let end = end_ms?;
    let dur = end.saturating_sub(start_ms.unwrap_or(0)) as f64 / 1000.0;
    (dur > 0.0).then_some(dur)
}

pub(crate) fn export_video_blocking(
    ffmpeg: &Path, src: &Path, dst: &Path, req: &ExportRequest, my_gen: u64, mut on_progress: impl FnMut(u32),
) -> Result<String, String> {
    let mut input_args: Vec<String> = Vec::new();
    if let Some(start) = req.trim_start_ms {
        if start > 0 {
            input_args.push("-ss".to_string());
            input_args.push(format!("{:.3}", start as f64 / 1000.0));
        }
    }
    let mut output_args: Vec<String> = Vec::new();
    if let Some(dur) = trim_duration_secs(req.trim_start_ms, req.trim_end_ms) {
        output_args.push("-t".to_string());
        output_args.push(format!("{dur:.3}"));
    }
    let plain_copy = req.lossless_copy && req.filter_chain.is_none() && req.audio_map.is_none();
    if plain_copy {
        // Without an explicit -map, ffmpeg's own default stream selection
        // keeps only its single "best" audio stream -- a muxed-in mic track
        // (the file's second audio stream) silently vanished on every
        // lossless trim-only export even though nothing asked to remove it.
        output_args.extend(["-map", "0:v", "-map", "0:a?", "-c", "copy"].map(String::from));
    } else {
        // GIF carries no audio at all -- never map or filter an audio stream
        // into it, even if the frontend sent one.
        let is_silent = req.format == "gif";
        let mut graphs = Vec::new();
        if let Some(v) = &req.filter_chain {
            graphs.push(v.clone());
        }
        if !is_silent {
            if let Some(a) = &req.audio_map {
                graphs.push(a.clone());
            }
        }
        if !graphs.is_empty() {
            output_args.push("-filter_complex".to_string());
            output_args.push(graphs.join(";"));
        }
        output_args.push("-map".to_string());
        output_args.push(if req.filter_chain.is_some() { "[vout]".to_string() } else { "0:v".to_string() });
        if !is_silent {
            output_args.push("-map".to_string());
            output_args.push(if req.audio_map.is_some() { "[aout]".to_string() } else { "0:a?".to_string() });
        }
        match req.format.as_str() {
            // "Lossless" has no bitrate of its own -- forcing it through
            // the bitrate branch below silently produced an ordinary 8000kbps
            // re-encode while still calling itself lossless. A real re-encode
            // that must stay lossless uses -crf 0 instead (same setting the
            // recording-conversion path already uses for its own mp4+lossless
            // case), never a bitrate target.
            "mp4" if req.lossless_quality => {
                output_args.extend(["-c:v", "libx264", "-crf", "0", "-preset", "veryfast"].map(String::from));
                output_args.extend(["-c:a", "aac", "-b:a", "320k"].map(String::from));
            }
            "mp4" => {
                output_args.extend(["-c:v", "libx264"].map(String::from));
                let kbps = req.video_kbps.unwrap_or(8000).clamp(500, 50000);
                output_args.extend(["-b:v".to_string(), format!("{kbps}k"), "-preset".to_string(), "medium".to_string()]);
                output_args.extend(["-c:a", "aac", "-b:a", "192k"].map(String::from));
            }
            "webm" if req.lossless_quality => {
                output_args.extend(["-c:v", "libvpx-vp9", "-lossless", "1"].map(String::from));
                output_args.extend(["-c:a", "libopus"].map(String::from));
            }
            "webm" => {
                output_args.extend(["-c:v", "libvpx-vp9"].map(String::from));
                let kbps = req.video_kbps.unwrap_or(4000).clamp(300, 50000);
                output_args.extend(["-b:v".to_string(), format!("{kbps}k")]);
                output_args.extend(["-c:a", "libopus"].map(String::from));
            }
            // The GIF preset's own filter_chain already ends in a palettegen +
            // paletteuse pair producing [vout]; -an drops any audio outright.
            "gif" => {
                output_args.push("-an".to_string());
            }
            // avi: the same codec family as mp4, repackaged into a container an
            // older program handles better.
            "avi" => {
                output_args.extend(["-c:v", "mpeg4"].map(String::from));
                let kbps = req.video_kbps.unwrap_or(8000).clamp(500, 50000);
                output_args.extend(["-b:v".to_string(), format!("{kbps}k")]);
                output_args.extend(["-c:a", "libmp3lame", "-b:a", "192k"].map(String::from));
            }
            _ => unreachable!(),
        }
    }
    let is_cancelled = || video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).contains(&my_gen);
    let staged = run_ffmpeg_to_staged(ffmpeg, src, dst, &input_args, &output_args, is_cancelled, &mut on_progress)?;
    // dest_path is the frontend's deterministic "<stem>-edit.<ext>" guess
    // -- exporting the same source in the same format twice must not silently
    // replace the first result, so the final name is only ever claimed if
    // free. reserve_unique_dest_path (unlike the plain unique_dest_path used
    // by the app's other save paths) makes that claim atomic: a file that
    // shows up at the winning name between the free-name search and the
    // rename below would otherwise get silently overwritten.
    let dir = dst.parent().unwrap_or_else(|| Path::new("."));
    let ext = dst.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let stem = dst.file_stem().and_then(|e| e.to_str()).unwrap_or("export");
    let final_dst = reserve_unique_dest_path(dir, stem, ext).map_err(|e| {
        let _ = fs::remove_file(&staged);
        let text = e.to_string();
        ffmpeg_error_json(classify_ffmpeg_error(&text), None, &text)
    })?;
    if let Err(e) = fs::rename(&staged, &final_dst) {
        // Clean up our own empty placeholder, never a file some other writer
        // might have claimed in its place -- reserve_unique_dest_path already
        // ruled that out, so final_dst is still ours to remove here.
        let _ = fs::remove_file(&final_dst);
        let _ = fs::remove_file(&staged);
        let text = e.to_string();
        return Err(ffmpeg_error_json(classify_ffmpeg_error(&text), None, &text));
    }
    Ok(final_dst.to_string_lossy().to_string())
}

// The video editor window (own label, one instance) and its own chrome:
// decorations(false)+transparent(true)+resizable(true), same shape as the
// main window's own (custom HTML titlebar drawn by video.html itself), not
// the fixed-size borderless-pill pattern the other satellite windows use.
pub(crate) const VIDEO_WIN_LABEL: &str = "video-editor";
pub(crate) const VIDEO_WIN_W: u32 = 1100;
pub(crate) const VIDEO_WIN_H: u32 = 720;
pub(crate) const VIDEO_WIN_MIN_W: u32 = 760;
pub(crate) const VIDEO_WIN_MIN_H: u32 = 520;

// Holds the recording path the (single) editor window should be showing, plus
// the origin folder to suggest on save when that path is a working copy made
// of a file opened from elsewhere for editing (empty for every other caller --
// a fresh recording or "Edit video" on an existing attachment, both of which
// keep editing the real file in place, same as before this point).
// Same pull pattern as VarPrompt/var_window_payload: a push at window-build
// time would race the webview's first frame, so the window asks for its own
// target once booted (video_editor_source), and again whenever
// "video-editor-load" fires for an already-open window.
// The third slot (overlay_path) is the camera dual-open: a camera recording
// whose companion screen recording has already finished lands here instead
// of staying empty, so the editor can add it as a video overlay itself
// instead of asking the user to pick it by hand. Its alignment offset is
// never carried alongside it here -- it lives in the camera recording's own
// sidecar (camera::CamSidecar::overlay_start_ms), which
// ui/video.js's addAlignedCameraOverlay reads directly.
pub(crate) struct VideoEditorPending(pub(crate) Mutex<(String, String, String)>);
impl VideoEditorPending {
    pub(crate) fn new() -> Self {
        Self(Mutex::new((String::new(), String::new(), String::new())))
    }
    pub(crate) fn lock(&self) -> std::sync::MutexGuard<'_, (String, String, String)> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Serialize, Clone)]
pub(crate) struct VideoEditorSource {
    pub(crate) path: String,
    pub(crate) origin_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) overlay_path: Option<String>,
}

// Opens the editor for one recording. A second call while it is already open
// just re-targets the existing window instead of stacking a second editor.
// origin_dir is Some only for a copy made of an externally opened file --
// see open_file_in_editor.
#[tauri::command]
pub(crate) async fn open_video_editor(
    app: AppHandle,
    pending: State<'_, VideoEditorPending>,
    src_path: String,
    origin_dir: Option<String>,
    overlay_path: Option<String>,
) -> Result<(), String> {
    if !PathBuf::from(&src_path).is_file() {
        return Err("recording file not found".to_string());
    }
    let origin_dir = origin_dir.unwrap_or_default();
    // An overlay path that no longer exists (a race with its own cleanup, or
    // simply never really there) is silently dropped rather than handed to
    // the editor, which would otherwise show it as a broken overlay op.
    let overlay_path = overlay_path.filter(|p| PathBuf::from(p).is_file()).unwrap_or_default();
    *pending.lock() = (src_path.clone(), origin_dir.clone(), overlay_path.clone());
    let overlay_opt = if overlay_path.is_empty() { None } else { Some(overlay_path) };
    if let Some(win) = app.get_webview_window(VIDEO_WIN_LABEL) {
        let _ = app.emit_to(
            VIDEO_WIN_LABEL,
            "video-editor-load",
            &VideoEditorSource { path: src_path, origin_dir, overlay_path: overlay_opt },
        );
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }
    // Never call Tauri window APIs while holding the Db lock (deadlock risk).
    let excluded = {
        let state: State<Db> = app.state();
        let store = lock(&state);
        capture_excluded(&store.settings)
    };
    let win = WebviewWindowBuilder::new(&app, VIDEO_WIN_LABEL, WebviewUrl::App("video.html".into()))
        .title(brand::DISPLAY_NAME)
        .inner_size(VIDEO_WIN_W as f64, VIDEO_WIN_H as f64)
        .min_inner_size(VIDEO_WIN_MIN_W as f64, VIDEO_WIN_MIN_H as f64)
        .decorations(false)
        .transparent(true)
        .resizable(true)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    set_capture_exclusion(&win, excluded);
    let geom = centered_on_primary(&win, VIDEO_WIN_W, VIDEO_WIN_H);
    let _ = win.set_position(PhysicalPosition::new(geom.x, geom.y));
    let _ = win.show();
    let _ = win.set_focus();
    Ok(())
}
