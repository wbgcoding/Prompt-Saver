use super::{
    append_crash_log, audio_mix_i16, crop_and_flip_rows_bottom_up, free_space_bytes, letterbox_frame_top_down,
    scale_to_fit_frame_top_down, rec_filename, free_rec_path, now_ms,
    AudioQueueShared, RecWavWriter,
    RecFeederShared, RecFrameItem, RecPhase, RecordClock, REC_AUDIO_CHANNELS, REC_AUDIO_SAMPLE_RATE,
    canvas_within_ceiling, external_window_rect, monitor_rect_at, place_frame_in_canvas, visible_canvas_rect,
    scale_bitrate_to_used_area, virtual_screen_bounds, recording_limit_reached,
    estimate_recording_seconds_remaining, active_tick_secs, DISK_CHECK_SECS, UsedAreaTracker,
};
use crate::keylog;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::Media::Audio::{eCapture, eRender};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::encoder::{
    AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder, VideoSettingsSubType,
};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;
// Re-exported for the test module's border_setting_for_target assertions.
#[cfg(test)]
pub(crate) use windows_capture::settings::DrawBorderSettings as BorderSetting;

pub type RecError = Box<dyn std::error::Error + Send + Sync>;

// Target selected for recording, identified like the snip overlay identifies windows and
// monitors (raw HMONITOR/HWND). WGC has no region source, so Region captures the whole
// `monitor` and crops to (x, y, w, h), relative to that monitor's top-left, in the encoder
// pipeline.
#[derive(Clone, Copy)]
pub enum RecTarget {
    Monitor(isize),
    Window(isize),
    Region { monitor: isize, x: i32, y: i32, w: u32, h: u32 },
}

// What RecTarget captures from WGC's point of view: each kind needs its own Settings::new
// call (different item types), but Region shares the Monitor path and only the crop differs.
enum CaptureItem {
    Monitor(Monitor),
    Window(Window),
}

fn resolve_capture_item(target: RecTarget) -> Result<CaptureItem, String> {
    match target {
        RecTarget::Monitor(0) => Ok(CaptureItem::Monitor(Monitor::primary().map_err(|e| e.to_string())?)),
        RecTarget::Monitor(h) | RecTarget::Region { monitor: h, .. } => {
            Ok(CaptureItem::Monitor(Monitor::from_raw_hmonitor(h as *mut std::ffi::c_void)))
        }
        RecTarget::Window(h) => {
            let w = Window::from_raw_hwnd(h as *mut std::ffi::c_void);
            if !w.is_valid() {
                return Err("window is no longer valid".to_string());
            }
            Ok(CaptureItem::Window(w))
        }
    }
}

// Pure: the part of a monitor-relative region that lies on a frame_w x frame_h monitor, as
// (x, y, w, h), or None when nothing of it does.
pub(crate) fn clip_region_to_frame(x: i32, y: i32, w: u32, h: u32, frame_w: u32, frame_h: u32) -> Option<(u32, u32, u32, u32)> {
    let x0 = (x as i64).max(0);
    let y0 = (y as i64).max(0);
    let x1 = (x as i64 + w as i64).min(frame_w as i64);
    let y1 = (y as i64 + h as i64).min(frame_h as i64);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
}

// A Region target's rectangle clipped to its monitor and rounded to even sizes, or None for
// Monitor/Window targets. This is both the encoder size and the crop applied to each captured
// frame; a region with nothing on the monitor is refused rather than recorded black.
fn resolve_region_clip(target: RecTarget) -> Result<Option<(u32, u32, u32, u32)>, String> {
    let RecTarget::Region { monitor, x, y, w, h } = target else {
        return Ok(None);
    };
    if w == 0 || h == 0 {
        return Err("region has no size".to_string());
    }
    let (frame_w, frame_h) = if monitor == 0 {
        let m = Monitor::primary().map_err(|e| e.to_string())?;
        (m.width().map_err(|e| e.to_string())?, m.height().map_err(|e| e.to_string())?)
    } else {
        let m = Monitor::from_raw_hmonitor(monitor as *mut std::ffi::c_void);
        (m.width().map_err(|e| e.to_string())?, m.height().map_err(|e| e.to_string())?)
    };
    let (cx, cy, cw, ch) = clip_region_to_frame(x, y, w, h, frame_w, frame_h).ok_or_else(|| "region has no size".to_string())?;
    Ok(Some((cx, cy, even_dim(cw), even_dim(ch))))
}

// The H.264 Media Foundation encoder rejects an odd width or height (0xC00DA412), so sizes
// are rounded down to even (staying inside the captured area), with a floor of 2.
pub(crate) fn even_dim(n: u32) -> u32 {
    (n & !1).max(2)
}

// Removes the output file a failed start left behind: the encoder creates it before it can
// reject the type, so a refused start leaves a 0-byte file. A file with content is never
// touched. Returns whether it deleted.
pub(crate) fn discard_empty_output(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(m) if m.is_file() && m.len() == 0 => fs::remove_file(path).is_ok(),
        _ => false,
    }
}

// A Region target crops a borderless whole-monitor capture, so the OS capture border would
// ring the entire monitor instead of the recorded area; it is switched off for Region and
// replaced by the outline window. Monitor and Window targets keep the OS border.
pub(crate) fn border_setting_for_target(target: RecTarget) -> DrawBorderSettings {
    match target {
        RecTarget::Region { .. } => DrawBorderSettings::WithoutBorder,
        RecTarget::Monitor(_) | RecTarget::Window(_) => DrawBorderSettings::Default,
    }
}

// Pure: the outline window's rect in absolute (physical) screen pixels for a region
// capture, given its monitor's top-left. Mirrors resolve_region_clip's monitor-relative (x, y).
pub(crate) fn region_outline_rect(monitor_origin: (i32, i32), x: i32, y: i32, w: u32, h: u32) -> (i32, i32, u32, u32) {
    (monitor_origin.0 + x.max(0), monitor_origin.1 + y.max(0), w, h)
}

// The outline window's rect for this target, or None for Monitor/Window targets, which keep
// the OS border.
pub(crate) fn outline_rect_for_target(target: RecTarget) -> Option<(i32, i32, u32, u32)> {
    match target {
        RecTarget::Region { monitor, x, y, w, h } => {
            let bounds = super::monitor_bounds(monitor);
            let origin = bounds.map(|(l, t, _, _)| (l, t)).unwrap_or((0, 0));
            // Show only the part that is recorded.
            let (x, y, w, h) = match bounds {
                Some((l, t, r, b)) => match clip_region_to_frame(x, y, w, h, (r - l).max(0) as u32, (b - t).max(0) as u32) {
                    Some((cx, cy, cw, ch)) => (cx as i32, cy as i32, cw, ch),
                    None => (x, y, w, h),
                },
                None => (x, y, w, h),
            };
            Some(region_outline_rect(origin, x, y, w, h))
        }
        _ => None,
    }
}

// A Window recording whose canvas is bigger than the window (the every-display or
// current-display fallback tier): each frame is placed at the window's live screen position
// inside that fixed canvas, and the union of placements is tracked for the post-capture
// crop. Absent for Monitor/Region targets and the "window's own size" tier.
struct WindowPlacement {
    hwnd: isize,
    // The canvas' top-left in GetWindowRect's coordinate space (virtual-screen pixels),
    // subtracted from the window's live position to get its offset inside the canvas.
    canvas_origin: (i32, i32),
    used_area: Arc<UsedAreaTracker>,
}

pub(crate) struct RecHandler {
    clock: Arc<RecordClock>,
    shared: Arc<RecFeederShared>,
    crop: Option<(u32, u32, u32, u32)>,
    // The encoder's fixed output size, chosen once from the target at start(). A Window target
    // can be resized live while WGC keeps delivering frames at the current size; without this
    // the byte-buffer size would change on the next frame and send_frame_buffer would reject
    // it, silently ending capture.
    enc_size: (u32, u32),
    placement: Option<WindowPlacement>,
    // Set only on the handler a live target switch just started, flipped once its first frame
    // is pushed, so the switch can wait for it before closing the old handler without losing
    // anything at the seam. None for every ordinary handler.
    first_frame_signal: Option<Arc<AtomicBool>>,
}
impl GraphicsCaptureApiHandler for RecHandler {
    type Flags = (
        Arc<RecordClock>, Arc<RecFeederShared>, Option<(u32, u32, u32, u32)>, (u32, u32),
        Option<(isize, (i32, i32), Arc<UsedAreaTracker>)>,
        Option<Arc<AtomicBool>>,
    );
    type Error = RecError;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let (clock, shared, crop, enc_size, placement, first_frame_signal) = ctx.flags;
        let placement = placement.map(|(hwnd, canvas_origin, used_area)| WindowPlacement { hwnd, canvas_origin, used_area });
        Ok(Self { clock, shared, crop, enc_size, placement, first_frame_signal })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if self.clock.is_paused() || self.shared.is_stopping() {
            return Ok(());
        }
        let ts = self.clock.now_ticks();
        let mut fb = frame.buffer()?;
        let (width, height, stride) = (fb.width() as usize, fb.height() as usize, fb.row_pitch() as usize);
        let bytes = if let Some(placement) = &self.placement {
            // Place this window's frame at its live screen position inside the fixed canvas. If the
            // rect cannot be read this instant (e.g. the window just closed), keep the last slot rather
            // than dropping the frame.
            let live_origin = external_window_rect(placement.hwnd)
                .map(|(l, t, _, _)| (l, t))
                .unwrap_or(placement.canvas_origin);
            let at_x = (live_origin.0 - placement.canvas_origin.0) as i64;
            let at_y = (live_origin.1 - placement.canvas_origin.1) as i64;
            let (ew, eh) = (self.enc_size.0 as usize, self.enc_size.1 as usize);
            if let Some((vx, vy, vw, vh)) = visible_canvas_rect(at_x, at_y, width, height, ew, eh) {
                placement.used_area.update(vx, vy, vw, vh);
            }
            let (padded, padded_stride) = place_frame_in_canvas(fb.as_raw_buffer(), width, height, stride, ew, eh, at_x, at_y);
            crop_and_flip_rows_bottom_up(&padded, ew, eh, padded_stride, 0, 0, ew, eh)
        } else if self.crop.is_none() && (even_dim(width as u32), even_dim(height as u32)) != self.enc_size {
            // Live resize: re-anchor the frame into the encoder's original canvas instead of passing
            // the mismatched size through. Grown past the canvas in either dimension, the whole picture
            // is scaled down to fit; shrunk in both, it is black-padded, since a smaller frame already
            // shows everything.
            let (ew, eh) = (self.enc_size.0 as usize, self.enc_size.1 as usize);
            let (padded, padded_stride) = if width > ew || height > eh {
                self.shared.mark_resize_crop(width as u32, height as u32, ts);
                scale_to_fit_frame_top_down(fb.as_raw_buffer(), width, height, stride, ew, eh)
            } else {
                letterbox_frame_top_down(fb.as_raw_buffer(), width, height, stride, ew, eh)
            };
            crop_and_flip_rows_bottom_up(&padded, ew, eh, padded_stride, 0, 0, ew, eh)
        } else {
            // An odd-sized frame whose even-rounded size is the encoder size drops its last odd
            // row/column here.
            let (cx, cy, cw, ch) = self.crop.unwrap_or((0, 0, even_dim(width as u32), even_dim(height as u32)));
            crop_and_flip_rows_bottom_up(
                fb.as_raw_buffer(), width, height, stride,
                cx as usize, cy as usize, cw as usize, ch as usize,
            )
        };
        self.shared.push(RecFrameItem { bytes, ts });
        if let Some(sig) = &self.first_frame_signal {
            sig.store(true, Ordering::Release);
        }
        Ok(())
    }
}

// audio_kbps None disables the encoder's audio track (no active source); Some(kbps) enables
// it at the fixed target format (48 kHz stereo 16-bit AAC) that audio_convert_to_target
// and the feeder's audio tick already produce.
fn build_encoder(
    width: u32, height: u32, path: &Path, audio_kbps: Option<u32>, video_kbps: u32, video_fps: u32,
) -> Result<VideoEncoder, RecError> {
    // VideoSettingsBuilder defaults to HEVC; the app ships MP4/H.264, so the sub type is set
    // explicitly.
    let audio_settings = match audio_kbps {
        Some(kbps) => AudioSettingsBuilder::default()
            .bitrate(kbps.saturating_mul(1000))
            .channel_count(REC_AUDIO_CHANNELS)
            .sample_rate(REC_AUDIO_SAMPLE_RATE)
            .disabled(false),
        None => AudioSettingsBuilder::default().disabled(true),
    };
    let encoder = VideoEncoder::new(
        VideoSettingsBuilder::new(width, height)
            .sub_type(VideoSettingsSubType::H264)
            .frame_rate(video_fps)
            .bitrate(video_kbps.saturating_mul(1000)),
        audio_settings,
        ContainerSettingsBuilder::default(),
        path,
    )?;
    Ok(encoder)
}

// Config for the two optional audio sources. With neither active, start() takes the
// video-only path (build_encoder with None audio, plain feeder_loop).
#[derive(Clone)]
pub struct AudioConfig {
    pub system: bool,
    pub mic: bool,
    pub mic_device_id: String,
    pub kbps: u32,
}

fn feeder_loop(shared: Arc<RecFeederShared>, mut encoder: VideoEncoder) {
    // During a live target switch the old and new capture push for a moment, each stamping its
    // frame before its own copy work, so the queue can hold an older stamp behind a newer one,
    // which the encoder must never see.
    let mut last_sent: Option<i64> = None;
    while let Some(item) = shared.pop_or_stop() {
        let ts = strictly_increasing_ts(&mut last_sent, item.ts);
        if let Err(e) = encoder.send_frame_buffer(&item.bytes, ts) {
            append_crash_log(&format!("recording: send_frame_buffer failed: {e}"));
            shared.set_encode_error(e.to_string());
            shared.request_stop(); // further frames would just fail the same way
            break;
        }
    }
    if let Err(e) = encoder.finish() {
        append_crash_log(&format!("recording: encoder finish failed: {e}"));
        shared.set_encode_error(e.to_string());
    }
}

/// Returns `ts`, or one tick past the last sent timestamp when `ts` would not
/// move forward, and records the result as the new last one.
pub(crate) fn strictly_increasing_ts(last_sent: &mut Option<i64>, ts: i64) -> i64 {
    let out = match *last_sent {
        Some(prev) if ts <= prev => prev + 1,
        _ => ts,
    };
    *last_sent = Some(out);
    out
}

/// Sends every frame the capture callback has queued so far, in order. The
/// first one also starts the clock-gated audio: whatever the audio queues
/// collected before it is discarded.
fn drain_pending_frames(
    shared: &Arc<RecFeederShared>,
    encoder: &mut VideoEncoder,
    queues: &[Arc<AudioQueueShared>],
    video_started: &mut bool,
    last_frame: &mut Option<Vec<u8>>,
    last_frame_at: &mut Option<Instant>,
    last_sent: &mut Option<i64>,
) {
    while let Some(item) = shared.try_pop() {
        if !*video_started {
            *video_started = true;
            let at = keylog::mono_ms();
            shared.mark_first_frame(at);
            // The video track starts at this frame: the key overlay counts from here too.
            keylog::rebase_start(at);
            for q in queues {
                q.clear();
            }
        }
        let ts = strictly_increasing_ts(last_sent, item.ts);
        if let Err(e) = encoder.send_frame_buffer(&item.bytes, ts) {
            append_crash_log(&format!("recording: send_frame_buffer failed: {e}"));
            shared.set_encode_error(e.to_string());
            shared.request_stop(); // further frames would just fail the same way
            return;
        }
        *last_frame = Some(item.bytes);
        *last_frame_at = Some(Instant::now());
    }
}

// Audio-enabled variant: a fixed 20ms tick drains the active audio queue(s) (mixed when
// system and mic are both on) into the encoder's audio track, gated by the same RecordClock
// pause state as the video path. Video frames are drained non-blockingly between ticks; the
// tick pacing keeps the encoder's audio clock (derived from cumulative samples sent, see
// send_audio_buffer) in step with wall-clock time.
//
// Ticking starts only once the first video frame has arrived: WGC takes a few hundred ms
// after session start to deliver it, and video's time zero is that frame's timestamp
// (send_frame_buffer normalizes on its first call). Ticking from thread start would give
// audio a head start no video covers, making the audio track run ~0.3s longer. Audio
// captured during that startup gap is discarded so the first tick matches video's zero.
fn feeder_loop_with_audio(
    shared: Arc<RecFeederShared>,
    mut encoder: VideoEncoder,
    clock: Arc<RecordClock>,
    queues: Vec<Arc<AudioQueueShared>>,
) {
    const TICK: Duration = Duration::from_millis(20);
    let samples_per_tick = (REC_AUDIO_SAMPLE_RATE as usize / 50) * REC_AUDIO_CHANNELS as usize; // 20ms
    let mut next_tick = Instant::now() + TICK;
    let mut video_started = false;
    // WGC calls on_frame_arrived only when the surface changes, so an idle screen can go over a
    // second without a frame. The video track's duration would then trail real elapsed time by up
    // to ~1s, truncating the recording and breaking audio/video sync. So the last frame is
    // resent with a fresh timestamp once per tick when nothing new arrived, keeping video's PTS
    // within one tick of real time.
    let mut last_frame: Option<Vec<u8>> = None;
    let mut last_frame_at: Option<Instant> = None;
    // The last timestamp handed to the encoder: every later frame, a resent
    // hold frame included, must be strictly greater.
    let mut last_sent: Option<i64> = None;
    loop {
        drain_pending_frames(&shared, &mut encoder, &queues, &mut video_started, &mut last_frame, &mut last_frame_at, &mut last_sent);
        if shared.stopping.load(Ordering::Relaxed) {
            break;
        }
        let now = Instant::now();
        if now < next_tick {
            thread::sleep(next_tick - now);
        }
        next_tick += TICK;
        // A frame that arrived during the sleep carries an older timestamp than the hold resend
        // below would get, so it goes out first.
        drain_pending_frames(&shared, &mut encoder, &queues, &mut video_started, &mut last_frame, &mut last_frame_at, &mut last_sent);
        // After the first encoder error nothing more is fed: every further send would fail the same way.
        if video_started && !clock.is_paused() && shared.peek_encode_error().is_none() {
            if let (Some(bytes), Some(at)) = (&last_frame, last_frame_at) {
                if at.elapsed() >= TICK {
                    let ts = strictly_increasing_ts(&mut last_sent, clock.now_ticks());
                    if let Err(e) = encoder.send_frame_buffer(bytes, ts) {
                        append_crash_log(&format!("recording: send_frame_buffer (hold) failed: {e}"));
                        shared.set_encode_error(e.to_string());
                        shared.request_stop();
                    }
                    last_frame_at = Some(Instant::now());
                }
            }
            let mut mixed: Option<Vec<i16>> = None;
            for q in &queues {
                let chunk = q.drain(samples_per_tick);
                mixed = Some(match mixed {
                    None => chunk,
                    Some(m) => audio_mix_i16(&m, &chunk),
                });
            }
            if let Some(m) = mixed {
                let mut bytes = Vec::with_capacity(m.len() * 2);
                for s in &m {
                    bytes.extend_from_slice(&s.to_le_bytes());
                }
                if let Err(e) = encoder.send_audio_buffer(&bytes, 0) {
                    append_crash_log(&format!("recording: send_audio_buffer failed: {e}"));
                    shared.set_encode_error(e.to_string());
                    shared.request_stop();
                }
            }
        }
    }
    // Final flush: keep waiting (pop_or_stop, not try_pop) for whatever was still in flight
    // when the capture callback saw the stop signal, as in the plain feeder_loop.
    while let Some(item) = shared.pop_or_stop() {
        let ts = strictly_increasing_ts(&mut last_sent, item.ts);
        if let Err(e) = encoder.send_frame_buffer(&item.bytes, ts) {
            append_crash_log(&format!("recording: send_frame_buffer failed: {e}"));
            shared.set_encode_error(e.to_string());
            break;
        }
    }
    if let Err(e) = encoder.finish() {
        append_crash_log(&format!("recording: encoder finish failed: {e}"));
        shared.set_encode_error(e.to_string());
    }
}

// Runs alongside feeder_loop_with_audio only when both system and mic are captured: drains
// the mic queue to its own ".mic.wav" sidecar instead of mixing it into the encoder's one
// track. Same tick cadence and pause/first-frame gating as the main feeder, so the sidecar
// lines up with the mp4 when the editor muxes them (a few ms of slack around the first
// frame is possible, as in the main feeder).
pub(crate) fn mic_wav_writer_loop(
    queue: Arc<AudioQueueShared>,
    clock: Arc<RecordClock>,
    shared: Arc<RecFeederShared>,
    stop: Arc<AtomicBool>,
    path: PathBuf,
) {
    const TICK: Duration = Duration::from_millis(20);
    let samples_per_tick = (REC_AUDIO_SAMPLE_RATE as usize / 50) * REC_AUDIO_CHANNELS as usize;
    let mut writer = match RecWavWriter::create(&path, REC_AUDIO_SAMPLE_RATE, REC_AUDIO_CHANNELS as u16) {
        Ok(w) => w,
        Err(e) => {
            append_crash_log(&format!("recording: mic sidecar create failed: {e}"));
            // The bar's mic-lost notice reads this flag; the recording itself carries on.
            queue.mark_lost();
            return;
        }
    };
    let mut started = false;
    let mut next_tick = Instant::now() + TICK;
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let now = Instant::now();
        if now < next_tick {
            thread::sleep(next_tick - now);
        }
        next_tick += TICK;
        if !started {
            if shared.has_first_frame() {
                started = true;
                queue.clear();
            } else {
                continue;
            }
        }
        if clock.is_paused() {
            continue;
        }
        let chunk = queue.drain(samples_per_tick);
        if let Err(e) = writer.write_samples(&chunk) {
            append_crash_log(&format!("recording: mic sidecar write failed: {e}"));
            queue.mark_lost();
            break;
        }
    }
    if let Err(e) = writer.finish() {
        append_crash_log(&format!("recording: mic sidecar finish failed: {e}"));
    }
}

pub struct RecSession {
    control: CaptureControl<RecHandler, RecError>,
    clock: Arc<RecordClock>,
    shared: Arc<RecFeederShared>,
    feeder: Option<thread::JoinHandle<()>>,
    audio_stop: Option<Arc<AtomicBool>>,
    audio_threads: Vec<thread::JoinHandle<()>>,
    // Tracked separately from the feeder's mixing queues (which live in its closure) so
    // recording_status can report a mic-specific loss toast and the rec bar's mute buttons have
    // a queue to flip.
    pub mic_queue: Option<Arc<AudioQueueShared>>,
    pub system_queue: Option<Arc<AudioQueueShared>>,
    // Every active audio queue (system and/or mic), cleared on resume: the WASAPI threads keep
    // pushing during a pause (only the feeder's drain is gated), and that audio must not bleed
    // into the post-resume track, since video freezes at the pause instant and resumes there.
    audio_queues: Vec<Arc<AudioQueueShared>>,
    // Only set when both system and mic are captured: the mic's sidecar writer thread and the
    // path it writes to.
    mic_wav_thread: Option<thread::JoinHandle<()>>,
    pub mic_wav_path: Option<PathBuf>,
    pub audio_system: bool,
    pub audio_mic: bool,
    pub output_path: PathBuf,
    // What this session is capturing right now, kept current across a live target switch (see
    // commit_switch_target) so "finish and continue" reopens against the same source without the
    // control bar remembering a target (start_recording is invoked from the composer window).
    pub target: RecTarget,
    pub phase: RecPhase,
    // Wall-clock start of this session, so "finish and continue" can refuse a click in the
    // first few seconds, the riskiest moment for a file switch.
    pub started_at_ms: u64,
    // Set only for a Window recording whose canvas is bigger than the window (see
    // WindowPlacement): the post-capture crop reads the union of placed frames from this once
    // the capture callback is done. None for Monitor/Region and the "window's own size" tier.
    pub used_area: Option<Arc<UsedAreaTracker>>,
    // Stable key naming which canvas this recording used, for the result dialog: "all" (every
    // display), "monitor" or "window" (the window's own size). None for Monitor/Region targets.
    pub canvas_note: Option<&'static str>,
    // The encoder's frame size; with used_area it decides whether the post-capture crop has
    // anything to do (a used area that fills the canvas needs none).
    pub canvas_size: (u32, u32),
    // The placement canvas' top-left (see CanvasCandidate::origin), kept only for a Window
    // recording that used one (canvas_note "all" or "monitor"): a live target switch re-anchors
    // its new window inside this origin instead of re-deriving a canvas. None otherwise.
    canvas_origin: Option<(i32, i32)>,
    // Whether the cursor was drawn, so a target switch builds its replacement capture with the
    // same setting.
    pub show_cursor: bool,
    // Every other setting this session was built with, so "finish and continue" reopens with the
    // same values even if the user changed the settings store in between.
    pub audio_config: AudioConfig,
    pub video_kbps: u32,
    pub video_fps: u32,
    pub canvas_mode: String,
    pub canvas_ceiling: (u32, u32),
    pub disk_warn_mb: f64,
    // Recording-length and file-size safety nets (0 = off), carried through like disk_warn_mb:
    // "finish and continue" reuses the finished part's values.
    pub max_min: f64,
    pub max_mb: f64,
}

// A Window recording's canvas defaults to the union of every display so dragging the window
// across displays never re-scales it. That canvas can be too large (huge per-frame memory,
// or the encoder refusing the size), so candidates are tried in order and the first one
// within the ceiling that build_encoder accepts wins. The stored `recCanvasMode` setting
// (defaults to "all", not in the expert menu) lets a user skip straight to a smaller tier.
struct CanvasCandidate {
    size: (u32, u32),
    origin: (i32, i32),
    // Whether a frame needs live per-frame placement; false only for the final "window's own
    // size" tier, which is a single fixed rectangle.
    placement: bool,
    // Stable key, not display text, sharing its values with `recCanvasMode`
    // ("all"/"monitor"/"window"); the frontend looks up its translated sentence from it.
    note: &'static str,
}
fn window_canvas_candidates(
    hwnd: isize, native_w: u32, native_h: u32, mode: &str, ceiling: (u32, u32),
) -> Vec<CanvasCandidate> {
    let native_origin = external_window_rect(hwnd).map(|(l, t, _, _)| (l, t)).unwrap_or((0, 0));
    let mut out = Vec::new();
    if mode != "window" {
        if mode != "monitor" {
            if let Ok((vx, vy, vw, vh)) = virtual_screen_bounds() {
                if canvas_within_ceiling(vw, vh, ceiling.0, ceiling.1) {
                    out.push(CanvasCandidate { size: (vw, vh), origin: (vx, vy), placement: true, note: "all" });
                }
            }
        }
        if let Some((mx, my, mw, mh)) = monitor_rect_at(native_origin.0, native_origin.1) {
            out.push(CanvasCandidate { size: (mw, mh), origin: (mx, my), placement: true, note: "monitor" });
        }
    }
    out.push(CanvasCandidate {
        size: (native_w, native_h), origin: native_origin, placement: false, note: "window",
    });
    out
}

fn resolve_target_size(target: RecTarget) -> Result<(u32, u32), String> {
    if let Some((_, _, w, h)) = resolve_region_clip(target)? {
        return Ok((w, h));
    }
    match resolve_capture_item(target)? {
        CaptureItem::Monitor(m) => Ok((
            even_dim(m.width().map_err(|e| e.to_string())?),
            even_dim(m.height().map_err(|e| e.to_string())?),
        )),
        CaptureItem::Window(w) => {
            let width = w.width().map_err(|e| e.to_string())?;
            let height = w.height().map_err(|e| e.to_string())?;
            if width <= 0 || height <= 0 {
                return Err("window has no visible size".to_string());
            }
            Ok((even_dim(width as u32), even_dim(height as u32)))
        }
    }
}

// Runs as long as the recording does, checking every DISK_CHECK_SECS whether the destination
// drive is about to run out (a long gap lets a high bitrate fill the drive between two checks).
// The 1-second sleep granularity lets the thread notice a normal stop and exit within a second.
// Also enforces the optional safety nets recMaxMin/recMaxMb (0 = off) through the same
// "set an error, request a stop" channel as the disk check, on the same 1s tick since both
// read a cheap number (elapsed time, one fs::metadata call).
#[allow(clippy::too_many_arguments)]
fn spawn_disk_watch(
    shared: Arc<RecFeederShared>, clock: Arc<RecordClock>, output_path: PathBuf, warn_mb: f64, max_min: f64, max_mb: f64,
    video_kbps: u32, audio_active: bool,
) {
    let spawned = thread::Builder::new().name("rec-disk-watch".into()).spawn(move || {
        let warn_bytes = (warn_mb.max(1.0) * 1024.0 * 1024.0) as u64;
        let critical_bytes = ((warn_mb / 4.0).max(1.0) * 1024.0 * 1024.0) as u64;
        let limits_active = max_min > 0.0 || max_mb > 0.0;
        let mut waited = Duration::ZERO;
        let mut elapsed = Duration::ZERO;
        while !shared.is_stopping() {
            thread::sleep(Duration::from_secs(1));
            // The minute limit counts recorded time: a pause does not use it up.
            elapsed += Duration::from_secs(active_tick_secs(clock.is_paused()));
            waited += Duration::from_secs(1);
            if limits_active {
                let file_len = fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
                if recording_limit_reached(elapsed.as_secs(), file_len, max_min, max_mb) {
                    shared.set_encode_error("max-limit-reached".to_string());
                    shared.request_stop();
                    return;
                }
            }
            if waited < Duration::from_secs(DISK_CHECK_SECS) {
                continue;
            }
            waited = Duration::ZERO;
            if let Ok(free) = free_space_bytes(&output_path) {
                if free < critical_bytes {
                    // Same channel an unrecoverable encoder failure uses (see feeder_loop): the control bar's
                    // poll sees it and stops the recording cleanly before ffmpeg hits ENOSPC mid-write.
                    shared.set_encode_error("low-disk-space".to_string());
                    shared.request_stop();
                    return;
                }
                // The softer, non-stopping notice: fires once, well above the critical threshold.
                if free < warn_bytes {
                    let free_mb = free / (1024 * 1024);
                    let est_min = estimate_recording_seconds_remaining(free, video_kbps, audio_active) / 60;
                    shared.note_disk_low(free_mb, est_min);
                }
            }
        }
    });
    if spawned.is_err() {
        append_crash_log("recording: could not start the disk-space watch thread");
    }
}

// Called right after the key-overlay capture is installed: when the first frame already came in
// before it, the feeder's own rebase found no capture, so the overlay's time zero moves here.
pub fn align_key_overlay_start(session: &RecSession) {
    if let Some(at) = session.shared.first_frame_mono_ms() {
        keylog::rebase_start(at);
    }
}

// Grouped so `start` stays under clippy's argument limit.
pub struct RecStartOptions {
    pub show_cursor: bool,
    pub video_kbps: u32,
    pub video_fps: u32,
    pub canvas_ceiling: (u32, u32),
    pub disk_warn_mb: f64,
    pub max_min: f64,
    pub max_mb: f64,
}

// show_cursor is windows-capture's own capture setting, never a manually drawn cursor. With
// neither audio source active the take is video-only.
pub fn start(
    target: RecTarget, output_path: PathBuf, audio: AudioConfig, canvas_mode: &str, opts: RecStartOptions,
) -> Result<RecSession, String> {
    let result = start_inner(target, output_path.clone(), audio, canvas_mode, opts);
    if result.is_err() {
        discard_start_leftovers(&output_path);
    }
    result
}

// What a refused start leaves behind: the empty output file and, for a take with both audio
// sources, the microphone sidecar its writer thread had begun. The sidecar belongs to this
// unfinished take, so it goes with it.
pub(crate) fn discard_start_leftovers(output_path: &Path) {
    discard_empty_output(output_path);
    let _ = fs::remove_file(output_path.with_extension("mic.wav"));
}

// Everything start_inner has started so far. Dropping the guard sets every stop flag, runs
// the wake action (the same request_stop the normal stop uses, which also ends the feeder's
// wait) and joins the threads, each for a bounded time, so a failed start leaves no feeder
// waiting, no audio thread capturing and no open output file. On success `take` hands the
// threads to the session and the guard is left empty, so its drop does nothing.
pub(crate) struct StartGuard {
    flags: Vec<Arc<AtomicBool>>,
    wake: Option<Box<dyn FnOnce() + Send>>,
    pub(crate) audio_threads: Vec<thread::JoinHandle<()>>,
    pub(crate) feeder: Option<thread::JoinHandle<()>>,
    pub(crate) mic_wav_thread: Option<thread::JoinHandle<()>>,
}

pub(crate) type StartParts = (Vec<thread::JoinHandle<()>>, Option<thread::JoinHandle<()>>, Option<thread::JoinHandle<()>>);

impl StartGuard {
    pub(crate) fn new(wake: impl FnOnce() + Send + 'static) -> Self {
        Self { flags: Vec::new(), wake: Some(Box::new(wake)), audio_threads: Vec::new(), feeder: None, mic_wav_thread: None }
    }

    pub(crate) fn add_flag(&mut self, flag: Arc<AtomicBool>) {
        self.flags.push(flag);
    }

    // Success: the session owns the threads from here on, nothing is left to undo.
    pub(crate) fn take(&mut self) -> StartParts {
        self.flags.clear();
        self.wake = None;
        (std::mem::take(&mut self.audio_threads), self.feeder.take(), self.mic_wav_thread.take())
    }
}

// Waits for a thread to end, but never longer than `limit`: a thread that does not
// react is left behind rather than hanging the failed start.
fn join_bounded(handle: thread::JoinHandle<()>, limit: Duration) {
    let deadline = Instant::now() + limit;
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            append_crash_log("recording: a thread did not end after a failed start");
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let _ = handle.join();
}

impl Drop for StartGuard {
    fn drop(&mut self) {
        for flag in &self.flags {
            flag.store(true, Ordering::Relaxed);
        }
        if let Some(wake) = self.wake.take() {
            wake();
        }
        let limit = Duration::from_secs(5);
        for t in self.audio_threads.drain(..) {
            join_bounded(t, limit);
        }
        if let Some(t) = self.mic_wav_thread.take() {
            join_bounded(t, limit);
        }
        if let Some(t) = self.feeder.take() {
            join_bounded(t, limit);
        }
    }
}

fn start_inner(
    target: RecTarget, output_path: PathBuf, audio: AudioConfig, canvas_mode: &str, opts: RecStartOptions,
) -> Result<RecSession, String> {
    let RecStartOptions { show_cursor, video_kbps, video_fps, canvas_ceiling, disk_warn_mb, max_min, max_mb } = opts;
    let (native_w, native_h) = resolve_target_size(target)?;
    let audio_active = audio.system || audio.mic;
    // With both sources active the encoder gets one source (system, when present) and the
    // second is written to its own sidecar file instead of being mixed in.
    let both_active = audio.system && audio.mic;
    // A Window target tries its canvas candidates in order (every display, the display it started
    // on, its own size); Monitor and Region have exactly one. Bitrate is scaled down for a
    // placement candidate whose canvas is bigger than the window, so a small window on a big
    // canvas does not spend its budget on black pixels.
    let candidates = match target {
        RecTarget::Window(hwnd) => window_canvas_candidates(hwnd, native_w, native_h, canvas_mode, canvas_ceiling),
        _ => vec![CanvasCandidate { size: (native_w, native_h), origin: (0, 0), placement: false, note: "" }],
    };
    let mut built: Option<(VideoEncoder, CanvasCandidate)> = None;
    let mut last_err = String::new();
    for mut cand in candidates {
        cand.size = (even_dim(cand.size.0), even_dim(cand.size.1));
        let kbps = if cand.placement {
            scale_bitrate_to_used_area(video_kbps, native_w, native_h, cand.size.0, cand.size.1)
        } else {
            video_kbps
        };
        match build_encoder(cand.size.0, cand.size.1, &output_path, audio_active.then_some(audio.kbps), kbps, video_fps) {
            Ok(enc) => {
                built = Some((enc, cand));
                break;
            }
            Err(e) => last_err = e.to_string(),
        }
    }
    let (encoder, canvas) = built.ok_or_else(|| if last_err.is_empty() { "could not start the encoder".to_string() } else { last_err })?;
    let (width, height) = canvas.size;
    let used_area = canvas.placement.then(|| Arc::new(UsedAreaTracker::new()));
    let canvas_note = if canvas.note.is_empty() { None } else { Some(canvas.note) };
    let clock = Arc::new(RecordClock::new());
    let shared = Arc::new(RecFeederShared::new());
    spawn_disk_watch(shared.clone(), clock.clone(), output_path.clone(), disk_warn_mb, max_min, max_mb, video_kbps, audio_active);
    let mut guard = StartGuard::new({
        let shared = shared.clone();
        move || shared.request_stop()
    });

    let mut audio_queues: Vec<Arc<AudioQueueShared>> = Vec::new();
    let mut mic_queue: Option<Arc<AudioQueueShared>> = None;
    let mut system_queue: Option<Arc<AudioQueueShared>> = None;
    let audio_stop = if audio_active { Some(Arc::new(AtomicBool::new(false))) } else { None };
    if let Some(stop) = &audio_stop {
        guard.add_flag(stop.clone());
    }
    if audio.system {
        let q = Arc::new(AudioQueueShared::new());
        guard.audio_threads.push(super::audio::spawn_capture(
            "system", eRender, true, String::new(), q.clone(), audio_stop.clone().unwrap(),
        ).map_err(|e| e.to_string())?);
        audio_queues.push(q.clone());
        system_queue = Some(q);
    }
    if audio.mic {
        let q = Arc::new(AudioQueueShared::new());
        guard.audio_threads.push(super::audio::spawn_capture(
            "mic", eCapture, false, audio.mic_device_id.clone(), q.clone(), audio_stop.clone().unwrap(),
        ).map_err(|e| e.to_string())?);
        audio_queues.push(q.clone());
        mic_queue = Some(q);
    }
    // Queues the encoder's feeder drains: all except a mic alongside an already-present system
    // track (both_active), which goes to the sidecar writer below. System is pushed first, so
    // take(1) is that queue.
    let encoder_queues: Vec<Arc<AudioQueueShared>> =
        if both_active { audio_queues.iter().take(1).cloned().collect() } else { audio_queues.clone() };

    guard.feeder = Some(if audio_active {
        let shared2 = shared.clone();
        let clock2 = clock.clone();
        thread::Builder::new()
            .name("rec-feeder".into())
            .spawn(move || feeder_loop_with_audio(shared2, encoder, clock2, encoder_queues))
            .map_err(|e| e.to_string())?
    } else {
        let shared2 = shared.clone();
        thread::Builder::new()
            .name("rec-feeder".into())
            .spawn(move || feeder_loop(shared2, encoder))
            .map_err(|e| e.to_string())?
    });

    let mic_wav_path = both_active.then(|| output_path.with_extension("mic.wav"));
    guard.mic_wav_thread = if let (Some(path), Some(q)) = (mic_wav_path.clone(), mic_queue.clone()) {
        let stop = audio_stop.clone().unwrap();
        let clock2 = clock.clone();
        let shared2 = shared.clone();
        Some(
            thread::Builder::new()
                .name("rec-mic-wav".into())
                .spawn(move || mic_wav_writer_loop(q, clock2, shared2, stop, path))
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };

    let cursor = if show_cursor { CursorCaptureSettings::WithCursor } else { CursorCaptureSettings::WithoutCursor };
    let crop = resolve_region_clip(target)?;
    let border = border_setting_for_target(target);
    let placement_flags = match (canvas.placement, target) {
        (true, RecTarget::Window(hwnd)) => Some((hwnd, canvas.origin, used_area.clone().unwrap())),
        _ => None,
    };
    let flags = (clock.clone(), shared.clone(), crop, (width, height), placement_flags, None);
    let control = match resolve_capture_item(target)? {
        CaptureItem::Monitor(m) => {
            let settings = Settings::new(
                m, cursor, border, SecondaryWindowSettings::Default,
                MinimumUpdateIntervalSettings::Default, DirtyRegionSettings::Default, ColorFormat::Bgra8, flags,
            );
            RecHandler::start_free_threaded(settings).map_err(|e| e.to_string())?
        }
        CaptureItem::Window(w) => {
            let settings = Settings::new(
                w, cursor, border, SecondaryWindowSettings::Default,
                MinimumUpdateIntervalSettings::Default, DirtyRegionSettings::Default, ColorFormat::Bgra8, flags,
            );
            RecHandler::start_free_threaded(settings).map_err(|e| e.to_string())?
        }
    };

    let canvas_origin = canvas.placement.then_some(canvas.origin);
    let (audio_threads, feeder, mic_wav_thread) = guard.take();
    Ok(RecSession {
        control, clock, shared, feeder, audio_stop, audio_threads, mic_queue, system_queue,
        audio_queues, mic_wav_thread, mic_wav_path,
        audio_system: audio.system, audio_mic: audio.mic,
        output_path, target, phase: RecPhase::Recording, started_at_ms: now_ms(),
        used_area, canvas_note, canvas_size: (width, height),
        canvas_origin, show_cursor,
        video_kbps, video_fps, canvas_ceiling, disk_warn_mb, max_min, max_mb,
        canvas_mode: canvas_mode.to_string(),
        audio_config: audio,
    })
}

pub fn pause(session: &RecSession) {
    session.clock.pause();
}
pub fn resume(session: &RecSession) {
    session.clock.resume();
    // Discard what the WASAPI threads pushed while paused (see the audio_queues field).
    for q in &session.audio_queues {
        q.clear();
    }
}

// Active-recording time in whole milliseconds, for the control bar's live timer. The clock
// counts 100ns ticks (what send_frame_buffer wants), so this is display-only.
pub fn elapsed_ms(session: &RecSession) -> i64 {
    session.clock.now_ticks() / 10_000
}

// Whether this session can offer a live target switch: only a window recording on one of the
// placement canvases (see CanvasCandidate). Monitor/Region targets and the "window's own
// size" tier have no spare canvas room to re-anchor a different window into.
pub fn can_switch_target(session: &RecSession) -> bool {
    session.canvas_origin.is_some()
}

// Whether a window fits the already built canvas without rebuilding the encoder: "all" fits
// anything within the virtual screen, "monitor" only a window on that same display, and the
// "window" tier never offers a switch. Pure, independent of any capture handle, so it is
// unit-testable on any platform.
pub(crate) fn target_fits_canvas(
    canvas_note: Option<&str>, canvas_origin: (i32, i32), canvas_size: (u32, u32),
    new_monitor: Option<(i32, i32, u32, u32)>,
) -> bool {
    match canvas_note {
        Some("all") => true,
        Some("monitor") => new_monitor.map(|(l, t, w, h)| (l, t, even_dim(w), even_dim(h))) == Some((canvas_origin.0, canvas_origin.1, canvas_size.0, canvas_size.1)),
        _ => false,
    }
}

// Swaps a Window recording's capture source to a different window while keeping the encoder,
// canvas, clock and output file. A second WGC session starts into the same canvas origin and
// pushes into the same feeder queue; only once it has delivered its first frame is the old
// one torn down, so the seam drops no frame and the shared clock keeps timestamps
// continuous. A failure at any step leaves the old session untouched.
//
// Split in three so the (up to 2 s) first-frame wait runs without the recording-state lock:
// begin (validate, start the second capture), wait, commit (swap the handles under the lock).
pub struct PendingSwitch {
    control: CaptureControl<RecHandler, RecError>,
    first_frame: Arc<AtomicBool>,
    // The session the switch was started for.
    shared: Arc<RecFeederShared>,
    // The recording's clock: a paused one delivers no frames, so the wait below must not count
    // paused time against the first-frame budget.
    clock: Arc<RecordClock>,
    new_hwnd: isize,
}

pub fn begin_switch_target(session: &RecSession, new_hwnd: isize, new_monitor: Option<(i32, i32, u32, u32)>) -> Result<PendingSwitch, String> {
    let canvas_origin = session
        .canvas_origin
        .ok_or_else(|| "recording-target-switch-not-supported".to_string())?;
    if !target_fits_canvas(session.canvas_note, canvas_origin, session.canvas_size, new_monitor) {
        return Err("recording-target-switch-different-monitor".to_string());
    }
    let used_area = session
        .used_area
        .clone()
        .ok_or_else(|| "recording-target-switch-not-supported".to_string())?;
    let w = Window::from_raw_hwnd(new_hwnd as *mut std::ffi::c_void);
    if !w.is_valid() {
        return Err("recording-target-switch-invalid-window".to_string());
    }
    let cursor = if session.show_cursor { CursorCaptureSettings::WithCursor } else { CursorCaptureSettings::WithoutCursor };
    let first_frame = Arc::new(AtomicBool::new(false));
    let flags = (
        session.clock.clone(), session.shared.clone(), None, session.canvas_size,
        Some((new_hwnd, canvas_origin, used_area)), Some(first_frame.clone()),
    );
    let settings = Settings::new(
        w, cursor, DrawBorderSettings::Default, SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default, DirtyRegionSettings::Default, ColorFormat::Bgra8, flags,
    );
    let control = RecHandler::start_free_threaded(settings).map_err(|e| e.to_string())?;
    Ok(PendingSwitch { control, first_frame, shared: session.shared.clone(), clock: session.clock.clone(), new_hwnd })
}

// What is left of the first-frame budget after one 10 ms tick; a paused tick costs nothing.
pub(crate) fn switch_wait_budget_after_tick(budget_ms: u32, paused: bool) -> u32 {
    if paused {
        budget_ms
    } else {
        budget_ms.saturating_sub(10)
    }
}

pub fn wait_switch_first_frame(pending: PendingSwitch) -> Result<PendingSwitch, String> {
    // A real frame typically lands within a couple hundred ms; 2s leaves room for a slow first
    // WGC frame without hanging the caller long on a failed switch. Only running time counts: a
    // pause during the wait must not become a timeout, and a recording being stopped ends it.
    let mut budget_ms: u32 = 2000;
    while !pending.first_frame.load(Ordering::Acquire) {
        if pending.shared.is_stopping() {
            let _ = pending.control.stop();
            return Err("no active recording".to_string());
        }
        if budget_ms == 0 {
            let _ = pending.control.stop();
            return Err("recording-target-switch-timeout".to_string());
        }
        thread::sleep(Duration::from_millis(10));
        budget_ms = switch_wait_budget_after_tick(budget_ms, pending.clock.is_paused());
    }
    Ok(pending)
}

/// Swaps the proven new capture in. If the recording ended or moved on to another session
/// while the lock was released, the new capture is stopped again and the switch fails.
pub fn commit_switch_target(session: Option<&mut RecSession>, pending: PendingSwitch) -> Result<(), String> {
    let session = match session {
        Some(s) if Arc::ptr_eq(&s.shared, &pending.shared) => s,
        _ => {
            let _ = pending.control.stop();
            return Err("no active recording".to_string());
        }
    };
    let old_control = std::mem::replace(&mut session.control, pending.control);
    if let Err(e) = old_control.stop() {
        // The new source is already live, so a failed stop of the old handle is only worth a log
        // line, not a failed switch.
        append_crash_log(&format!("recording: old capture control stop failed after a target switch: {e}"));
    }
    // A later "finish and continue" reopens against what the session is capturing now.
    session.target = RecTarget::Window(pending.new_hwnd);
    Ok(())
}

// Shared tail of stop/cancel: drain the queue and finalise the encoder, then tear down the
// capture thread. Finalising after stop() runs against a WM_QUIT'd capture pipeline and
// yields an MP4 with no closed moov atom (unplayable).
// Cannot fail: every failure inside is folded into the returned encode error.
fn finish_and_stop(mut session: RecSession) -> (u64, Option<String>) {
    // Both sides stop producing at about the same instant (audio_stop cuts the WASAPI threads,
    // is_stopping cuts new video frames) before either is joined or drained, so neither track
    // keeps growing while the other has gone silent.
    if let Some(stop) = &session.audio_stop {
        stop.store(true, Ordering::Relaxed);
    }
    session.shared.request_stop();
    for t in session.audio_threads.drain(..) {
        let _ = t.join();
    }
    if let Some(t) = session.mic_wav_thread.take() {
        let _ = t.join();
    }
    if let Some(feeder) = session.feeder.take() {
        let _ = feeder.join();
    }
    let dropped = session.shared.dropped.load(Ordering::Relaxed);
    let mut encode_error = session.shared.peek_encode_error();
    // The encoder is already finalised here (finish() ran in the feeder thread's tail), so a
    // failure is only the WGC handle refusing to stop cleanly and does not undo the written
    // file. It is surfaced as a non-fatal encode error: a hard Err would drop the session
    // without a "recording-captured" event and strand a finished recording with no result
    // dialog.
    if let Err(e) = session.control.stop() {
        append_crash_log(&format!("recording: capture control stop failed: {e}"));
        encode_error = encode_error.or_else(|| Some(e.to_string()));
    }
    (dropped, encode_error)
}

// path, dropped-frame count, encode error, mic sidecar path (only when both system and mic
// were captured; the editor merges it into a second AAC track on first open), canvas note,
// used-area crop rect (canvas-local, None unless a placement recording), canvas size, and
// the timestamp of the first frame the resize safety net had to shrink (None if never).
pub type StopResult = (PathBuf, u64, Option<String>, Option<PathBuf>, Option<&'static str>, Option<(u32, u32, u32, u32)>, (u32, u32), Option<u64>);

pub fn stop(session: RecSession) -> StopResult {
    let path = session.output_path.clone();
    let mic_wav_path = session.mic_wav_path.clone();
    let canvas_note = session.canvas_note;
    let used_area = session.used_area.clone();
    let canvas_size = session.canvas_size;
    let shared = session.shared.clone();
    let (dropped, encode_error) = finish_and_stop(session);
    // Safe to read now: the capture thread (sole writer of used_area and the resize-crop
    // tracker) is joined once finish_and_stop returns.
    let crop_rect = used_area.and_then(|u| u.get());
    let resize_crop_at_ms = shared.resize_crop_at_ms();
    // A sidecar that was never created (its writer failed at start) is not handed on: the
    // editor would try to mux a file that is not there.
    let mic_wav_path = mic_wav_path.filter(|p| p.is_file());
    (path, dropped, encode_error, mic_wav_path, canvas_note, crop_rect, canvas_size, resize_crop_at_ms)
}

// Live-poll accessor for the control bar, so it notices an unrecoverable encoder failure
// (e.g. disk full) while nominally recording and stops itself instead of looking like it is
// still capturing.
pub fn encode_error(session: &RecSession) -> Option<String> {
    session.shared.peek_encode_error()
}

// Same live-poll shape as encode_error, but consuming: the low-disk notice fires once and
// never stops the recording.
pub fn take_disk_low(session: &RecSession) -> Option<(u64, u64)> {
    session.shared.take_disk_low()
}

// VideoEncoder::new() opens the output file eagerly, so a clean cancel still finalises it
// through finish_and_stop (one shutdown sequence) and then moves it to the holding area for
// discarded recordings.
pub fn cancel(session: RecSession) {
    let path = session.output_path.clone();
    let mic_wav_path = session.mic_wav_path.clone();
    finish_and_stop(session);
    crate::media_files::discard_or_remove_take(&path);
    if let Some(w) = mic_wav_path {
        let _ = std::fs::remove_file(&w);
    }
}

pub fn new_output_path(recordings_dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(recordings_dir).map_err(|e| e.to_string())?;
    let st = unsafe { GetLocalTime() };
    let name = rec_filename(
        st.wYear as i32, st.wMonth as u32, st.wDay as u32, st.wHour as u32, st.wMinute as u32, st.wSecond as u32,
    );
    Ok(free_rec_path(recordings_dir, &name))
}

// Every continuation part keeps the first file's name stem and timestamp with a numbered
// suffix, so one "finish and continue" chain groups together in a folder listing. Numbering
// starts at the second file; the first part carries no suffix.
pub fn continuation_output_path(base: &Path, part_number: u32) -> PathBuf {
    let stem = base.file_stem().and_then(|s| s.to_str()).unwrap_or("rec");
    let ext = base.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    base.with_file_name(format!("{stem}_part{part_number}.{ext}"))
}
