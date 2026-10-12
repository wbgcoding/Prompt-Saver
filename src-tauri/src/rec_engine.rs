use super::*;

// ---------- Screen recording: recording engine core ----------
//
// Windows Graphics Capture (via the windows-capture crate) plus its own Media
// Foundation H.264 encoder. The target picker and control bar live further
// down (see "Run the overlay as a recording-target picker" and "Recording
// control bar" above); audio capture, quality/format options and further
// polish are still to come.
//
// Two research passes already paid for this design: a throwaway example
// proved start/pause/resume/stop on real hardware, and three traps were found
// and are worked around here:
// - CaptureControl::stop() posts WM_QUIT to the capture thread and skips
//   on_closed(), so the encoder must be finalised BEFORE stop() is called,
//   never inside on_closed().
// - CaptureControl::halt_handle() is the same flag stop() sets, not a pause --
//   pause is tracked with our own clock instead.
// - send_frame(&Frame) times video from WGC's own per-frame timestamp, which
//   keeps advancing during a pause and bakes a frozen-frame gap into the
//   output. send_frame_buffer(bytes, own_timestamp) with a clock that stops
//   advancing while paused avoids that.
//
// Everything below that has no Windows-specific type is kept outside the
// #[cfg(windows)] module so it stays unit-testable on any platform.

// The recording state machine: pure functions of the current phase, so the
// allowed transitions are testable without a capture session.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RecPhase {
    Idle,
    Recording,
    Paused,
}
pub(crate) fn rec_can_start(phase: RecPhase) -> bool {
    phase == RecPhase::Idle
}
pub(crate) fn rec_can_pause(phase: RecPhase) -> bool {
    phase == RecPhase::Recording
}
pub(crate) fn rec_can_resume(phase: RecPhase) -> bool {
    phase == RecPhase::Paused
}
pub(crate) fn rec_can_finish(phase: RecPhase) -> bool {
    // stop and cancel are reachable from the same two phases.
    matches!(phase, RecPhase::Recording | RecPhase::Paused)
}

// The "active recording time" a pause must not advance, in 100ns ticks (the
// same unit VideoEncoder::send_frame_buffer expects). Kept as a pure function
// of its inputs so the pause-freezes-time behaviour is testable without a
// real clock.
pub(crate) fn rec_clock_ticks(accumulated_ticks: i64, active_segment_elapsed_ticks: Option<i64>) -> i64 {
    accumulated_ticks.saturating_add(active_segment_elapsed_ticks.unwrap_or(0))
}

// rec-YYYY-MM-DD_HHMMSS.mp4, built from already-split components so the
// formatting is testable without touching the system clock.
pub(crate) fn rec_filename(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> String {
    format!("rec-{year:04}-{month:02}-{day:02}_{hour:02}{minute:02}{second:02}.mp4")
}

// Instant-based wrapper around rec_clock_ticks: accumulates active time across
// pauses and converts to 100ns ticks.
pub(crate) struct RecordClock {
    pub(crate) paused: std::sync::atomic::AtomicBool,
    pub(crate) accumulated_ticks: std::sync::atomic::AtomicI64,
    pub(crate) segment_start: Mutex<Option<std::time::Instant>>,
}
impl RecordClock {
    pub(crate) fn new() -> Self {
        Self {
            paused: std::sync::atomic::AtomicBool::new(false),
            accumulated_ticks: std::sync::atomic::AtomicI64::new(0),
            segment_start: Mutex::new(Some(std::time::Instant::now())),
        }
    }
    pub(crate) fn is_paused(&self) -> bool {
        self.paused.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub(crate) fn now_ticks(&self) -> i64 {
        let elapsed = self
            .segment_start
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .map(|t0| (t0.elapsed().as_nanos() / 100) as i64);
        rec_clock_ticks(self.accumulated_ticks.load(std::sync::atomic::Ordering::SeqCst), elapsed)
    }
    pub(crate) fn pause(&self) {
        if self.paused.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return; // already paused
        }
        let mut seg = self.segment_start.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(t0) = seg.take() {
            let add = (t0.elapsed().as_nanos() / 100) as i64;
            self.accumulated_ticks.fetch_add(add, std::sync::atomic::Ordering::SeqCst);
        }
    }
    pub(crate) fn resume(&self) {
        if !self.paused.swap(false, std::sync::atomic::Ordering::SeqCst) {
            return; // was not paused
        }
        *self.segment_start.lock().unwrap_or_else(|e| e.into_inner()) = Some(std::time::Instant::now());
    }
}

// Pure byte-shuffle: crops to (crop_x, crop_y, crop_w, crop_h) within a
// possibly padded, top-down source (row spacing `stride`), copying into a
// tightly packed, bottom-up destination -- what VideoEncoder::send_frame_buffer's
// own docs say it expects. A full-frame flip (238's monitor/window targets) is
// just this with the crop set to the whole frame; region recording (238's third
// target kind) uses a real sub-rectangle, since WGC itself has no region capture
// source and always delivers the whole monitor. Independent of any capture-frame
// type so it is unit-testable without a GPU or a Windows build.
#[allow(clippy::too_many_arguments)]
pub(crate) fn crop_and_flip_rows_bottom_up(
    raw: &[u8],
    src_width: usize,
    src_height: usize,
    stride: usize,
    crop_x: usize,
    crop_y: usize,
    crop_w: usize,
    crop_h: usize,
) -> Vec<u8> {
    let row_bytes = crop_w * 4;
    let mut out = vec![0u8; row_bytes * crop_h];
    if crop_x + crop_w > src_width {
        return out; // defensive -- a stale/out-of-range crop must not panic the capture thread
    }
    for row in 0..crop_h {
        let sy = crop_y + row;
        if sy >= src_height {
            break;
        }
        let src = sy * stride + crop_x * 4;
        if src + row_bytes > raw.len() {
            break;
        }
        let dst_row = crop_h - 1 - row;
        let dst = dst_row * row_bytes;
        out[dst..dst + row_bytes].copy_from_slice(&raw[src..src + row_bytes]);
    }
    out
}

// Re-anchors a top-down BGRA frame of its own live (width, height) into a
// fixed (target_w, target_h) top-down canvas, top-left corner, black-padding
// whatever the (smaller) live frame does not cover. Used only for the case a
// Window recording target shrank mid-capture (see RecHandler::on_frame_arrived,
// which routes a GROWN frame through scale_to_fit_frame_top_down instead --
// cropping it here would have cut off whatever grew past the canvas) --
// independent of the capture-frame type so it stays unit-testable.
pub(crate) fn letterbox_frame_top_down(
    raw: &[u8], width: usize, height: usize, stride: usize, target_w: usize, target_h: usize,
) -> (Vec<u8>, usize) {
    let dst_stride = target_w * 4;
    let mut out = vec![0u8; dst_stride * target_h];
    let copy_w = width.min(target_w);
    let copy_h = height.min(target_h);
    for row in 0..copy_h {
        let src = row * stride;
        let dst = row * dst_stride;
        if src + copy_w * 4 > raw.len() {
            break; // defensive -- a stale row_pitch must not panic the capture thread
        }
        out[dst..dst + copy_w * 4].copy_from_slice(&raw[src..src + copy_w * 4]);
    }
    (out, dst_stride)
}

// The plain top-left crop above silently cut off whatever grew past the
// encoder's fixed canvas on the right/bottom edge. When the live frame is
// bigger in either dimension, scale the WHOLE frame down instead (aspect
// ratio preserved, nearest-neighbour -- cheap enough for a capture thread and
// plenty for the brief span a live resize actually spans) and centre it, so
// nothing recorded is ever lost to a resize.
pub(crate) fn scale_to_fit_frame_top_down(
    raw: &[u8], width: usize, height: usize, stride: usize, target_w: usize, target_h: usize,
) -> (Vec<u8>, usize) {
    let dst_stride = target_w * 4;
    let mut out = vec![0u8; dst_stride * target_h];
    if width == 0 || height == 0 || target_w == 0 || target_h == 0 {
        return (out, dst_stride);
    }
    let scale = (target_w as f64 / width as f64).min(target_h as f64 / height as f64);
    let scaled_w = ((width as f64 * scale).round() as usize).clamp(1, target_w);
    let scaled_h = ((height as f64 * scale).round() as usize).clamp(1, target_h);
    let (off_x, off_y) = ((target_w - scaled_w) / 2, (target_h - scaled_h) / 2);
    for dy in 0..scaled_h {
        let sy = ((dy as f64 / scale) as usize).min(height - 1);
        let src_row = sy * stride;
        if src_row + width * 4 > raw.len() {
            break; // defensive -- a stale row_pitch must not panic the capture thread
        }
        let dst_row = (off_y + dy) * dst_stride + off_x * 4;
        for dx in 0..scaled_w {
            let sx = ((dx as f64 / scale) as usize).min(width - 1);
            let (s, d) = (src_row + sx * 4, dst_row + dx * 4);
            out[d..d + 4].copy_from_slice(&raw[s..s + 4]);
        }
    }
    (out, dst_stride)
}

// Places a top-down BGRA frame of its own live (width, height) at (at_x, at_y)
// inside a fixed (canvas_w, canvas_h) top-down output buffer, black-padding
// whatever the frame does not cover and silently clipping whatever falls
// outside the buffer on any edge. A multi-display recording canvas can start
// at a negative coordinate (a second display placed left of or above the first one), and a
// tracked window dragged to the very edge of that canvas can still overhang
// it by a pixel or two -- both are ordinary placements here, not error cases.
// `letterbox_frame_top_down` is the (0, 0) case of this, kept as its own name
// since its own two existing tests call it directly. Independent of any
// capture-frame type so it stays unit-testable without a GPU or a Windows
// build.
#[allow(clippy::too_many_arguments)]
pub(crate) fn place_frame_in_canvas(
    raw: &[u8], width: usize, height: usize, stride: usize,
    canvas_w: usize, canvas_h: usize, at_x: i64, at_y: i64,
) -> (Vec<u8>, usize) {
    let dst_stride = canvas_w * 4;
    let mut out = vec![0u8; dst_stride * canvas_h];
    for row in 0..height {
        let dy = at_y + row as i64;
        if dy < 0 || dy as usize >= canvas_h {
            continue;
        }
        let src = row * stride;
        // Clip horizontally: skip source columns that would land left of
        // x=0, then stop at the buffer's right edge.
        let src_x0 = if at_x < 0 { (-at_x) as usize } else { 0 };
        if src_x0 >= width {
            continue;
        }
        let dst_x0 = (at_x + src_x0 as i64).max(0) as usize;
        if dst_x0 >= canvas_w {
            continue;
        }
        let copy_w = (width - src_x0).min(canvas_w - dst_x0);
        if copy_w == 0 {
            continue;
        }
        let src_off = src + src_x0 * 4;
        if src_off + copy_w * 4 > raw.len() {
            continue; // defensive -- a stale row_pitch must not panic the capture thread
        }
        let dst_off = (dy as usize) * dst_stride + dst_x0 * 4;
        out[dst_off..dst_off + copy_w * 4].copy_from_slice(&raw[src_off..src_off + copy_w * 4]);
    }
    (out, dst_stride)
}

// The combined-display rectangle (369's default recording canvas for a
// Window target), via the same desktop-geometry metrics Windows itself
// already tracks -- a plain four-integer query, not an image read, and
// already the union of every display, so no per-display enumeration or
// union math is needed on top of it.
#[cfg(windows)]
pub(crate) fn virtual_screen_bounds() -> Result<(i32, i32, u32, u32), String> {
    #[link(name = "user32")]
    extern "system" {
        fn GetSystemMetrics(index: i32) -> i32;
    }
    const SM_XVIRTUALSCREEN: i32 = 76;
    const SM_YVIRTUALSCREEN: i32 = 77;
    const SM_CXVIRTUALSCREEN: i32 = 78;
    const SM_CYVIRTUALSCREEN: i32 = 79;
    let (x, y, w, h) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    if w <= 0 || h <= 0 {
        return Err("could not read the combined display size".to_string());
    }
    Ok((x, y, w as u32, h as u32))
}

// Whether an output size of (w, h) is small enough to even attempt (369's
// safety guard against the memory cost -- and possible outright refusal --
// of a very large combined-display output). `build_encoder` is still tried
// on top of this and can still fail on its own; this only short-circuits the
// obviously too-large case before paying for that attempt.
pub(crate) fn canvas_within_ceiling(w: u32, h: u32, ceiling_w: u32, ceiling_h: u32) -> bool {
    w <= ceiling_w && h <= ceiling_h
}

// A value from an imported settings file or a stray manual edit can carry
// anything; fall back to the smallest, most conservative capture ("window")
// for any string that is not one of the three modes this actually knows.
pub(crate) fn sanitize_canvas_mode(raw: &str) -> &'static str {
    match raw {
        "monitor" => "monitor",
        "all" => "all",
        _ => "window",
    }
}

// Largest width and height, in pixels, the recording canvas may reach. A set
// of screens wider than this falls back to the screen the window is on.
pub(crate) const REC_CANVAS_MAX_PX: u32 = 8192;

// How much of one 1-second watch tick counts as recorded time.
pub(crate) fn active_tick_secs(paused: bool) -> u64 {
    if paused { 0 } else { 1 }
}

// Pure predicate for the two optional recording safety nets (recMaxMin and
// recMaxMb, both 0 = off) -- kept free of the watch thread's sleep loop so it
// is unit-testable without spinning up a real recording.
pub(crate) fn recording_limit_reached(elapsed_secs: u64, file_len: u64, max_min: f64, max_mb: f64) -> bool {
    let by_time = max_min > 0.0 && elapsed_secs >= (max_min * 60.0) as u64;
    let by_size = max_mb > 0.0 && file_len >= (max_mb * 1024.0 * 1024.0) as u64;
    by_time || by_size
}

// The bounds of whichever display currently contains (x, y) -- 369's "the
// display the window is on" fallback tier, using the same Win32 lookup
// `resolve_region_monitor` already does for region recordings (no second way
// to ask which display a point belongs to).
#[cfg(windows)]
pub(crate) fn monitor_rect_at(x: i32, y: i32) -> Option<(i32, i32, u32, u32)> {
    let (hmon, ..) = resolve_region_monitor(x, y, 1, 1).ok()?;
    let (left, top, right, bottom) = monitor_bounds(hmon as isize)?;
    Some((left, top, (right - left).max(1) as u32, (bottom - top).max(1) as u32))
}

// The part of a frame placed at (at_x, at_y) that really lands on the canvas, as
// (x, y, w, h); None when none of it does. The used-area crop built from these
// must stay inside the encoded picture, or ffmpeg refuses the crop.
pub(crate) fn visible_canvas_rect(at_x: i64, at_y: i64, width: usize, height: usize, canvas_w: usize, canvas_h: usize) -> Option<(u32, u32, u32, u32)> {
    let x0 = at_x.max(0);
    let y0 = at_y.max(0);
    let x1 = (at_x + width as i64).min(canvas_w as i64);
    let y1 = (at_y + height as i64).min(canvas_h as i64);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
}

// Tracks the union of every placed frame's own rectangle over a recording,
// so the post-capture crop step knows the smallest rectangle that
// still contains everything actually shown, rather than the full output
// size -- which is mostly black once that size covers every display. Four
// `AtomicU32`s rather than a `Mutex<Rect>`: `on_frame_arrived` runs on the
// capture thread and must never block behind a lock a slow reader could be
// holding.
pub(crate) struct UsedAreaTracker {
    pub(crate) min_x: std::sync::atomic::AtomicU32,
    pub(crate) min_y: std::sync::atomic::AtomicU32,
    pub(crate) max_x: std::sync::atomic::AtomicU32, // exclusive (x + w)
    pub(crate) max_y: std::sync::atomic::AtomicU32, // exclusive (y + h)
    pub(crate) seen: std::sync::atomic::AtomicBool,
}
impl UsedAreaTracker {
    pub(crate) fn new() -> Self {
        Self {
            min_x: std::sync::atomic::AtomicU32::new(u32::MAX),
            min_y: std::sync::atomic::AtomicU32::new(u32::MAX),
            max_x: std::sync::atomic::AtomicU32::new(0),
            max_y: std::sync::atomic::AtomicU32::new(0),
            seen: std::sync::atomic::AtomicBool::new(false),
        }
    }
    // (x, y) is the frame's top-left placement in output coordinates
    // (already clamped by the caller), (w, h) its size.
    pub(crate) fn update(&self, x: u32, y: u32, w: u32, h: u32) {
        use std::sync::atomic::Ordering::Relaxed;
        self.min_x.fetch_min(x, Relaxed);
        self.min_y.fetch_min(y, Relaxed);
        self.max_x.fetch_max(x + w, Relaxed);
        self.max_y.fetch_max(y + h, Relaxed);
        self.seen.store(true, Relaxed);
    }
    // The union rectangle over every update() call so far, or None if there
    // was never one.
    pub(crate) fn get(&self) -> Option<(u32, u32, u32, u32)> {
        use std::sync::atomic::Ordering::Relaxed;
        if !self.seen.load(Relaxed) {
            return None;
        }
        let (min_x, min_y) = (self.min_x.load(Relaxed), self.min_y.load(Relaxed));
        let (max_x, max_y) = (self.max_x.load(Relaxed), self.max_y.load(Relaxed));
        Some((min_x, min_y, max_x.saturating_sub(min_x), max_y.saturating_sub(min_y)))
    }
}

// 369's bitrate scaling: `recBitrateKbps` is defined as the rate for a
// full-canvas recording, so when the canvas ends up larger than the window
// itself (the every-display case), the encoder's bitrate is scaled down by
// the fraction the window occupies of that canvas -- otherwise a small
// window recorded on a huge canvas would spend most of its budget on pixels
// that are just going to be black. Floored well above zero so a tiny window
// on a huge canvas never gets an unusably low rate, and never scaled UP past
// the configured value even if the "window" size handed in is somehow
// already bigger than the canvas.
pub(crate) fn scale_bitrate_to_used_area(full_canvas_kbps: u32, used_w: u32, used_h: u32, canvas_w: u32, canvas_h: u32) -> u32 {
    if canvas_w == 0 || canvas_h == 0 {
        return full_canvas_kbps;
    }
    let fraction = (used_w as f64 * used_h as f64) / (canvas_w as f64 * canvas_h as f64);
    ((full_canvas_kbps as f64 * fraction).round() as u32).clamp(500, full_canvas_kbps)
}

// ---------- Screen recording: audio (loopback + microphone) ----------
//
// windows-capture's own VideoEncoder already accepts an AAC audio track
// (AudioSettingsBuilder, send_audio_buffer) -- no custom Media Foundation
// sink writer needed. What it does NOT do is capture the audio itself: that
// is plain WASAPI (system loopback on the default render endpoint, the
// microphone on the default -- or a chosen -- capture endpoint), converted to
// this fixed target format before it reaches the encoder. Recording always
// asks the encoder for exactly this format, so only the CAPTURE side ever
// has to adapt to a device's own native format.
pub(crate) const REC_AUDIO_SAMPLE_RATE: u32 = 48_000;
pub(crate) const REC_AUDIO_CHANNELS: u32 = 2;

// f32 sample in [-1.0, 1.0] (WASAPI's shared-mode mix format is virtually
// always IEEE float) -> i16 PCM, hard-clipped rather than wrapped.
pub(crate) fn audio_f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

// Down/up-mixes an interleaved f32 buffer from `in_channels` to `out_channels`
// (mono -> stereo duplicates the one channel; N>2 -> stereo keeps the first
// two and drops the rest -- good enough for a screen-recording voice/system
// track, not a mixing console) and resamples to `out_rate` with linear
// interpolation (adequate for speech/system audio at these sample rates; a
// polyphase resampler would be overkill for this use case). Returns
// interleaved i16 PCM at `out_channels`/`out_rate`.
pub(crate) fn audio_convert_to_target(
    samples: &[f32],
    in_channels: u32,
    in_rate: u32,
    out_channels: u32,
    out_rate: u32,
) -> Vec<i16> {
    if in_channels == 0 || out_channels == 0 || in_rate == 0 {
        return Vec::new();
    }
    let in_frames = samples.len() / in_channels as usize;
    // Channel-adapt first, in f32, one row per output channel.
    let mut adapted = vec![0f32; in_frames * out_channels as usize];
    for frame in 0..in_frames {
        for oc in 0..out_channels as usize {
            let ic = if in_channels == 1 { 0 } else { oc.min(in_channels as usize - 1) };
            adapted[frame * out_channels as usize + oc] = samples[frame * in_channels as usize + ic];
        }
    }
    if in_rate == out_rate {
        return adapted.into_iter().map(audio_f32_to_i16).collect();
    }
    // Linear-interpolation resample, per output channel.
    let out_frames = ((in_frames as u64) * out_rate as u64 / in_rate as u64) as usize;
    let mut out = vec![0i16; out_frames * out_channels as usize];
    for frame in 0..out_frames {
        let src_pos = frame as f64 * in_rate as f64 / out_rate as f64;
        let i0 = src_pos.floor() as usize;
        let frac = (src_pos - i0 as f64) as f32;
        let i1 = (i0 + 1).min(in_frames.saturating_sub(1));
        let i0 = i0.min(in_frames.saturating_sub(1));
        for oc in 0..out_channels as usize {
            let a = adapted[i0 * out_channels as usize + oc];
            let b = adapted[i1 * out_channels as usize + oc];
            out[frame * out_channels as usize + oc] = audio_f32_to_i16(a + (b - a) * frac);
        }
    }
    out
}

// Sums two equal-length interleaved i16 buffers (both sources active), with
// saturating (hard-clip) addition rather than wraparound -- clipping on loud
// combined peaks is far less objectionable than digital wraparound noise.
pub(crate) fn audio_mix_i16(a: &[i16], b: &[i16]) -> Vec<i16> {
    a.iter().zip(b.iter()).map(|(&x, &y)| x.saturating_add(y)).collect()
}

// Minimal MP4 box walker, used only to measure each track's own declared
// duration for the audio/video drift check below -- no new dependency, no
// Media Foundation reader needed (the file's own box tree already carries
// the answer, same "trust the output file's own numbers" approach 191 used
// for its pause-gap proof).
#[cfg(test)]
pub(crate) fn mp4_child_boxes(data: &[u8]) -> Vec<(String, std::ops::Range<usize>)> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 8 <= data.len() {
        let size32 = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as u64;
        let typ = String::from_utf8_lossy(&data[i + 4..i + 8]).into_owned();
        let (header_len, box_size) = if size32 == 1 {
            if i + 16 > data.len() {
                break;
            }
            (16usize, u64::from_be_bytes(data[i + 8..i + 16].try_into().unwrap()))
        } else if size32 == 0 {
            (8usize, (data.len() - i) as u64)
        } else {
            (8usize, size32)
        };
        if box_size < header_len as u64 {
            break;
        }
        let end = i + box_size as usize;
        if end > data.len() || end <= i {
            break;
        }
        out.push((typ, (i + header_len)..end));
        i = end;
    }
    out
}

// Descends `path` (e.g. ["moov", "trak"]), fanning out at every level that
// has more than one match (there are two "trak" boxes once audio is on) --
// so this returns every leaf box matching the full path, in file order.
#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)] // a Vec<Range> search scope, not an index list
pub(crate) fn mp4_find_path(data: &[u8], path: &[&str]) -> Vec<std::ops::Range<usize>> {
    let mut current = vec![0..data.len()];
    for name in path {
        let mut next = Vec::new();
        for range in &current {
            for (typ, r) in mp4_child_boxes(&data[range.clone()]) {
                if typ == *name {
                    next.push((range.start + r.start)..(range.start + r.end));
                }
            }
        }
        current = next;
    }
    current
}

// One track's own declared duration in seconds, from its mdhd box
// (moov/trak/mdia/mdhd) -- version 0 uses 32-bit fields, version 1 uses
// 64-bit creation/modification/duration with a still-32-bit timescale.
// Handler-type fourCC per track (moov/trak/mdia/hdlr), e.g. "vide"/"soun" --
// verifies which declared duration in mp4_track_durations_secs actually
// belongs to which stream, rather than assuming trak order.
#[cfg(test)]
pub(crate) fn mp4_track_handler_types(data: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for trak in mp4_find_path(data, &["moov", "trak"]) {
        let trak_bytes = &data[trak.clone()];
        let Some(hdlr) = mp4_find_path(trak_bytes, &["mdia", "hdlr"]).into_iter().next() else { continue };
        let b = &trak_bytes[hdlr];
        if b.len() >= 12 {
            out.push(String::from_utf8_lossy(&b[8..12]).into_owned());
        }
    }
    out
}

#[cfg(test)]
pub(crate) fn mp4_track_durations_secs(data: &[u8]) -> Vec<f64> {
    let mut out = Vec::new();
    for trak in mp4_find_path(data, &["moov", "trak"]) {
        let trak_bytes = &data[trak.clone()];
        let Some(mdhd) = mp4_find_path(trak_bytes, &["mdia", "mdhd"]).into_iter().next() else { continue };
        let b = &trak_bytes[mdhd];
        if b.is_empty() {
            continue;
        }
        let version = b[0];
        let (timescale, duration) = if version == 0 && b.len() >= 20 {
            let timescale = u32::from_be_bytes(b[12..16].try_into().unwrap()) as u64;
            let duration = u32::from_be_bytes(b[16..20].try_into().unwrap()) as u64;
            (timescale, duration)
        } else if version == 1 && b.len() >= 32 {
            let timescale = u32::from_be_bytes(b[20..24].try_into().unwrap()) as u64;
            let duration = u64::from_be_bytes(b[24..32].try_into().unwrap());
            (timescale, duration)
        } else {
            continue;
        };
        if timescale > 0 {
            out.push(duration as f64 / timescale as f64);
        }
    }
    out
}

// Producer (a WASAPI capture thread) / consumer (the recording feeder's fixed-
// cadence audio tick) queue of already-converted interleaved i16 PCM at the
// fixed target format above. Capped well above one tick's worth so a capture
// thread that briefly gets ahead does not grow without bound; the cap is
// generous (2s) since audio packets arrive in small, frequent bursts, unlike
// video frames.
pub(crate) struct AudioQueueShared {
    pub(crate) samples: Mutex<std::collections::VecDeque<i16>>,
    pub(crate) lost: std::sync::atomic::AtomicBool,
    // The rec bar's mute buttons flip this instead of stopping the
    // WASAPI capture thread itself -- push() below just stops feeding the
    // queue while it is set, and drain()'s existing zero-pad-on-underflow
    // (a few lines down) does the rest, so the muted span of the track comes
    // out as real silence rather than a gap, with zero change to timing.
    pub(crate) muted: std::sync::atomic::AtomicBool,
}
impl AudioQueueShared {
    pub(crate) fn new() -> Self {
        Self {
            samples: Mutex::new(std::collections::VecDeque::new()),
            lost: std::sync::atomic::AtomicBool::new(false),
            muted: std::sync::atomic::AtomicBool::new(false),
        }
    }
    pub(crate) fn push(&self, chunk: &[i16]) {
        if self.muted.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let cap = (REC_AUDIO_SAMPLE_RATE * REC_AUDIO_CHANNELS * 2) as usize; // ~2s
        let mut q = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        q.extend(chunk.iter().copied());
        while q.len() > cap {
            q.pop_front();
        }
    }
    pub(crate) fn set_muted(&self, muted: bool) {
        self.muted.store(muted, std::sync::atomic::Ordering::Relaxed);
    }
    pub(crate) fn is_muted(&self) -> bool {
        self.muted.load(std::sync::atomic::Ordering::Relaxed)
    }
    // Drains exactly `n` samples, zero-padding (silence) if fewer are
    // available -- the tick must never block or shrink to keep the encoder's
    // own monotonic audio clock in step with real wall-clock time.
    pub(crate) fn drain(&self, n: usize) -> Vec<i16> {
        let mut q = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(q.pop_front().unwrap_or(0));
        }
        out
    }
    pub(crate) fn mark_lost(&self) {
        self.lost.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    pub(crate) fn is_lost(&self) -> bool {
        self.lost.load(std::sync::atomic::Ordering::Relaxed)
    }
    // Discards whatever is currently queued -- used once, the moment the
    // first video frame arrives, to throw away audio captured during WGC's
    // own startup latency (real, measured: a few hundred ms before the first
    // frame shows up). Without this, audio ticking (which starts as soon as
    // the feeder thread exists, well before any video) would establish an
    // audio "time zero" earlier than video's own "time zero" (the first
    // frame's own timestamp, per send_frame_buffer's self-normalization) --
    // measured as an audio track running ~0.3s longer than the video track
    // in every real recording before this fix.
    pub(crate) fn clear(&self) {
        self.samples.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

// A same-stem "<name>.mic.wav" sidecar for the microphone track when a
// recording captures both system and mic audio. Two independent AAC tracks
// in the same MP4 is the end state the video editor produces, but writing
// them at capture time would mean the encoder (windows-capture's own
// MediaStreamSource-based VideoEncoder) needs a second, simultaneously
// selectable audio stream, which that encoder does not support -- and a
// custom Media Foundation sink writer just for this would put fresh risk on
// an already-shipped, freshly-stabilised recording engine. So capture keeps
// writing exactly one AAC track (system, when present) into the mp4, as
// before, and the second source goes to this plain PCM sidecar instead; the
// editor muxes the two into one two-track mp4 the first time it opens the
// recording (see video_editor::mux_mic_track's contract).
// A hand-rolled RIFF/WAVE header rather than a new crate dependency -- the format
// is small and fixed (mono/stereo 16-bit PCM), not worth a dep. The header is 80
// bytes: RIFF, WAVE, a 28-byte JUNK chunk that is the room a 64-bit size table
// needs, fmt, data. Up to 4 GiB the file is an ordinary WAV (every reader skips the
// JUNK); past that finish() turns the same bytes into an RF64 file whose JUNK chunk
// becomes the ds64 size table, so a long mic track stays readable.
pub(crate) const REC_WAV_HEADER_BYTES: u64 = 80;
const REC_WAV_DATA_SIZE_OFFSET: u64 = 76;

pub(crate) struct RecWavWriter {
    pub(crate) file: fs::File,
    pub(crate) data_bytes: u64,
    block_align: u16,
}
impl RecWavWriter {
    pub(crate) fn create(path: &Path, sample_rate: u32, channels: u16) -> std::io::Result<Self> {
        use std::io::Write;
        let mut file = fs::File::create(path)?;
        let bits_per_sample: u16 = 16;
        let block_align = channels * (bits_per_sample / 8);
        let byte_rate = sample_rate * block_align as u32;
        file.write_all(b"RIFF")?;
        file.write_all(&0u32.to_le_bytes())?; // patched by finish()
        file.write_all(b"WAVE")?;
        file.write_all(b"JUNK")?;
        file.write_all(&28u32.to_le_bytes())?;
        file.write_all(&[0u8; 28])?; // becomes the ds64 table past 4 GiB
        file.write_all(b"fmt ")?;
        file.write_all(&16u32.to_le_bytes())?;
        file.write_all(&1u16.to_le_bytes())?; // PCM
        file.write_all(&channels.to_le_bytes())?;
        file.write_all(&sample_rate.to_le_bytes())?;
        file.write_all(&byte_rate.to_le_bytes())?;
        file.write_all(&block_align.to_le_bytes())?;
        file.write_all(&bits_per_sample.to_le_bytes())?;
        file.write_all(b"data")?;
        file.write_all(&0u32.to_le_bytes())?; // patched by finish()
        Ok(Self { file, data_bytes: 0, block_align })
    }
    pub(crate) fn write_samples(&mut self, samples: &[i16]) -> std::io::Result<()> {
        use std::io::Write;
        for s in samples {
            self.file.write_all(&s.to_le_bytes())?;
        }
        self.data_bytes = self.data_bytes.saturating_add(samples.len() as u64 * 2);
        Ok(())
    }
    // Seeks back to patch the placeholder sizes now that the real total is
    // known -- streamed formats (WAV included) put their length in the header,
    // so it cannot be written until the last sample has landed.
    pub(crate) fn finish(mut self) -> std::io::Result<()> {
        use std::io::{Seek, SeekFrom, Write};
        // Everything after the first 8 bytes: the header minus RIFF and its size, plus the data.
        let riff_size = (REC_WAV_HEADER_BYTES - 8).saturating_add(self.data_bytes);
        if riff_size <= u32::MAX as u64 {
            self.file.seek(SeekFrom::Start(4))?;
            self.file.write_all(&(riff_size as u32).to_le_bytes())?;
            self.file.seek(SeekFrom::Start(REC_WAV_DATA_SIZE_OFFSET))?;
            self.file.write_all(&(self.data_bytes as u32).to_le_bytes())?;
        } else {
            let samples = self.data_bytes / self.block_align.max(1) as u64;
            self.file.seek(SeekFrom::Start(0))?;
            self.file.write_all(b"RF64")?;
            self.file.write_all(&u32::MAX.to_le_bytes())?; // the real size lives in ds64
            self.file.seek(SeekFrom::Start(12))?;
            self.file.write_all(b"ds64")?;
            self.file.seek(SeekFrom::Start(20))?;
            self.file.write_all(&riff_size.to_le_bytes())?;
            self.file.write_all(&self.data_bytes.to_le_bytes())?;
            self.file.write_all(&samples.to_le_bytes())?;
            self.file.write_all(&0u32.to_le_bytes())?; // no extra size table entries
            self.file.seek(SeekFrom::Start(REC_WAV_DATA_SIZE_OFFSET))?;
            self.file.write_all(&u32::MAX.to_le_bytes())?;
        }
        Ok(())
    }
}

// The mp4 sibling of a "<name>.mic.wav" sidecar, or None if `path` is not one.
pub(crate) fn mic_sidecar_video_path(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".mic.wav")?;
    Some(path.with_file_name(format!("{stem}.mp4")))
}

// The mp4 sibling of a "<name>.keys.json" sidecar, or None if `path` is not
// one -- same shape as mic_sidecar_video_path above, kept free of the
// keylog module (and its cfg(windows) gate) since the stale-recordings sweep
// this feeds must reason about `.keys.json` on every target, not just
// Windows. Without this, sweep_stale_recordings_in only ever
// treated a key-overlay sidecar as unreferenced -- it was deleted out from
// under a KEPT recording as soon as it aged past the stale window, even
// though the mp4 itself was correctly kept.
pub(crate) fn keys_sidecar_video_path(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".keys.json")?;
    Some(path.with_file_name(format!("{stem}.mp4")))
}

pub(crate) const REC_QUEUE_CAP: usize = 4; // bounded frame queue -- drop-oldest once the feeder falls behind

pub(crate) struct RecFrameItem {
    pub(crate) bytes: Vec<u8>,
    pub(crate) ts: i64,
}

// Shared between the WGC capture callback (producer) and the encoder feeder
// thread (consumer). windows-capture's own internal channel from our callback
// to its transcode thread is unbounded and un-instrumentable, so a slow
// encoder must be caught here instead: a full queue drops the OLDEST frame
// rather than blocking the capture callback (and with it the desktop
// compositor). Dropped frames are counted and logged, never silent.
pub(crate) struct RecFeederShared {
    pub(crate) queue: Mutex<std::collections::VecDeque<RecFrameItem>>,
    pub(crate) cv: std::sync::Condvar,
    pub(crate) dropped: std::sync::atomic::AtomicU64,
    pub(crate) stopping: std::sync::atomic::AtomicBool,
    // First unrecoverable encoder write failure (e.g. disk full), if any --
    // surfaced to the user rather than left as a silently truncated recording.
    // Only the first is kept: a failing encoder tends to fail identically on
    // every following frame, and one message is all the user needs.
    pub(crate) encode_error: Mutex<Option<String>>,
    // Set once, the same instant the audio feeder sees its first video
    // frame -- lets a parallel mic-sidecar writer thread (RecWavWriter, spawned
    // only when both system and mic are captured) discard its own pre-roll at
    // the same "time zero" the main track uses, without stealing frames from
    // the feeder's single-consumer queue to find that moment itself.
    pub(crate) first_frame: std::sync::atomic::AtomicBool,
    // Written only by the capture callback (never the feeder thread), the
    // same "window size" canvas default's resize-safety-net path
    // (scale_to_fit_frame_top_down) that shrinks a grown window's picture to
    // keep fitting the encoder's original, smaller canvas -- the exact
    // moment content starts being lost off the edge. Largest frame size ever
    // seen (for a possible future message; only the timestamp is shown
    // today) plus the timestamp of the FIRST such frame, encoded as ms + 1 so
    // 0 unambiguously means "never happened" -- a real first frame at ms=0 is
    // not something this recorder can produce anyway (on_frame_arrived does
    // real work before ever reading the clock a second time).
    pub(crate) resize_crop_max_w: std::sync::atomic::AtomicU32,
    pub(crate) resize_crop_max_h: std::sync::atomic::AtomicU32,
    pub(crate) resize_crop_first_ms_plus_one: std::sync::atomic::AtomicU64,
    // Set at most once per recording, by the disk-space watch thread, the
    // first time free space drops below the warning threshold -- read (and cleared)
    // by the live recording_status poll so the control bar can show it
    // exactly once, never stopping the recording (that is the hard, separate
    // "low-disk-space" threshold at warn_mb/4 below).
    pub(crate) disk_low: Mutex<Option<(u64, u64)>>,
}
impl RecFeederShared {
    pub(crate) fn new() -> Self {
        Self {
            queue: Mutex::new(std::collections::VecDeque::new()),
            cv: std::sync::Condvar::new(),
            dropped: std::sync::atomic::AtomicU64::new(0),
            stopping: std::sync::atomic::AtomicBool::new(false),
            encode_error: Mutex::new(None),
            first_frame: std::sync::atomic::AtomicBool::new(false),
            resize_crop_max_w: std::sync::atomic::AtomicU32::new(0),
            resize_crop_max_h: std::sync::atomic::AtomicU32::new(0),
            resize_crop_first_ms_plus_one: std::sync::atomic::AtomicU64::new(0),
            disk_low: Mutex::new(None),
        }
    }
    // Called only from on_frame_arrived's legacy resize-safety-net branch, at
    // the exact instant a grown window's picture is being shrunk to keep
    // fitting the canvas -- never from the 369 placement-canvas branch, which
    // is deliberately oversized and can never lose content this way.
    pub(crate) fn mark_resize_crop(&self, width: u32, height: u32, ts_ticks: i64) {
        use std::sync::atomic::Ordering::Relaxed;
        self.resize_crop_max_w.fetch_max(width, Relaxed);
        self.resize_crop_max_h.fetch_max(height, Relaxed);
        let ts_ms = (ts_ticks / 10_000).max(0) as u64;
        let _ = self.resize_crop_first_ms_plus_one.compare_exchange(0, ts_ms + 1, Relaxed, Relaxed);
    }
    // Safe to call only once the capture callback is provably done (after
    // finish_and_stop has run session.control.stop()) -- the same threading
    // rule as the used-area tracker: written only by the capture thread,
    // read only once that thread is guaranteed to have stopped writing.
    pub(crate) fn resize_crop_at_ms(&self) -> Option<u64> {
        use std::sync::atomic::Ordering::Relaxed;
        let plus_one = self.resize_crop_first_ms_plus_one.load(Relaxed);
        (plus_one > 0).then(|| plus_one - 1)
    }
    pub(crate) fn mark_first_frame(&self) {
        self.first_frame.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    pub(crate) fn has_first_frame(&self) -> bool {
        self.first_frame.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub(crate) fn note_disk_low(&self, free_mb: u64, est_seconds: u64) {
        let mut d = self.disk_low.lock().unwrap_or_else(|e| e.into_inner());
        if d.is_none() {
            *d = Some((free_mb, est_seconds));
        }
    }
    // Consuming: recording_status() takes it exactly once, so the control
    // bar's toast fires exactly once per recording, not on every poll tick.
    pub(crate) fn take_disk_low(&self) -> Option<(u64, u64)> {
        self.disk_low.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
    pub(crate) fn set_encode_error(&self, msg: String) {
        let mut e = self.encode_error.lock().unwrap_or_else(|e| e.into_inner());
        if e.is_none() {
            *e = Some(msg);
        }
    }
    // Non-consuming: the live status poll (recording_status) and the final
    // stop payload both need to read this, and the live poll must not eat it
    // before the stop payload gets a turn.
    pub(crate) fn peek_encode_error(&self) -> Option<String> {
        self.encode_error.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub(crate) fn push(&self, item: RecFrameItem) {
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        if q.len() >= REC_QUEUE_CAP {
            q.pop_front();
            self.dropped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        q.push_back(item);
        drop(q);
        self.cv.notify_one();
    }
    pub(crate) fn pop_or_stop(&self) -> Option<RecFrameItem> {
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(item) = q.pop_front() {
                return Some(item);
            }
            if self.stopping.load(std::sync::atomic::Ordering::Relaxed) {
                return None;
            }
            q = self.cv.wait(q).unwrap_or_else(|e| e.into_inner());
        }
    }
    // Non-blocking pop, used by the audio-tick feeder loop: video frames are
    // drained whenever available between ticks, never waited on (the audio
    // tick's own pacing must not stall behind an empty video queue).
    pub(crate) fn try_pop(&self) -> Option<RecFrameItem> {
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        q.pop_front()
    }
    pub(crate) fn request_stop(&self) {
        self.stopping.store(true, std::sync::atomic::Ordering::Relaxed);
        self.cv.notify_all();
    }
    // Lets the capture callback stop pushing NEW frames the instant a stop
    // was requested, symmetric with how the audio capture threads stop
    // producing once audio_stop fires -- without this, video kept accepting
    // frames for as long as WGC stayed alive (control.stop() only runs after
    // the encoder is finalised, see Trap 1 below), while audio had already
    // gone silent, so the video track ended up measurably longer than the
    // audio track at the end of every recording.
    pub(crate) fn is_stopping(&self) -> bool {
        self.stopping.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(windows)]
pub(crate) struct RecordingState(pub(crate) Mutex<Option<recording::RecSession>>);
#[cfg(not(windows))]
pub(crate) struct RecordingState(pub(crate) Mutex<Option<()>>);

#[derive(serde::Serialize, Clone)]
pub(crate) struct RecordingStoppedPayload {
    pub(crate) path: String,
    pub(crate) dropped_frames: u64,
    pub(crate) encode_error: Option<String>,
    // Set when both system and mic were captured -- the video editor
    // consolidates this sidecar into the mp4's second AAC track on open.
    pub(crate) mic_wav_path: Option<String>,
    // One sentence naming the canvas a window recording actually used
    // (e.g. "every display", "the display the window is on", "the
    // window's own size") -- None for Monitor/Region targets, which have
    // exactly one candidate and nothing to report.
    pub(crate) canvas_note: Option<String>,
    // True when the window used less of its canvas than the canvas itself,
    // leaving a black margin the frontend should now trim automatically
    // (crop_recording_to_used_area) before showing the result. Always
    // false for Monitor/Region and for the "window's own size" fallback.
    pub(crate) needs_crop: bool,
    // Milliseconds into the recording when the "window size" canvas default
    // first had to shrink the picture to keep it fitting (the window grew
    // past its starting size) -- None if that never happened. Drives the
    // reactive notice offering to switch recCanvasMode to "all" from now on.
    pub(crate) resize_crop_at_ms: Option<u64>,
    // Every earlier part of this recording, oldest first, when the "finish and
    // continue" button was used at least once -- empty for a plain recording,
    // so existing callers reading only `path` see no change at all. The
    // frontend chains [...parts, { path, mic_wav_path }] when it wants the
    // full sequence, e.g. to open every part in the video editor one after
    // another with its own mic sidecar muxed in.
    pub(crate) parts: Vec<RecordingPartInfo>,
}

#[derive(serde::Serialize, Clone)]
pub(crate) struct RecordingPartInfo {
    pub(crate) path: String,
    pub(crate) mic_wav_path: Option<String>,
}

// A cancel discards the WHOLE take, not just its current file -- every
// earlier part a "finish and continue" chain already closed goes with it,
// video, mic audio and its own key-overlay sidecar all three, into the
// holding area when possible. Best-effort: a missing file (already
// moved/deleted by the user) is not a reason to fail an otherwise-successful
// cancel.
#[cfg(windows)]
pub(crate) fn remove_recording_part_files(part: &RecordingPartInfo) {
    crate::media_files::discard_or_remove_take(Path::new(&part.path));
    if let Some(w) = &part.mic_wav_path {
        let _ = std::fs::remove_file(w);
    }
    let _ = std::fs::remove_file(sidecar_path(Path::new(&part.path), KEYS_SIDECAR_EXT));
}

#[derive(serde::Serialize, Clone)]
pub(crate) struct RecordingContinuedPayload {
    // The just-finished part's own outcome -- surfaced here (rather than only
    // logged) because this part gets no result dialog of its own; the final
    // stop is still the only place a dropped-frame count or encode error
    // becomes visible to the user, via `RecordingStoppedPayload.parts`.
    pub(crate) finished_part_path: String,
    pub(crate) finished_part_dropped_frames: u64,
    pub(crate) finished_part_encode_error: Option<String>,
    pub(crate) new_path: String,
    pub(crate) part_number: u32,
    // How long the switch itself took, start to finish -- shown briefly in
    // the control bar so the missing moment between the two files is never
    // silently hidden from the user.
    pub(crate) gap_ms: u64,
}
