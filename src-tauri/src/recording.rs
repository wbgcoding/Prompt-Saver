use super::{
    append_crash_log, audio_mix_i16, crop_and_flip_rows_bottom_up, free_space_bytes, letterbox_frame_top_down,
    scale_to_fit_frame_top_down, rec_filename, now_ms,
    AudioQueueShared, RecWavWriter,
    RecFeederShared, RecFrameItem, RecPhase, RecordClock, REC_AUDIO_CHANNELS, REC_AUDIO_SAMPLE_RATE,
    canvas_within_ceiling, external_window_rect, monitor_rect_at, place_frame_in_canvas, visible_canvas_rect,
    scale_bitrate_to_used_area, virtual_screen_bounds, recording_limit_reached,
    estimate_recording_seconds_remaining, active_tick_secs, UsedAreaTracker,
};
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
// Re-exported so the top-level test module can assert on
// border_setting_for_target's result without its own separate import.
#[cfg(test)]
pub(crate) use windows_capture::settings::DrawBorderSettings as BorderSetting;

pub type RecError = Box<dyn std::error::Error + Send + Sync>;

// Target selected for recording, identified the same way the snip overlay
// already identifies windows/monitors (raw HMONITOR/HWND). Region is the
// third target kind: WGC has no region capture source, so
// the whole `monitor` is captured and cropped to (x, y, w, h) -- relative to
// that monitor's own top-left -- in the encoder pipeline instead.
#[derive(Clone, Copy)]
pub enum RecTarget {
    Monitor(isize),
    Window(isize),
    Region { monitor: isize, x: i32, y: i32, w: u32, h: u32 },
}

// What RecTarget actually captures from WGC's point of view: either kind
// needs its own Settings::new call (different item types), but Region shares
// the Monitor path -- only the crop differs.
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

// The crop rectangle to apply to each captured frame, or None to keep the
// whole frame (Monitor/Window targets).
fn resolve_crop(target: RecTarget) -> Option<(u32, u32, u32, u32)> {
    match target {
        RecTarget::Region { x, y, w, h, .. } => Some((x.max(0) as u32, y.max(0) as u32, even_dim(w), even_dim(h))),
        _ => None,
    }
}

// The H.264 Media Foundation encoder rejects an odd width or height
// (0xC00DA412), so every size that reaches it is rounded DOWN to even
// (the picture then stays inside the captured area) with a floor of 2
// so a 1-px edge never becomes an empty size.
pub(crate) fn even_dim(n: u32) -> u32 {
    (n & !1).max(2)
}

// Removes the output file a failed start left behind: the encoder creates
// it before it can reject the type, so a refused start leaves a 0-byte
// file. A file with content is never touched. Returns whether it deleted.
pub(crate) fn discard_empty_output(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(m) if m.is_file() && m.len() == 0 => fs::remove_file(path).is_ok(),
        _ => false,
    }
}

// A Region target crops a borderless whole-monitor capture down to the
// requested area (see resolve_crop), so the OS capture border would ring
// the entire monitor instead of the recorded area -- switched off for
// Region and replaced by the outline window below. Monitor and Window
// targets are untouched and keep today's OS-drawn border.
pub(crate) fn border_setting_for_target(target: RecTarget) -> DrawBorderSettings {
    match target {
        RecTarget::Region { .. } => DrawBorderSettings::WithoutBorder,
        RecTarget::Monitor(_) | RecTarget::Window(_) => DrawBorderSettings::Default,
    }
}

// Pure: the outline window's rect in absolute (physical) screen pixels
// for a region capture, given that region's own monitor's top-left --
// mirrors resolve_crop's monitor-relative (x, y) exactly, just without
// clamping it away against the monitor's own origin.
pub(crate) fn region_outline_rect(monitor_origin: (i32, i32), x: i32, y: i32, w: u32, h: u32) -> (i32, i32, u32, u32) {
    (monitor_origin.0 + x.max(0), monitor_origin.1 + y.max(0), w, h)
}

// The outline window's rect for this target, or None for Monitor/Window
// targets, which keep the OS border and need no outline window at all.
pub(crate) fn outline_rect_for_target(target: RecTarget) -> Option<(i32, i32, u32, u32)> {
    match target {
        RecTarget::Region { monitor, x, y, w, h } => {
            let origin = super::monitor_bounds(monitor).map(|(l, t, _, _)| (l, t)).unwrap_or((0, 0));
            Some(region_outline_rect(origin, x, y, w, h))
        }
        _ => None,
    }
}

// A Window recording whose canvas ended up bigger than the window
// itself (the every-display or the current-display fallback tier): each
// frame is placed at the window's own LIVE screen position inside that
// fixed canvas instead of at (0, 0), and the union of every placement is
// tracked for the post-capture crop. Absent for Monitor/Region targets
// and for the "window's own size" fallback tier, both of which keep
// today's original single-fixed-rectangle behaviour untouched.
struct WindowPlacement {
    hwnd: isize,
    // The canvas' own top-left in the same coordinate space GetWindowRect
    // reports (virtual-screen pixels) -- subtracted from the window's live
    // position to get its offset inside the canvas.
    canvas_origin: (i32, i32),
    used_area: Arc<UsedAreaTracker>,
}

pub(crate) struct RecHandler {
    clock: Arc<RecordClock>,
    shared: Arc<RecFeederShared>,
    crop: Option<(u32, u32, u32, u32)>,
    // The encoder's fixed output size, chosen once from the target at
    // start(). A Window target can be resized live by the user while WGC
    // keeps delivering frames at the window's current size -- without this,
    // a resize mid-recording changes the byte-buffer size on the very next
    // frame and send_frame_buffer rejects it, silently ending capture.
    // Monitor/Region targets never change size mid-capture, so this only
    // ever bites Window.
    enc_size: (u32, u32),
    placement: Option<WindowPlacement>,
    // Set only on the handler a live target switch just started --
    // flipped once this handler's own first frame is pushed, so the
    // switch can wait for it before closing the OLD handler and losing
    // nothing at the seam. None for every ordinary handler.
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
            // Place this window's own frame at its live screen
            // position inside the fixed (all-displays or current-display)
            // canvas. If the window's rect cannot be read this instant
            // (e.g. it just closed), keep the last frame's slot rather
            // than dropping the frame outright.
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
            // Live resize: re-anchor the frame into the encoder's original,
            // fixed canvas instead of passing through the now-mismatched
            // size. Grown past the canvas in either dimension -> scale the
            // whole picture down to fit (a plain crop lost whatever
            // fell outside it); shrunk in both -> the old black-pad, since
            // a smaller frame already shows everything on its own.
            let (ew, eh) = (self.enc_size.0 as usize, self.enc_size.1 as usize);
            let (padded, padded_stride) = if width > ew || height > eh {
                self.shared.mark_resize_crop(width as u32, height as u32, ts);
                scale_to_fit_frame_top_down(fb.as_raw_buffer(), width, height, stride, ew, eh)
            } else {
                letterbox_frame_top_down(fb.as_raw_buffer(), width, height, stride, ew, eh)
            };
            crop_and_flip_rows_bottom_up(&padded, ew, eh, padded_stride, 0, 0, ew, eh)
        } else {
            // An odd-sized frame whose even-rounded size is the encoder size
            // drops its last odd row/column here.
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

// audio_kbps: None disables the encoder's audio track entirely (no active
// source); Some(kbps) enables it at the fixed target format (48kHz/stereo/
// 16-bit AAC), matching what audio_convert_to_target and the feeder's
// audio tick already produce.
fn build_encoder(
    width: u32, height: u32, path: &Path, audio_kbps: Option<u32>, video_kbps: u32, video_fps: u32,
) -> Result<VideoEncoder, RecError> {
    // VideoSettingsBuilder defaults to HEVC -- the app always ships MP4/H.264,
    // so the sub type is set explicitly rather than trusting the default.
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

// Config for the two optional audio sources. Neither active ->
// start() takes the exact original video-only path (build_encoder(...,
// None), plain feeder_loop) with zero behaviour change from 191/238.
#[derive(Clone)]
pub struct AudioConfig {
    pub system: bool,
    pub mic: bool,
    pub mic_device_id: String,
    pub kbps: u32,
}

fn feeder_loop(shared: Arc<RecFeederShared>, mut encoder: VideoEncoder) {
    // During a live target switch the old and the new capture push for a moment, each
    // stamping its frame before its own copy work: the queue can hold an older stamp
    // behind a newer one, which the encoder must never see.
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
            shared.mark_first_frame();
            for q in queues {
                q.clear();
            }
        }
        let ts = strictly_increasing_ts(last_sent, item.ts);
        if let Err(e) = encoder.send_frame_buffer(&item.bytes, ts) {
            append_crash_log(&format!("recording: send_frame_buffer failed: {e}"));
            shared.set_encode_error(e.to_string());
            shared.request_stop(); // further frames would just fail the same way
        }
        *last_frame = Some(item.bytes);
        *last_frame_at = Some(Instant::now());
    }
}

// Audio-enabled variant: a fixed 20ms tick drains the active audio
// queue(s) (mixing when both system+mic are on) and feeds the encoder's
// audio track, gated by the same RecordClock pause state the video path
// already uses -- so pausing needs no extra sync. Video frames are
// drained non-blockingly between ticks rather than waited on, since the
// tick's own pacing is what keeps the encoder's internal audio clock in
// step with real wall-clock time (its clock is derived purely from
// cumulative samples sent, see send_audio_buffer's docs).
//
// Ticking does not start until the FIRST video frame has arrived: WGC
// takes a real, measured few hundred ms after the capture session starts
// before its first frame shows up, and video's own "time zero" is that
// first frame's timestamp (send_frame_buffer self-normalizes on its
// first call). Ticking from the feeder thread's own start instead would
// give audio a head start no video content covers, so every real
// recording measured the audio track running ~0.3s longer than the
// video track. Once the first frame lands, anything already captured
// into the audio queues (from that same startup gap) is discarded so the
// first real tick's content matches what the video's own zero covers.
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
    // WGC only calls on_frame_arrived when the captured surface actually
    // changes -- during a static/idle screen it can go over a second
    // with no new frame at all. Measured on real hardware: the video
    // track's own declared duration (last frame's PTS minus the first
    // frame's) then falls up to ~1s behind real elapsed time, which both
    // truncates the visible recording and blows the audio/video sync
    // budget. Fix: hold and resend the last frame with a fresh timestamp
    // once per tick whenever nothing new has arrived, so video's own PTS
    // never trails real time by more than one tick.
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
        // A frame that arrived during the sleep carries an older timestamp than
        // the hold resend below would get; it goes out first.
        drain_pending_frames(&shared, &mut encoder, &queues, &mut video_started, &mut last_frame, &mut last_frame_at, &mut last_sent);
        if video_started && !clock.is_paused() {
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
    // Final flush: keep waiting (pop_or_stop, not try_pop) for whatever
    // was still in flight when the capture callback saw the stop signal,
    // same tail behaviour as the plain feeder_loop above.
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

// Runs alongside feeder_loop_with_audio only when both system and mic
// are captured -- drains the mic queue straight to its own ".mic.wav"
// sidecar instead of mixing it into the encoder's one track. Same tick
// cadence and the same pause/first-frame gating as the main feeder so the
// sidecar's timeline lines up with the mp4 when the editor muxes them
// together (a few ms of slack is possible around the first-frame instant,
// since this thread only notices it on its own next tick -- same order of
// magnitude as the startup slop the main feeder already tolerates).
fn mic_wav_writer_loop(
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
    // Tracked separately from the feeder's own mixing queues (which live
    // only in the feeder thread's closure) so recording_status can report
    // a mic-specific loss toast without guessing which queue is which, and
    // so the rec bar's mute buttons have a queue to flip regardless
    // of where it landed in the feeder's own encoder_queues/sidecar split.
    pub mic_queue: Option<Arc<AudioQueueShared>>,
    pub system_queue: Option<Arc<AudioQueueShared>>,
    // Every active audio queue (system and/or mic) -- cleared on resume so
    // audio captured live during a pause (the WASAPI threads never stop
    // pushing; only the feeder's drain is gated on the pause state) does
    // not bleed into the post-resume track out of sync with video, which
    // freezes at the pause instant and resumes exactly where it left off.
    audio_queues: Vec<Arc<AudioQueueShared>>,
    // Only set when both system and mic are captured -- the mic's own
    // sidecar writer thread and the path it is writing to.
    mic_wav_thread: Option<thread::JoinHandle<()>>,
    pub mic_wav_path: Option<PathBuf>,
    pub audio_system: bool,
    pub audio_mic: bool,
    pub output_path: PathBuf,
    // What this session is actually capturing right now -- kept up to
    // date across a live target switch (see commit_switch_target) so "finish and
    // continue" can reopen against the SAME source without asking the
    // control bar to remember a target it was never given in the first
    // place (start_recording is invoked from the composer window, not
    // from the bar).
    pub target: RecTarget,
    pub phase: RecPhase,
    // Wall-clock time this session actually started, so "finish and
    // continue" can refuse a click inside the first few seconds -- the
    // riskiest moment for a file switch, when the encoder has barely
    // begun.
    pub started_at_ms: u64,
    // Set only for a Window recording whose canvas ended up bigger
    // than the window (see WindowPlacement) -- the post-capture crop step
    // reads the union of every placed frame from this once the capture
    // callback is done writing to it. None for Monitor/Region targets and
    // for the "window's own size" fallback tier.
    pub used_area: Option<Arc<UsedAreaTracker>>,
    // Stable key naming which canvas this recording actually used, for
    // the result dialog -- e.g. "all" (every display) or "window"
    // (fell back to the window's own size). None for Monitor/Region
    // targets.
    pub canvas_note: Option<&'static str>,
    // The encoder's actual frame size -- needed alongside used_area to
    // decide whether the post-capture crop step has anything to do at
    // all (a used area that already fills the canvas needs no crop).
    pub canvas_size: (u32, u32),
    // The placement canvas' own top-left (see CanvasCandidate::origin),
    // kept only for a Window recording that actually used one (canvas_note
    // "all" or "monitor") -- a live target switch re-anchors its new
    // window inside this SAME origin instead of re-deriving a canvas.
    // None for Monitor/Region targets and for the "window's own size"
    // fallback tier, both of which have nothing a switch could reuse.
    canvas_origin: Option<(i32, i32)>,
    // Whether the cursor was drawn into the picture -- kept so a target
    // switch can build its replacement capture with the same setting
    // rather than silently reverting to the default.
    pub show_cursor: bool,
    // Every other setting this session was actually built with, kept so
    // "finish and continue" reopens with the EXACT same values -- not
    // whatever the settings store happens to hold at that later moment,
    // which the user could have changed in between.
    pub audio_config: AudioConfig,
    pub video_kbps: u32,
    pub video_fps: u32,
    pub canvas_mode: String,
    pub canvas_ceiling: (u32, u32),
    pub disk_warn_mb: f64,
    // Recording-length and file-size safety nets (0 = off, matching
    // disk_warn_mb's own carry-through: "finish and continue" reuses the
    // just-finished part's values, never re-reads the store).
    pub max_min: f64,
    pub max_mb: f64,
}

// A Window recording's canvas defaults to the union of every display
// on purpose, so dragging the window across displays never
// re-scales it -- but that canvas can be too large to use (a huge memory
// cost per frame, or the encoder simply refusing the size), so candidates
// are tried in order and the first one that is both within the ceiling AND
// actually accepted by `build_encoder` wins. `recCanvasMode` (a stored
// setting the expert menu does not expose yet, defaulting to "all") lets a
// user who already knows they want the smaller, error-prone-on-drag
// canvas skip straight past the tiers they do not want.
struct CanvasCandidate {
    size: (u32, u32),
    origin: (i32, i32),
    // Whether a frame needs live per-frame placement at all -- false only
    // for the final "window's own size" tier, which is byte-for-byte
    // today's original single-fixed-rectangle behaviour.
    placement: bool,
    // Stable key, not display text -- shares its three values with
    // `recCanvasMode` ("all"/"monitor"/"window") since a note IS just
    // "which mode this recording actually ended up using". The frontend
    // looks its own translated sentence up from this key.
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
    if let RecTarget::Region { w, h, .. } = target {
        if w == 0 || h == 0 {
            return Err("region has no size".to_string());
        }
        return Ok((even_dim(w), even_dim(h)));
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

// Runs for as long as the recording does, checking roughly once a minute
// whether the destination drive is about to run out from under it. The
// 1-second sleep granularity (rather than one 60s sleep) is what lets this
// thread notice a normal stop and exit on its own within a second, instead
// of a caller having to join a thread that might otherwise be mid-sleep for
// up to a minute.
// Also enforces the two optional user-set safety nets (recMaxMin/recMaxMb,
// 0 = off): same clean "set an error, request a stop" channel as the
// disk-space check below, checked on the same 1s tick since both are just
// reading an already-cheap number (elapsed time, one fs::metadata call).
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
            if waited < Duration::from_secs(60) {
                continue;
            }
            waited = Duration::ZERO;
            if let Ok(free) = free_space_bytes(&output_path) {
                if free < critical_bytes {
                    // Same channel an unrecoverable encoder failure already uses (see
                    // feeder_loop above) -- the control bar's existing poll notices
                    // this and stops the recording cleanly, before ffmpeg itself would
                    // have hit ENOSPC mid-write.
                    shared.set_encode_error("low-disk-space".to_string());
                    shared.request_stop();
                    return;
                }
                // The softer, non-stopping notice: fires once, well above the
                // hard critical_bytes threshold above.
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

// Grouped so `start` stays under clippy's argument limit; every field is
// still read individually the moment the function body begins.
pub struct RecStartOptions {
    pub show_cursor: bool,
    pub video_kbps: u32,
    pub video_fps: u32,
    pub canvas_ceiling: (u32, u32),
    pub disk_warn_mb: f64,
    pub max_min: f64,
    pub max_mb: f64,
}

// show_cursor: the mouse-cursor toggle (ships ON) -- windows-capture's
// own capture setting, never a manually drawn cursor. audio: neither
// source active takes the exact original video-only path unchanged.
pub fn start(
    target: RecTarget, output_path: PathBuf, audio: AudioConfig, canvas_mode: &str, opts: RecStartOptions,
) -> Result<RecSession, String> {
    let result = start_inner(target, output_path.clone(), audio, canvas_mode, opts);
    if result.is_err() {
        discard_start_leftovers(&output_path);
    }
    result
}

// What a refused start leaves behind: the empty output file and, for a take that
// had both audio sources, the microphone sidecar its writer thread had already
// begun. The sidecar belongs to this one unfinished take, so it goes with it.
pub(crate) fn discard_start_leftovers(output_path: &Path) {
    discard_empty_output(output_path);
    let _ = fs::remove_file(output_path.with_extension("mic.wav"));
}

// Everything start_inner has started so far. A failure after the first thread is up
// (the capture item is gone, the capture refuses to start) used to return without
// stopping any of them: the feeder waited on its queue for good, the audio threads
// kept capturing and the output file stayed open, so it could not even be deleted.
// Dropping the guard sets every stop flag, runs the wake action (the same
// request_stop the normal stop uses, which also ends the feeder's wait) and joins the
// threads, each for a bounded time. On success `take` hands the threads to the
// session and the guard is left empty, so its drop does nothing.
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
    // Both sources active used to mean "mixed into the encoder's one
    // track" -- now the encoder only ever gets ONE source (system, when
    // present; mic alone otherwise, byte-for-byte as before) and a
    // second, simultaneously-active source is written to its own sidecar
    // file instead of being mixed away. Either single-source case is
    // completely unchanged.
    let both_active = audio.system && audio.mic;
    // A Window target tries its canvas candidates in order (every
    // display -> the display it started on -> its own size); Monitor and
    // Region targets have exactly one candidate, unchanged from before.
    // Bitrate is scaled down for a placement candidate whose canvas is
    // bigger than the window itself, so a small window on a big canvas
    // does not spend its whole budget on pixels that are just black.
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
        ));
        audio_queues.push(q.clone());
        system_queue = Some(q);
    }
    if audio.mic {
        let q = Arc::new(AudioQueueShared::new());
        guard.audio_threads.push(super::audio::spawn_capture(
            "mic", eCapture, false, audio.mic_device_id.clone(), q.clone(), audio_stop.clone().unwrap(),
        ));
        audio_queues.push(q.clone());
        mic_queue = Some(q);
    }
    // Queues the encoder's own feeder drains: everything except a mic
    // riding along beside an already-present system track (both_active) --
    // that one is pulled out to the sidecar writer below instead. System
    // was always pushed first above, so take(1) is exactly that queue.
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
    let crop = resolve_crop(target);
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
    // Discard whatever the WASAPI threads pushed while paused -- see the
    // audio_queues field doc for why stale paused-duration audio must
    // never reach the encoder.
    for q in &session.audio_queues {
        q.clear();
    }
}

// Active-recording-time elapsed so far, in whole milliseconds -- for the
// control bar's live timer. The clock's own unit is 100ns ticks (what
// send_frame_buffer wants), so this is display-only, never fed back into
// the encoder.
pub fn elapsed_ms(session: &RecSession) -> i64 {
    session.clock.now_ticks() / 10_000
}

// Whether this session can offer a live target switch at all -- a plain
// window recording using one of the placement canvases (see CanvasCandidate).
// A Monitor/Region target, or the "window's own size" fallback tier, has
// no spare canvas room a different window could be re-anchored into.
pub fn can_switch_target(session: &RecSession) -> bool {
    session.canvas_origin.is_some()
}

// Whether a specific new window fits the ALREADY BUILT canvas without
// rebuilding the encoder: "all" fits anything within the virtual screen
// by construction, "monitor" only a window on that exact same display, and
// the "window" fallback tier (Some("window") or None) never offers a
// switch at all. Pure and independent of any capture handle so it is
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

// Swaps a Window recording's capture source to a different window while
// keeping the same encoder, canvas, clock and output file. A new,
// second WGC session is started into the SAME canvas origin and pushes
// frames into the SAME feeder queue the running encoder already drains;
// only once that new session has proven it is actually delivering (its own
// first frame) does the old one get torn down, so the seam never drops a
// frame and the timestamps -- driven by the one shared clock throughout --
// never jump. A failure at any step leaves the OLD session completely
// untouched: the recording it is already producing never dies from a
// failed switch.
//
// Split in three so the (up to 2 s) wait for the first frame happens without
// the recording-state lock: begin (validate, start the second capture),
// wait, commit (swap the handles under the lock again).
pub struct PendingSwitch {
    control: CaptureControl<RecHandler, RecError>,
    first_frame: Arc<AtomicBool>,
    // Identifies the session the switch was started for.
    shared: Arc<RecFeederShared>,
    // The recording's clock: a paused one delivers no frames, so the wait below
    // must not count paused time against the first-frame budget.
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

// What is left of the first-frame budget after one 10 ms tick: a paused recording
// delivers no frame, so a paused tick costs nothing.
pub(crate) fn switch_wait_budget_after_tick(budget_ms: u32, paused: bool) -> u32 {
    if paused {
        budget_ms
    } else {
        budget_ms.saturating_sub(10)
    }
}

pub fn wait_switch_first_frame(pending: PendingSwitch) -> Result<PendingSwitch, String> {
    // A real frame typically lands within a couple hundred ms; 2s gives a
    // slow first WGC frame plenty of room without leaving a failed switch
    // hanging the caller for long.
    // Only running time counts: while the recording is paused no capture delivers a
    // frame, so a pause during the wait must not turn into a timeout. A recording
    // that is being stopped ends the wait.
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

/// Swaps the proven new capture in. The recording may have ended or moved on to
/// another session while the lock was released; then the new capture is just
/// stopped again and the switch fails.
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
        // The new source is already live and recording fine -- losing the
        // old capture handle cleanly is a log-worthy oddity, not a reason
        // to fail a switch that otherwise fully succeeded.
        append_crash_log(&format!("recording: old capture control stop failed after a target switch: {e}"));
    }
    // A later "finish and continue" reopens against whatever this session
    // is ACTUALLY capturing right now, so a switch updates that record too.
    session.target = RecTarget::Window(pending.new_hwnd);
    Ok(())
}

// Shared tail of stop/cancel: drain the queue and finalise the encoder,
// THEN tear down the capture thread -- Trap 1. Finalising after stop()
// finalises against a WM_QUIT'd capture pipeline and produces an MP4 with
// no closed moov atom (unplayable).
fn finish_and_stop(mut session: RecSession) -> Result<(u64, Option<String>), String> {
    // Both sides stop PRODUCING new content at essentially the same
    // instant -- audio_stop cuts the WASAPI capture threads, is_stopping
    // (checked in on_frame_arrived) cuts new video frames -- before
    // either side is joined/drained, so neither track keeps growing
    // while the other has already gone silent.
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
    // The encoder is already finalised at this point (finish() ran inside
    // the feeder thread's own tail above), so a failure here is only the
    // WGC capture handle itself refusing to stop cleanly -- it does not
    // undo the file already written. Surface it like any other non-fatal
    // encode error instead of discarding the whole result: propagating it
    // as a hard Err would drop the session out of RecordingState with no
    // "recording-captured" event ever emitted, stranding a finished
    // recording with no reachable result dialog.
    if let Err(e) = session.control.stop() {
        append_crash_log(&format!("recording: capture control stop failed: {e}"));
        encode_error = encode_error.or_else(|| Some(e.to_string()));
    }
    Ok((dropped, encode_error))
}

// The fourth field is the mic sidecar's path, only when both system
// and mic were captured -- the video editor consolidates it into a real
// second AAC track the first time it opens this recording; nothing else
// in the app looks at it, so a workflow that never opens the editor never
// needs ffmpeg for a plain recording.
// The sixth and seventh fields let the caller decide whether a post-capture
// crop is worthwhile: the used-area union rect (canvas-local coordinates,
// None when this was not a placement recording) and the canvas size it is
// relative to. The eighth is the "window size" canvas default's own
// reactive notice: the timestamp of the first frame the resize-safety-net
// had to shrink to keep fitting, or None if that never happened this
// recording.
// path, dropped-frame count, encode error, mic sidecar path, canvas note,
// used-area crop rect, canvas size, first-resize timestamp -- see the
// field-by-field comment above.
pub type StopResult = (PathBuf, u64, Option<String>, Option<PathBuf>, Option<&'static str>, Option<(u32, u32, u32, u32)>, (u32, u32), Option<u64>);

pub fn stop(session: RecSession) -> Result<StopResult, String> {
    let path = session.output_path.clone();
    let mic_wav_path = session.mic_wav_path.clone();
    let canvas_note = session.canvas_note;
    let used_area = session.used_area.clone();
    let canvas_size = session.canvas_size;
    let shared = session.shared.clone();
    let (dropped, encode_error) = finish_and_stop(session)?;
    // Only safe to read now: the capture thread (used_area's and the
    // resize-crop tracker's sole writer) is guaranteed joined once
    // finish_and_stop has returned.
    let crop_rect = used_area.and_then(|u| u.get());
    let resize_crop_at_ms = shared.resize_crop_at_ms();
    Ok((path, dropped, encode_error, mic_wav_path, canvas_note, crop_rect, canvas_size, resize_crop_at_ms))
}

// Live-poll accessor for the control bar: lets it notice an unrecoverable
// encoder failure (e.g. disk full) WHILE still nominally "recording" and
// stop itself, instead of the bar sitting there looking like it is still
// capturing while every frame silently fails in the background.
pub fn encode_error(session: &RecSession) -> Option<String> {
    session.shared.peek_encode_error()
}

// Same live-poll shape as encode_error above, but consuming: the low-disk
// mid-recording notice fires exactly once, never stops the recording.
pub fn take_disk_low(session: &RecSession) -> Option<(u64, u64)> {
    session.shared.take_disk_low()
}

// The output file exists the instant VideoEncoder::new() runs (it opens the
// file eagerly), so a clean cancel still finalises it -- via the same
// finish_and_stop tail, to keep exactly one shutdown sequence -- and then
// moves it to the holding area for discarded recordings.
pub fn cancel(session: RecSession) -> Result<(), String> {
    let path = session.output_path.clone();
    let mic_wav_path = session.mic_wav_path.clone();
    finish_and_stop(session)?;
    crate::media_files::discard_or_remove_take(&path);
    if let Some(w) = mic_wav_path {
        let _ = std::fs::remove_file(&w);
    }
    Ok(())
}

pub fn list_mic_devices() -> Vec<super::audio::MicDevice> {
    super::audio::list_mic_devices()
}

pub fn new_output_path(recordings_dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(recordings_dir).map_err(|e| e.to_string())?;
    let st = unsafe { GetLocalTime() };
    let name = rec_filename(
        st.wYear as i32, st.wMonth as u32, st.wDay as u32, st.wHour as u32, st.wMinute as u32, st.wSecond as u32,
    );
    Ok(recordings_dir.join(name))
}

// Every continuation part keeps the very first file's own name stem and
// timestamp, with a numbered suffix -- so a folder listing groups every
// part of one "finish and continue" chain together under its original
// name instead of scattering fresh timestamps across it. Numbering starts
// at the second file: the first part IS the recording, nothing about its
// name says "part 1".
pub fn continuation_output_path(base: &Path, part_number: u32) -> PathBuf {
    let stem = base.file_stem().and_then(|s| s.to_str()).unwrap_or("rec");
    let ext = base.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    base.with_file_name(format!("{stem}_part{part_number}.{ext}"))
}
