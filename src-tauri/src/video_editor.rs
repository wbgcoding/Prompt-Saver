use super::*;

// Video editor: probe, mux, filmstrip, waveform and still-frame helpers are read-only or
// one-shot commands the editor calls while a recording is open. export_video is the one
// additional ffmpeg entry point beyond convert_recording, sharing run_ffmpeg_to_staged so
// there is a single argument-building, progress and cancel implementation.

#[derive(serde::Serialize)]
pub(crate) struct RecordingProbe {
    pub(crate) duration_ms: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) fps: f64,
    pub(crate) has_audio: bool,
    pub(crate) has_mic_track: bool,
    pub(crate) mic_sidecar: Option<String>,
    // The lossless "-c copy" fast path only yields a valid mp4 when every mapped stream is a
    // codec mp4 can carry (a webm/VP9 source dragged in from elsewhere fails otherwise).
    pub(crate) stream_copy_safe: bool,
    // Whether the recording has a key-overlay sidecar (see keylog); the editor offers the keys
    // presets only then.
    pub(crate) has_keys_sidecar: bool,
}

// The text-overlay font list is built from SystemRoot (or windir), which Windows sets
// itself, so exports work when Windows is installed on another drive.
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

// Checks one extra video source (an inserted overlay or section clip) before the user
// builds on it. A failure comes back as the same classified JSON envelope export_video
// uses, so the editor's ffErr* messages (see classify_ffmpeg_error) cover this path too.
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

// The one place a key-sidecar JSON file becomes a KeySidecar, shared by the read command and
// the copy-safety check that runs before a sidecar follows its recording into an editor
// working copy, so both accept the same shape.
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

// The key-overlay editor's data fetch: a plain read of the sidecar JSON
// keylog::write_sidecar produced, or None when the recording never captured one (setting
// off, or older). A command like every other file access, never a direct filesystem call
// from the webview.
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

// Duration, width, height and size for a media attachment (file_path or icon_path),
// measured once when the attachment is set and stored on the Prompt, never on render. A
// still goes through the image crate, a video through ffprobe. Nothing returns an error: an
// unknown value is 0, which callers and the frontend treat as "show nothing".
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

// Only the four media fields: a confidential entry's stored prompt carries ciphertext, and
// the window merges this answer over its decrypted copy.
#[derive(serde::Serialize)]
pub(crate) struct MediaMeta {
    media_duration_ms: u64,
    media_width: u32,
    media_height: u32,
    media_size_bytes: u64,
}

// The attachment a backfill would measure: only an entry whose media meta is still all zero,
// and its file before its icon. Asked twice (before and after the slow probe), so a result is
// written only to the entry state it was measured for.
pub(crate) fn media_backfill_target(p: &Prompt) -> Option<String> {
    if p.media_duration_ms != 0 || p.media_width != 0 || p.media_size_bytes != 0 {
        return None;
    }
    if !p.file_path.is_empty() {
        Some(p.file_path.clone())
    } else if !p.icon_path.is_empty() {
        Some(p.icon_path.clone())
    } else {
        None
    }
}

// One-time catch-up for an entry never measured or whose first measurement failed (source
// briefly locked, network drive asleep): called the first time such a row is shown, never
// during render. A no-op (Ok(None), nothing saved) when the entry already has a
// measurement, has no video or image attachment, or is still unreadable.
#[tauri::command]
pub(crate) async fn backfill_media_meta(app: AppHandle, state: State<'_, Db>, id: String) -> Result<Option<MediaMeta>, String> {
    let media_path = {
        let store = lock(&state);
        if !store.settings.ui_flags.get("mediaMetaBackfill").copied().unwrap_or(true) {
            return Ok(None);
        }
        store.prompts.iter().find(|p| p.id == id).and_then(media_backfill_target)
    };
    let Some(media_path) = media_path else { return Ok(None) };
    let ffprobe = ffprobe_search_path(&app);
    let probed_path = media_path.clone();
    let (duration_ms, width, height, size) =
        tauri::async_runtime::spawn_blocking(move || probe_media_meta(ffprobe.as_deref(), Path::new(&probed_path)))
            .await
            .map_err(|e| e.to_string())?;
    if duration_ms == 0 && width == 0 && size == 0 {
        return Ok(None); // still unreadable: nothing changed, nothing to save
    }
    let mut store = lock(&state);
    // The probe ran without the lock: the attachment may have been replaced (and measured anew)
    // meanwhile, and then this result belongs to a file that is no longer the entry's.
    let updated = match store.prompts.iter_mut().find(|p| p.id == id && media_backfill_target(p).as_deref() == Some(media_path.as_str())) {
        Some(p) => {
            p.media_duration_ms = duration_ms;
            p.media_width = width;
            p.media_height = height;
            p.media_size_bytes = size;
            Some(MediaMeta {
                media_duration_ms: p.media_duration_ms,
                media_width: p.media_width,
                media_height: p.media_height,
                media_size_bytes: p.media_size_bytes,
            })
        }
        None => None,
    };
    if updated.is_some() {
        save_prompts(&app, &store);
    }
    Ok(updated)
}

// Consolidates a recording's mic sidecar (see mic_sidecar_video_path and the recording
// engine's mic_wav_writer_loop) into a second AAC track, once, the first time the editor
// opens a recording that still has one. Non-fatal: on failure the video plays
// system-audio-only and the sidecar stays for a retry.
#[tauri::command]
pub(crate) async fn mux_mic_track(app: AppHandle, state: State<'_, Db>, video_path: String, sidecar_path: String) -> Result<(), String> {
    let video = PathBuf::from(&video_path);
    let sidecar = PathBuf::from(&sidecar_path);
    if !video.is_file() || !sidecar.is_file() {
        return Err("recording or mic sidecar not found".to_string());
    }
    // mux_mic_track_blocking replaces video in place and deletes sidecar on success, so both
    // must be proven to belong to this app first: the video one of our recordings, the sidecar
    // exactly that video's own mic capture. The editor's working copy of an opened file counts
    // too (its mic track is what that copy's sidecar is for).
    if !is_own_recording_source(&recording_source_dirs(&app, &state), &video) && !is_editor_work_copy(&video) {
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
        // No -shortest: the mic writer can stop slightly before the video, and -shortest would
        // permanently truncate the video's tail to match. Without it the mic track just ends a
        // little early inside a video-length file.
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

// Thumbnail cache keyed by (path, mtime, requested count). count is in the key because each
// thumbnail's timestamp is a fraction of count, so reusing one count's files for another
// covered only part of the clip. Swept by age in sweep_stale_temp_files.
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

// No duration parameter: the editor already ran probe_recording before reaching the
// timeline, so ffprobe is known to be present.
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

// Peaks built while the decoded audio streams in: one running maximum per bucket, so memory
// is the bucket count, not the recording length (8 kHz mono s16 is 57.6 MB per hour). Bucket
// edges match pcm_s16le_to_peaks for a buffer of total_samples. Samples past the announced
// total fold into the last bucket.
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

// The running max is a u16: i16::MIN.unsigned_abs() is 32768, one past i16::MAX, so a signed
// max would wrap on exactly that sample. Normalized by dividing by 32768.0.
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

// Same screenshotFormat setting as the snip tool. ffmpeg encodes the still directly: the
// extension picks the encoder, -q:v is JPEG's quality knob (unset for lossless targets), and
// -lossless 1 keeps WebP lossless like encode_screenshot's WebP.
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

// Export request built by the frontend from its own operation list. filter_chain and
// audio_map are raw -filter_complex fragments; this backend only wires them together and
// maps their outputs, after export_video has checked each against export_graph (allowed
// filters, readable files).
//   filter_chain: ends with [vout] if present, else the source video is mapped unfiltered.
//   audio_map:    ends with [aout] if present, else the source audio (if any) is mapped
//                 unfiltered. Never sent for gif, which carries no audio.
// trim_start_ms is an input-side "-ss" (keyframe-snapped for lossless_copy) and trim_end_ms
// an output-side "-t <duration>", both applied before any filter graph, so a plain trim with
// lossless_copy never touches a filter graph or re-encodes.
#[derive(serde::Deserialize)]
pub(crate) struct ExportRequest {
    pub(crate) src_path: String,
    pub(crate) dest_path: String,
    pub(crate) filter_chain: Option<String>,
    pub(crate) audio_map: Option<String>,
    // Every media file the graph reads besides the source (overlay pictures and videos, inserted
    // clips, extra audio). A movie=/amovie= source may only open the source itself or one of these.
    #[serde(default)]
    pub(crate) media_paths: Vec<String>,
    pub(crate) lossless_copy: bool,
    // "Lossless" picked as a re-encode quality tier, distinct from lossless_copy (the
    // stream-copy fast path). True whenever the quality dropdown is on "lossless", even when
    // another op (crop, overlay, ...) still forces a real re-encode.
    pub(crate) lossless_quality: bool,
    pub(crate) format: String,
    pub(crate) video_kbps: Option<u32>,
    pub(crate) trim_start_ms: Option<u64>,
    pub(crate) trim_end_ms: Option<u64>,
    // The finished export's expected length. Only export_video's progress wrapper reads it, to
    // turn "ms encoded so far" into the percent shown in the running-jobs list.
    pub(crate) total_ms: u64,
}

// Each export owns the generation number it was handed at start. Batch exports can run
// several generations at once, so cancellation is a set of cancelled generations rather than
// one slot; a finished job removes its own entry so the set stays small.
pub(crate) static VIDEO_EXPORT_GEN: AtomicU64 = AtomicU64::new(0);
// The generation of the export the video editor itself has in flight (0 = none), so its
// Cancel hits exactly that one and not a batch export's.
pub(crate) static EDITOR_EXPORT_GEN: AtomicU64 = AtomicU64::new(0);

pub(crate) fn video_export_cancelled() -> &'static Mutex<std::collections::HashSet<u64>> {
    use std::sync::OnceLock;
    static SET: OnceLock<Mutex<std::collections::HashSet<u64>>> = OnceLock::new();
    SET.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

// The one folder exports may write into this session: the directory last picked in a folder
// dialog the backend ran itself. An export command never trusts a dest_path from the IPC
// call alone, since that call could be made with any argument.
pub(crate) static LAST_EXPORT_DEST_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

pub(crate) fn remember_export_dest_dir(dir: &Path) {
    if let Ok(canon) = fs::canonicalize(dir) {
        *LAST_EXPORT_DEST_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(canon);
    }
}

// A bounded set of files this session's export_video calls wrote: the backend's proof that
// a path is one of its own outputs, so save_recording_as can move it without widening its
// "rec-" recording-folder rule (an export can land in any folder the destination gate
// authorized). The oldest entry is dropped at the cap, like PICKER_GRANTS above.
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

// Moves a file and carries its export-output proof over to the new path (dropping the old
// entry), so the same export can be moved again from where it landed.
pub(crate) fn move_tracked_export_output(src: &Path, dst: &Path) -> std::io::Result<()> {
    let old = if is_known_export_output(src) { fs::canonicalize(src).ok() } else { None };
    move_file(src, dst)?;
    if let Some(old) = old {
        EXPORT_OUTPUTS.lock().unwrap_or_else(|e| e.into_inner()).retain(|p| p != &old);
        remember_export_output(dst);
    }
    Ok(())
}

// What export_video remembers once ffmpeg finishes: the path export_video_blocking returned,
// not the requested dest_path, since reserve_unique_dest_path may have renamed the file to a
// sibling when that name was taken.
pub(crate) fn export_output_to_remember(result: &Result<String, String>) -> Option<PathBuf> {
    result.as_ref().ok().map(PathBuf::from)
}

// Windows reserved device names, matched case-insensitively against the stem before its
// first dot ("CON.take2" is as reserved as "CON"). Keep in sync with ui/exportName.js.
pub(crate) const EXPORT_RESERVED_STEMS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4",
    "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

// A typed "File name" must stay one path segment: none of Windows' reserved filename
// characters, no "..", and not empty or "." alone. The frontend blocks this live; the same
// rule is re-applied to whatever dest_path arrives.
pub(crate) fn export_file_name_is_valid(name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return false;
    }
    // A ".." inside a longer name (e.g. "v1..2") cannot climb anywhere; only the exact "." and
    // ".." names are refused above.
    if trimmed.chars().any(|c| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || (c as u32) < 0x20) {
        return false;
    }
    // A trailing dot survives the trim above and Windows silently drops it from the real file
    // name.
    if trimmed.ends_with('.') {
        return false;
    }
    let first_segment = trimmed.split('.').next().unwrap_or("").to_ascii_uppercase();
    !EXPORT_RESERVED_STEMS.contains(&first_segment.as_str())
}

// A suggested file name from free text (a theme name, say): characters Windows does not
// allow become "-", and a result the validator above refuses falls back to `fallback`.
pub(crate) fn export_file_name_from(name: &str, fallback: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || (c as u32) < 0x20 { '-' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim_end().to_string();
    if export_file_name_is_valid(&cleaned) { cleaned } else { fallback.to_string() }
}

// Exporting a file over itself would have ffmpeg read and overwrite one file, so it is
// refused rather than left to reserve_unique_dest_path. dest need not exist, so only its
// parent is canonicalized; names compare case-insensitively, as Windows does.
pub(crate) fn export_dest_is_same_as_src(dest: &Path, src: &Path) -> bool {
    let Ok(src_canon) = fs::canonicalize(src) else { return false };
    let dest_parent = dest.parent().unwrap_or_else(|| Path::new("."));
    let Ok(dest_parent_canon) = fs::canonicalize(dest_parent) else { return false };
    let Some(dest_name) = dest.file_name() else { return false };
    let combined = dest_parent_canon.join(dest_name);
    combined.to_string_lossy().eq_ignore_ascii_case(&src_canon.to_string_lossy())
}

// The destination checks the single export applies, for one batch item: a valid file name
// and a target that is not the source itself (ffmpeg would overwrite the file it reads).
pub(crate) fn export_dest_is_usable(dest: &Path, src: &Path) -> bool {
    dest.file_name().and_then(|n| n.to_str()).is_some_and(export_file_name_is_valid) && !export_dest_is_same_as_src(dest, src)
}

// Whether dest's folder is one this session may export into: the folder from the last save
// dialog the backend ran, the standing videoExportDir preference, or the source file's own
// folder, but only when that source already lives in a folder the app controls (recording
// folder or configured export folder). Without that restriction an IPC-supplied src_path
// naming any file would authorize its parent as a destination. Everything is compared
// canonicalized so relative segments or letter case cannot step outside the picked folder.
// Split from export_dest_dir_is_authorized so the decision is a plain function of paths and
// testable without a running app.
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
    // The app's own temp folder, where clip_export_path writes the file a "copy to clipboard"
    // export produces.
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
    // A fresh capture still sits in <data>/recordings when the editor opens it, so that folder
    // counts as a recording folder here.
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

// The editor's Cancel: cancels the export the editor itself started, never a batch job that
// reserved a later generation. Nothing in flight leaves no stale entry behind.
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

// Per-row cancel of the batch list: targets one job by the generation in its bg-ops id
// ("export-<gen>") and leaves the rest running.
#[tauri::command]
pub(crate) fn cancel_export_job(generation: u64) {
    video_export_cancelled().lock().unwrap_or_else(|e| e.into_inner()).insert(generation);
}

// Global cancel (app quit, cancel_all_bg_ops) stops every export in flight, not just the
// current one: a batch can have several generations running, found by scanning the bg-ops
// registry for running "export" rows. The current generation is added directly in case its
// row is not written yet.
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

// The files stored as some prompt's attachment or icon: the user's own picks, which the
// editor may open from anywhere.
pub(crate) fn attached_media_paths(prompts: &[Prompt]) -> Vec<String> {
    prompts
        .iter()
        .flat_map(|p| [p.file_path.clone(), p.icon_path.clone()])
        .filter(|s| !s.is_empty())
        .collect()
}

// The sources the video editor currently has open or was just offered: only
// these (besides the app's own recordings and work copies) may be exported.
fn open_editor_source_paths(app: &AppHandle) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(pending) = app.try_state::<VideoEditorPending>() {
        out.push(pending.lock().0.clone());
    }
    if let Some(offered) = VE_PENDING_REQUEST.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        out.push(offered.0.clone());
    }
    out
}

// Whether `src` may feed an export: one of the app's own recordings, a work
// copy the editor made of an opened file, or exactly the file the editor was
// handed. A renderer-supplied path outside those is never read.
pub(crate) fn export_source_is_allowed(allowed_dirs: &[PathBuf], temp_root: &Path, editor_sources: &[String], src: &Path) -> bool {
    if is_own_recording_source(allowed_dirs, src) || is_editor_work_copy_in(temp_root, src) {
        return true;
    }
    let Ok(canon) = fs::canonicalize(src) else { return false };
    editor_sources
        .iter()
        .filter(|s| !s.is_empty())
        .any(|s| fs::canonicalize(s).is_ok_and(|c| c == canon))
}

// Progress reuses convert_recording's "recording-convert-progress" event. The taskbar and
// running-jobs registry are updated here rather than by the editor window's JS, so a job
// keeps reporting after the window that started it has closed.
#[tauri::command]
pub(crate) async fn export_video(app: AppHandle, state: State<'_, Db>, req: ExportRequest) -> Result<String, String> {
    if !is_video_format(&req.format) {
        return Err("unsupported format".to_string());
    }
    let src = PathBuf::from(&req.src_path);
    if !src.is_file() {
        return Err(ffmpeg_error_json(FfmpegErrorKey::Missing, None, ""));
    }
    let editor_sources = open_editor_source_paths(&app);
    let own_temp_dir = std::env::temp_dir().join(brand::TEMP_DIR);
    if !export_source_is_allowed(&recording_source_dirs(&app, &state), &own_temp_dir, &editor_sources, &src) {
        return Err("source-not-allowed".to_string());
    }
    let dst = PathBuf::from(&req.dest_path);
    // The file name is checked again here rather than trusted from the frontend's matching
    // validation, since dest_path ultimately comes from user input.
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
    let graphs: Vec<&str> = [req.filter_chain.as_deref(), req.audio_map.as_deref()].into_iter().flatten().collect();
    validate_export_request_graphs(&src, &req.media_paths, &graphs)?;
    let ffmpeg = ffmpeg_search_path(&app).ok_or_else(|| "ffmpeg-missing".to_string())?;
    let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    EDITOR_EXPORT_GEN.store(my_gen, Ordering::SeqCst);
    // Own registry key per job so a batch shows one row per export; "export" stays the kind the
    // frontend groups and labels by.
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
    // A panicked export task is an error result like any other, so the row below still leaves
    // "running" and the generation still leaves the cancel set.
    let result = result.unwrap_or_else(|e| Err(e.to_string()));
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

// One batch item's outcome: a missing source is rejected as export_video rejects it,
// otherwise it runs through export_video_blocking. AppHandle-free so a test can prove three
// sources give three files and one broken source does not stop the others. my_gen is
// reserved by the caller so its bg-ops row can be registered before the job starts.
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
        media_paths: Vec::new(),
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

// Awaits one in-flight export and records its outcome in the bg-ops registry; shared by the
// concurrency loop and the final drain.
pub(crate) async fn await_batch_export(handle: ExportJoinHandle, id: String, generation: u64, app: AppHandle) -> Result<bool, String> {
    // A panicked export task fails that one item like any encode error does; the
    // rest of the batch still runs and the item's row does not stay "running".
    let res = handle.await.unwrap_or_else(|e| Err(e.to_string()));
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

// One format/quality/target choice from the batch dialog, applied to every selected
// recording with a concurrency window of EXPORT_PARALLEL. ffmpeg decoding is heavy enough
// that four parallel encodes make a normal machine unpleasant. A source that fails to open
// or an encode that fails only counts against that file; the rest of the batch still runs.
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
    // The batch dialog lists only this app's own recordings, so nothing else may
    // be handed to ffmpeg through it.
    let allowed_sources = recording_source_dirs(&app, &state);

    for item in items {
        if in_flight.len() >= parallel {
            let (handle, id, generation, job_app) = in_flight.pop_front().unwrap();
            if await_batch_export(handle, id, generation, job_app).await? { ok += 1 } else { failed += 1 }
        }
        let item_src = PathBuf::from(&item.src_path);
        if !item_src.is_file() || !is_own_recording_source(&allowed_sources, &item_src) {
            failed += 1;
            continue;
        }
        if !export_dest_is_usable(&PathBuf::from(&item.dest_path), &item_src) {
            failed += 1;
            continue;
        }
        if !export_dest_dir_is_authorized(&app, &state, &PathBuf::from(&item.dest_path), &item_src) {
            failed += 1;
            continue;
        }
        // Reserved up front so the bg-ops "running" row appears the instant the job is queued,
        // before its spawn_blocking thread is scheduled.
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
    // A plain bitrate-based format gets a single number; the three image-based formats get a
    // low/high bracket, since their size depends on motion and colour count more than on a
    // bitrate. Exactly one of the two shapes is filled in.
    pub(crate) bytes: Option<u64>,
    pub(crate) low_bytes: Option<u64>,
    pub(crate) high_bytes: Option<u64>,
}

// Mirrors export_video_blocking's per-format "-b:a" settings so the estimate cannot drift
// from what the encoder writes. gif is silent (-an) and never reaches this function.
pub(crate) fn export_audio_kbps(format: &str, lossless_quality: bool) -> u32 {
    match format {
        "mp4" if lossless_quality => 320,
        "mp4" | "avi" => 192,
        "webm" => 128, // libopus with no explicit -b:a: ffmpeg's own default range
        _ => 0,
    }
}

// Pure and window-free so the formula has a unit test. A plain stream-copy export has no
// bitrate to estimate from; the frontend never calls this for it (see isLosslessCopy() in
// video.js).
pub(crate) fn estimate_export_bytes(format: &str, video_kbps: Option<u32>, lossless_quality: bool, duration_ms: u64) -> ExportSizeEstimate {
    let secs = duration_ms as f64 / 1000.0;
    match format {
        "gif" => {
            // A static, few-colour clip can run under 5 KB/s; fast full-frame motion exceeds 300 kbps
            // once palette dithering kicks in.
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
    let mut filter_script: Option<PathBuf> = None;
    if plain_copy {
        // Without an explicit -map, ffmpeg keeps only its single "best" audio stream, and a muxed-in
        // mic track (the second audio stream) would vanish on a lossless trim-only export.
        output_args.extend(["-map", "0:v", "-map", "0:a?", "-c", "copy"].map(String::from));
    } else {
        // GIF carries no audio: never map or filter an audio stream into it, even if the frontend
        // sent one.
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
            let script = write_filter_script(dst, &graphs.join(";"))?;
            // "-/filter_complex <file>" reads the graph from the file; the older -filter_complex_script is gone from the pinned ffmpeg.
            output_args.push("-/filter_complex".to_string());
            output_args.push(script.to_string_lossy().to_string());
            filter_script = Some(script);
        }
        output_args.push("-map".to_string());
        output_args.push(if req.filter_chain.is_some() { "[vout]".to_string() } else { "0:v".to_string() });
        if !is_silent {
            output_args.push("-map".to_string());
            output_args.push(if req.audio_map.is_some() { "[aout]".to_string() } else { "0:a?".to_string() });
        }
        match req.format.as_str() {
            // "Lossless" has no bitrate of its own; the bitrate branch below would produce an ordinary
            // 8000kbps re-encode that still calls itself lossless. A lossless re-encode uses -crf 0, as
            // the recording-conversion path does for mp4.
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
            // The GIF preset's filter_chain already ends in a palettegen + paletteuse pair producing
            // [vout]; -an drops any audio.
            "gif" => {
                output_args.push("-an".to_string());
            }
            // avi: the mp4 codec family in a container older programs handle better.
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
    let staged = run_ffmpeg_to_staged(ffmpeg, src, dst, &input_args, &output_args, is_cancelled, &mut on_progress);
    if let Some(script) = &filter_script {
        remove_filter_script(script);
    }
    let staged = staged?;
    // dest_path is the frontend's "<stem>-edit.<ext>" guess, and exporting the same source twice
    // must not replace the first result. reserve_unique_dest_path claims the final name
    // atomically, unlike unique_dest_path, so a file appearing at the winning name before the
    // rename below cannot be overwritten.
    let dir = dst.parent().unwrap_or_else(|| Path::new("."));
    let ext = dst.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let stem = dst.file_stem().and_then(|e| e.to_str()).unwrap_or("export");
    let final_dst = reserve_unique_dest_path(dir, stem, ext).map_err(|e| {
        let _ = fs::remove_file(&staged);
        let text = e.to_string();
        ffmpeg_error_json(classify_ffmpeg_error(&text), None, &text)
    })?;
    if let Err(e) = fs::rename(&staged, &final_dst) {
        // Remove our own empty placeholder; reserve_unique_dest_path guarantees final_dst is still
        // ours, never a file another writer claimed.
        let _ = fs::remove_file(&final_dst);
        let _ = fs::remove_file(&staged);
        let text = e.to_string();
        return Err(ffmpeg_error_json(classify_ffmpeg_error(&text), None, &text));
    }
    Ok(final_dst.to_string_lossy().to_string())
}

// The video editor window (own label, one instance): decorations(false), transparent and
// resizable, with a custom HTML titlebar drawn by video.html, unlike the fixed-size
// borderless-pill satellite windows.
pub(crate) const VIDEO_WIN_LABEL: &str = "video-editor";
pub(crate) const VIDEO_WIN_W: u32 = 1100;
pub(crate) const VIDEO_WIN_H: u32 = 720;
pub(crate) const VIDEO_WIN_MIN_W: u32 = 760;
pub(crate) const VIDEO_WIN_MIN_H: u32 = 520;

// Holds the recording path the single editor window should show, the origin folder to
// suggest on save when that path is a working copy of a file opened from elsewhere (empty
// otherwise), and overlay_path. Pull pattern as VarPrompt/var_window_payload: a push at
// window-build time would race the webview's first frame, so the window asks for its target
// once booted (video_editor_source) and again on "video-editor-load" for an already-open
// window.
// overlay_path is the camera dual-open: a camera recording whose companion screen recording
// has finished, so the editor can add it as a video overlay itself. Its alignment offset
// lives in the camera sidecar (camera::CamSidecar::overlay_start_ms), which ui/videoOverlays.js's
// addAlignedCameraOverlay reads directly.
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
    // Set on a "video-editor-load" for an open editor: it answers with
    // video_editor_load_ack so the caller learns whether the switch happened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_id: Option<u64>,
}

// A source offered to an already-open editor. It becomes the pending source only once the
// editor takes it; until then the old source stays protected and a page still booting pulls
// this one.
pub(crate) static VE_PENDING_REQUEST: Mutex<Option<(String, String, String)>> = Mutex::new(None);
static VE_LOAD_REQUEST_SEQ: AtomicU64 = AtomicU64::new(1);
pub(crate) static VE_LOAD_ACKS: Mutex<Option<HashMap<u64, std::sync::mpsc::Sender<String>>>> = Mutex::new(None);

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EditorLoadDecision {
    Accepted,
    Refused,
    NoAnswer,
}

// Waits for the editor's answer. No message within `first` means the page is
// not up yet (it pulls its source on boot); "asking" means the person is
// answering an unsaved-edits question, which gets the longer `asked` window,
// and running out of it counts as a refusal.
pub(crate) fn wait_for_editor_decision(rx: &std::sync::mpsc::Receiver<String>, first: Duration, asked: Duration) -> EditorLoadDecision {
    let mut wait = first;
    let mut asking = false;
    loop {
        match rx.recv_timeout(wait) {
            Ok(s) => match s.as_str() {
                "accepted" => return EditorLoadDecision::Accepted,
                "refused" => return EditorLoadDecision::Refused,
                "asking" => {
                    wait = asked;
                    asking = true;
                }
                _ => {}
            },
            Err(_) => return if asking { EditorLoadDecision::Refused } else { EditorLoadDecision::NoAnswer },
        }
    }
}

#[tauri::command]
pub(crate) fn video_editor_load_ack(request_id: u64, state: String) {
    if let Some(tx) = VE_LOAD_ACKS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&request_id)) {
        let _ = tx.send(state);
    }
}

// The editor window is gone (closed while an unsaved-edits question was open, or crashed):
// every caller still waiting for its answer gets "refused" at once instead of hanging for the
// long window, and the offered-but-never-taken source is dropped so a freshly opened editor
// boots on the newly requested file, not the stale offer.
pub(crate) fn refuse_pending_editor_requests() {
    let waiting = VE_LOAD_ACKS.lock().unwrap_or_else(|e| e.into_inner()).take();
    for tx in waiting.into_iter().flat_map(|m| m.into_values()) {
        let _ = tx.send("refused".to_string());
    }
    *VE_PENDING_REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

// Opens the editor for one recording; a second call re-targets the open window instead of
// stacking another. origin_dir is Some only for a copy of an externally opened file (see
// open_file_in_editor).
#[tauri::command]
pub(crate) async fn open_video_editor(
    app: AppHandle,
    state: State<'_, Db>,
    pending: State<'_, VideoEditorPending>,
    src_path: String,
    origin_dir: Option<String>,
    overlay_path: Option<String>,
) -> Result<bool, String> {
    if !PathBuf::from(&src_path).is_file() {
        return Err("recording file not found".to_string());
    }
    // Opening makes the path an allowed export source, so only files the app knows may be
    // opened: its own recordings, an editor work copy, or a prompt's attached file.
    let attached = attached_media_paths(&lock(&state).prompts);
    let own_temp_dir = std::env::temp_dir().join(brand::TEMP_DIR);
    if !export_source_is_allowed(&recording_source_dirs(&app, &state), &own_temp_dir, &attached, Path::new(&src_path)) {
        return Err("source-not-allowed".to_string());
    }
    let origin_dir = origin_dir.unwrap_or_default();
    // An overlay path that no longer exists is dropped rather than shown as a broken overlay op.
    let overlay_path = overlay_path.filter(|p| PathBuf::from(p).is_file()).unwrap_or_default();
    let overlay_opt = if overlay_path.is_empty() { None } else { Some(overlay_path.clone()) };
    if let Some(win) = app.get_webview_window(VIDEO_WIN_LABEL) {
        // The open editor may still refuse (unsaved edits kept), so the source is only offered here
        // and becomes pending once taken.
        let id = VE_LOAD_REQUEST_SEQ.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        VE_LOAD_ACKS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(id, tx);
        let request = (src_path.clone(), origin_dir.clone(), overlay_path);
        *VE_PENDING_REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(request.clone());
        let _ = app.emit_to(
            VIDEO_WIN_LABEL,
            "video-editor-load",
            &VideoEditorSource { path: src_path, origin_dir, overlay_path: overlay_opt, request_id: Some(id) },
        );
        let _ = win.show();
        let _ = win.set_focus();
        let decision = tauri::async_runtime::spawn_blocking(move || {
            wait_for_editor_decision(&rx, Duration::from_secs(2), Duration::from_secs(3600))
        })
        .await
        .unwrap_or(EditorLoadDecision::Refused);
        if let Some(m) = VE_LOAD_ACKS.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            m.remove(&id);
        }
        let mut slot = VE_PENDING_REQUEST.lock().unwrap_or_else(|e| e.into_inner());
        // A newer request replaced this one while it waited: that one decides.
        let still_mine = slot.as_ref() == Some(&request);
        if still_mine {
            *slot = None;
        }
        match decision {
            EditorLoadDecision::Refused => return Ok(false),
            EditorLoadDecision::Accepted => *pending.lock() = request,
            // No answer: the page was still booting and pulled the request itself.
            EditorLoadDecision::NoAnswer => {
                if still_mine {
                    *pending.lock() = request;
                }
            }
        }
        return Ok(true);
    }
    *pending.lock() = (src_path.clone(), origin_dir.clone(), overlay_path.clone());
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
    win.on_window_event(|event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            refuse_pending_editor_requests();
        }
    });
    set_capture_exclusion(&win, excluded);
    let geom = centered_on_primary(&win, VIDEO_WIN_W, VIDEO_WIN_H);
    let _ = win.set_position(PhysicalPosition::new(geom.x, geom.y));
    let _ = win.show();
    let _ = win.set_focus();
    Ok(true)
}
