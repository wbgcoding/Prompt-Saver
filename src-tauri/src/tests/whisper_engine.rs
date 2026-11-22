use super::*;
use crate::whisper_audio::*;
use crate::whisper_engine::*;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const RATE: f32 = 16_000.0;

fn tone(secs: f32, amp: f32) -> Vec<f32> {
    (0..(secs * RATE) as usize).map(|i| amp * (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / RATE).sin()).collect()
}

fn quiet(secs: f32) -> Vec<f32> {
    vec![0.0; (secs * RATE) as usize]
}

fn cat(parts: &[Vec<f32>]) -> Vec<f32> {
    parts.iter().flatten().copied().collect()
}

// All phrases the segmenter cuts out of a signal, fed in chunks of the given size.
fn phrases_of(samples: &[f32], chunk: usize) -> Vec<Vec<f32>> {
    let mut seg = Segmenter::new();
    let mut out = Vec::new();
    for c in samples.chunks(chunk) {
        out.extend(seg.push(c));
    }
    out.extend(seg.flush());
    out
}

fn secs(n: &[f32]) -> f32 {
    n.len() as f32 / RATE
}

// ---------- Resampling ----------

#[test]
fn capture_format_averages_every_three_stereo_frames_to_one_mono_sample() {
    // 48 kHz stereo: a frame is 2 samples, 3 frames make one output sample.
    let pcm: Vec<i16> = vec![16384; 6 * 4];
    let mut carry = Vec::new();
    let out = mono16k_from_capture(&pcm, &mut carry);
    assert_eq!(out.len(), 4);
    assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-6));
    assert!(carry.is_empty());
}

#[test]
fn capture_conversion_is_the_same_for_any_chunking() {
    let pcm: Vec<i16> = (0..6000).map(|i| ((i * 37) % 2000) as i16 - 1000).collect();
    let whole = mono16k_from_capture(&pcm, &mut Vec::new());
    for chunk in [1usize, 2, 5, 7, 100, 1001] {
        let mut carry = Vec::new();
        let mut joined = Vec::new();
        for c in pcm.chunks(chunk) {
            joined.extend(mono16k_from_capture(c, &mut carry));
        }
        assert_eq!(joined, whole, "chunk size {chunk}");
    }
}

#[test]
fn capture_conversion_keeps_a_partial_group_for_the_next_call() {
    let mut carry = Vec::new();
    assert!(mono16k_from_capture(&[1, 2, 3], &mut carry).is_empty());
    assert_eq!(carry.len(), 3);
    let out = mono16k_from_capture(&[4, 5, 6], &mut carry);
    assert_eq!(out.len(), 1);
    assert!(carry.is_empty());
}

#[test]
fn resampling_changes_the_length_by_the_rate_ratio() {
    let src = tone(1.0, 0.5);
    assert_eq!(resample_to_16k(&src, 16_000), src);
    assert_eq!(resample_to_16k(&src, 0), src);
    assert!(resample_to_16k(&[], 44_100).is_empty());
    let up: Vec<f32> = (0..8000).map(|i| (i as f32 * 0.05).sin()).collect();
    assert_eq!(resample_to_16k(&up, 8_000).len(), 16_000);
    let down: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.01).sin()).collect();
    assert_eq!(resample_to_16k(&down, 48_000).len(), 16_000);
    let cd: Vec<f32> = vec![0.1; 44_100];
    assert_eq!(resample_to_16k(&cd, 44_100).len(), 16_000);
}

#[test]
fn resampling_keeps_the_pitch() {
    // A 440 Hz tone sampled at 48 kHz still crosses zero about 880 times a second at 16 kHz.
    let src: Vec<f32> = (0..48_000).map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / 48_000.0).sin()).collect();
    let out = resample_to_16k(&src, 48_000);
    let crossings = out.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
    assert!((860..=900).contains(&crossings), "{crossings} zero crossings");
}

// ---------- Phrase splitting ----------

#[test]
fn silence_tone_silence_is_one_phrase() {
    let signal = cat(&[quiet(1.0), tone(1.0, 0.3), quiet(1.0)]);
    let phrases = phrases_of(&signal, 1600);
    assert_eq!(phrases.len(), 1);
    // The tone plus a little lead-in and tail, never the whole second of silence either side.
    let len = secs(&phrases[0]);
    assert!((1.0..1.8).contains(&len), "{len} s");
}

#[test]
fn a_pause_of_600_ms_or_more_splits_two_phrases_and_a_shorter_one_does_not() {
    let split = cat(&[tone(0.8, 0.3), quiet(0.7), tone(0.8, 0.3), quiet(1.0)]);
    assert_eq!(phrases_of(&split, 1600).len(), 2);
    let joined = cat(&[tone(0.8, 0.3), quiet(0.4), tone(0.8, 0.3), quiet(1.0)]);
    let phrases = phrases_of(&joined, 1600);
    assert_eq!(phrases.len(), 1);
    assert!(secs(&phrases[0]) > 2.0);
}

#[test]
fn a_click_is_not_a_phrase_but_a_short_word_is() {
    assert!(phrases_of(&cat(&[quiet(1.0), tone(0.1, 0.5), quiet(1.5)]), 1600).is_empty());
    assert_eq!(phrases_of(&cat(&[quiet(1.0), tone(0.4, 0.3), quiet(1.5)]), 1600).len(), 1);
}

#[test]
fn quiet_audio_never_produces_a_phrase() {
    assert!(phrases_of(&quiet(5.0), 1600).is_empty());
    let hiss: Vec<f32> = (0..80_000).map(|i| 0.002 * ((i * 7919) % 13) as f32 / 13.0).collect();
    assert!(phrases_of(&hiss, 1600).is_empty());
}

// Speech-like: half a second of voice, then 120 ms between words, over and over.
fn talk(secs: f32) -> Vec<f32> {
    let mut out = Vec::new();
    while (out.len() as f32) < secs * RATE {
        out.extend(tone(0.5, 0.3));
        out.extend(quiet(0.12));
    }
    out.truncate((secs * RATE) as usize);
    out
}

// Deterministic hiss with the given RMS level (uniform noise, RMS = amplitude / sqrt(3)).
fn noise(secs: f32, rms: f32, seed: u32) -> Vec<f32> {
    let amp = rms * 3f32.sqrt();
    let mut x = seed.wrapping_mul(2_654_435_761).max(1);
    (0..(secs * RATE) as usize)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            amp * ((x as f32 / u32::MAX as f32) * 2.0 - 1.0)
        })
        .collect()
}

fn mix(a: &[f32], b: &[f32]) -> Vec<f32> {
    a.iter().zip(b).map(|(x, y)| x + y).collect()
}

#[test]
fn a_long_phrase_is_cut_at_a_quiet_moment_and_the_rest_is_kept() {
    let signal = talk(20.0);
    let mut seg = Segmenter::new();
    let mut done = Vec::new();
    for c in signal.chunks(1600) {
        done.extend(seg.push(c));
    }
    assert_eq!(done.len(), 1);
    let first = secs(&done[0]);
    assert!(first <= PHRASE_MAX_SECS as f32 + 0.05, "{first} s");
    assert!(first >= PHRASE_MAX_SECS as f32 - 1.6, "{first} s");
    // The cut lies in a pause between words, not in the middle of one.
    let last_frame = &done[0][done[0].len() - 480..];
    assert!(last_frame.iter().all(|s| s.abs() < 1e-6), "cut inside a word");
    let rest = seg.flush().expect("the remainder is a phrase");
    assert!((first + secs(&rest) - 20.0).abs() < 0.2, "{first} s + {} s", secs(&rest));
}

#[test]
fn a_noisy_microphone_still_splits_phrases_at_pauses() {
    // Background hiss well above the fixed threshold, as from a cheap USB microphone. The
    // speech bursts on top of it must still come out one by one, and the hiss between and
    // after them must count as quiet so the pause limit can run out.
    let bed = noise(12.0, 0.02, 7);
    let mut voice = quiet(4.0);
    voice.extend(talk(1.5));
    voice.extend(quiet(1.5));
    voice.extend(talk(1.5));
    voice.extend(quiet(3.5));
    let signal = mix(&bed, &voice);
    let mut seg = Segmenter::new();
    let mut done = Vec::new();
    for c in signal.chunks(1600) {
        done.extend(seg.push(c));
    }
    assert!(!seg.speaking(), "the hiss after the speech still counts as speech");
    assert!(seg.flush().is_none(), "a phrase is still open in the hiss");
    // The two bursts are the last two phrases (the first seconds, before the level is known,
    // may yield one phrase of hiss), each about as long as the burst.
    assert!(done.len() >= 2, "{} phrases", done.len());
    for p in &done[done.len() - 2..] {
        assert!((1.4..2.6).contains(&secs(p)), "{} s", secs(p));
    }
}

#[test]
fn splitting_does_not_depend_on_how_the_audio_is_chunked() {
    let signal = cat(&[quiet(0.5), tone(0.9, 0.3), quiet(0.8), tone(0.5, 0.3), quiet(1.0)]);
    let whole = phrases_of(&signal, 1600);
    for chunk in [1usize, 77, 480, 481, 5000] {
        assert_eq!(phrases_of(&signal, chunk), whole, "chunk size {chunk}");
    }
    assert_eq!(whole.len(), 2);
}

#[test]
fn flush_gives_the_phrase_in_progress_and_then_nothing() {
    let mut seg = Segmenter::new();
    assert!(seg.push(&cat(&[quiet(0.3), tone(0.8, 0.3)])).is_empty());
    assert!(seg.speaking());
    assert!(seg.flush().is_some());
    assert!(seg.flush().is_none());
    let mut idle = Segmenter::new();
    idle.push(&quiet(1.0));
    assert!(!idle.speaking());
    assert!(idle.flush().is_none());
}

// ---------- Language and text ----------

#[test]
fn language_tags_map_to_engine_codes() {
    assert_eq!(whisper_lang("de-DE"), Some("de"));
    assert_eq!(whisper_lang("en-US"), Some("en"));
    assert_eq!(whisper_lang("pt_BR"), Some("pt"));
    assert_eq!(whisper_lang("zh-Hans-CN"), Some("zh"));
    assert_eq!(whisper_lang("ZH"), Some("zh"));
    assert_eq!(whisper_lang("nb-NO"), Some("no"));
    assert_eq!(whisper_lang("fil-PH"), Some("tl"));
    assert_eq!(whisper_lang("yue"), Some("yue"));
}

#[test]
fn unknown_or_automatic_language_lets_the_engine_detect() {
    assert_eq!(whisper_lang("auto"), None);
    assert_eq!(whisper_lang(""), None);
    assert_eq!(whisper_lang("  "), None);
    assert_eq!(whisper_lang("xx-YY"), None);
    assert_eq!(whisper_lang("-"), None);
}

#[test]
fn every_interface_language_is_a_known_engine_language() {
    for code in crate::model::LANG_CODES {
        assert!(whisper_lang(code).is_some(), "{code}");
    }
}

#[test]
fn the_reported_language_is_the_chosen_one_or_the_interface_language() {
    assert_eq!(resolved_lang_tag("de-DE", "en"), "de-DE");
    assert_eq!(resolved_lang_tag("auto", "fr"), "fr");
    assert_eq!(resolved_lang_tag("AUTO", "fr"), "fr");
    assert_eq!(resolved_lang_tag("", "ja"), "ja");
    assert_eq!(resolved_lang_tag("  ", "ja"), "ja");
}

#[test]
fn engine_placeholders_for_no_speech_become_empty_text() {
    for raw in ["[BLANK_AUDIO]", " (silence) ", "*music*", "[ Silence ]", "", "   "] {
        assert_eq!(clean_text(raw), "", "{raw:?}");
    }
    assert_eq!(clean_text("  Hello there. "), "Hello there.");
    assert_eq!(clean_text("Das ist (fast) alles"), "Das ist (fast) alles");
    assert_eq!(clean_text("*"), "*");
}

#[test]
fn threads_follow_the_physical_cores_up_to_eight() {
    assert_eq!(engine_threads(0), 1);
    assert_eq!(engine_threads(1), 1);
    assert_eq!(engine_threads(6), 6);
    assert_eq!(engine_threads(8), 8);
    assert_eq!(engine_threads(20), 8);
}

// ---------- WAV reading ----------

fn wav(format: u16, channels: u16, rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&format.to_le_bytes());
    v.extend_from_slice(&channels.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&(rate * channels as u32 * bits as u32 / 8).to_le_bytes());
    v.extend_from_slice(&(channels * bits / 8).to_le_bytes());
    v.extend_from_slice(&bits.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&(data.len() as u32).to_le_bytes());
    v.extend_from_slice(data);
    v
}

fn pcm16(samples: &[f32]) -> Vec<u8> {
    samples.iter().flat_map(|s| ((s * 32767.0) as i16).to_le_bytes()).collect()
}

#[test]
fn wav_16_bit_mono_reads_back() {
    let src = tone(0.5, 0.4);
    let (rate, mono) = parse_wav(&wav(1, 1, 16_000, 16, &pcm16(&src))).unwrap();
    assert_eq!(rate, 16_000);
    assert_eq!(mono.len(), src.len());
    assert!(mono.iter().zip(&src).all(|(a, b)| (a - b).abs() < 1e-3));
}

#[test]
fn wav_stereo_is_averaged_to_mono() {
    let data: Vec<u8> = [16384i16, -16384, 16384, 0].iter().flat_map(|s| s.to_le_bytes()).collect();
    let (_, mono) = parse_wav(&wav(1, 2, 44_100, 16, &data)).unwrap();
    assert_eq!(mono.len(), 2);
    assert!(mono[0].abs() < 1e-6);
    assert!((mono[1] - 0.25).abs() < 1e-4);
}

#[test]
fn wav_8_24_bit_and_float_are_understood() {
    let (_, m8) = parse_wav(&wav(1, 1, 8_000, 8, &[128, 255, 0])).unwrap();
    assert!(m8[0].abs() < 1e-6 && m8[1] > 0.99 && m8[2] < -0.99);
    let d24: Vec<u8> = vec![0x00, 0x00, 0x40, 0x00, 0x00, 0xC0];
    let (_, m24) = parse_wav(&wav(1, 1, 8_000, 24, &d24)).unwrap();
    assert!((m24[0] - 0.5).abs() < 1e-4 && (m24[1] + 0.5).abs() < 1e-4);
    let df: Vec<u8> = [0.25f32, -0.5].iter().flat_map(|s| s.to_le_bytes()).collect();
    let (_, mf) = parse_wav(&wav(3, 1, 8_000, 32, &df)).unwrap();
    assert_eq!(mf, vec![0.25, -0.5]);
}

#[test]
fn wav_with_extra_chunks_and_a_cut_off_data_chunk_still_reads() {
    let mut v = wav(1, 1, 16_000, 16, &pcm16(&tone(0.2, 0.3)));
    // A LIST chunk (odd size, so with a pad byte) between fmt and data.
    let at = 36;
    let mut extra = Vec::new();
    extra.extend_from_slice(b"LIST");
    extra.extend_from_slice(&3u32.to_le_bytes());
    extra.extend_from_slice(&[1, 2, 3, 0]);
    v.splice(at..at, extra);
    assert!(parse_wav(&v).is_ok());
    // The header claims more data than the file holds.
    let mut cut = wav(1, 1, 16_000, 16, &pcm16(&tone(0.2, 0.3)));
    cut.truncate(cut.len() - 7);
    assert!(parse_wav(&cut).is_ok());
}

#[test]
fn wav_garbage_is_refused_without_a_panic() {
    assert_eq!(parse_wav(b"").unwrap_err(), "wav-invalid");
    assert_eq!(parse_wav(b"RIFF").unwrap_err(), "wav-invalid");
    assert_eq!(parse_wav(&[7u8; 100]).unwrap_err(), "wav-invalid");
    assert_eq!(parse_wav(&wav(1, 0, 16_000, 16, &[0, 0])).unwrap_err(), "wav-invalid");
    assert_eq!(parse_wav(&wav(1, 1, 0, 16, &[0, 0])).unwrap_err(), "wav-invalid");
    assert_eq!(parse_wav(&wav(2, 1, 16_000, 4, &[0, 0])).unwrap_err(), "wav-unsupported");
    let no_data = wav(1, 1, 16_000, 16, &[]);
    assert!(parse_wav(&no_data[..36]).is_err());
    for cut in 0..80 {
        let _ = parse_wav(&wav(1, 1, 16_000, 16, &[1, 2, 3, 4])[..cut.min(48)]);
    }
}

// ---------- Fake engines ----------

#[derive(Default)]
struct Calls(Mutex<Vec<(usize, Option<String>)>>);

struct Numbered {
    calls: Arc<Calls>,
    fail: bool,
}

impl Transcribe for Numbered {
    fn transcribe(&mut self, samples: &[f32], lang: Option<&str>) -> Result<String, String> {
        let mut c = self.calls.0.lock().unwrap();
        c.push((samples.len(), lang.map(str::to_string)));
        if self.fail {
            return Err("engine broke".to_string());
        }
        Ok(format!("phrase {}", c.len()))
    }
}

// Blocks inside a phrase until released, and says when it got there.
struct Gated {
    started: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}

impl Transcribe for Gated {
    fn transcribe(&mut self, _: &[f32], _: Option<&str>) -> Result<String, String> {
        let _ = self.started.send(());
        self.release.recv_timeout(Duration::from_secs(10)).map_err(|_| "never released".to_string())?;
        Ok("late words".to_string())
    }
}

// Works on a phrase until told to abort, like the real engine's abort callback.
struct Stubborn {
    started: mpsc::Sender<()>,
    abort: Arc<AtomicBool>,
}

impl Transcribe for Stubborn {
    fn transcribe(&mut self, _: &[f32], _: Option<&str>) -> Result<String, String> {
        let _ = self.started.send(());
        let until = Instant::now() + Duration::from_secs(8);
        while Instant::now() < until {
            if self.abort.load(Ordering::SeqCst) {
                return Err("aborted".to_string());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok("too slow".to_string())
    }
}

fn config(auto_stop_sec: u32, max_sec: u32) -> SessionConfig {
    SessionConfig { lang: Some("de"), auto_stop_sec, max_sec, poll: Duration::from_millis(1), interim: false }
}

type Texts = Arc<Mutex<Vec<String>>>;

fn collector() -> (Texts, Arc<dyn Fn(String) + Send + Sync>) {
    let texts: Texts = Arc::default();
    let sink = texts.clone();
    (texts, Arc::new(move |t| sink.lock().unwrap().push(t)))
}

// Feeds a clip in 100 ms pieces, then asks for a manual stop and keeps returning nothing.
fn clip_feed(samples: Vec<f32>, stop: mpsc::Sender<&'static str>) -> impl FnMut() -> Result<Vec<f32>, &'static str> {
    let mut q: VecDeque<Vec<f32>> = samples.chunks(1600).map(|c| c.to_vec()).collect();
    move || match q.pop_front() {
        Some(c) => Ok(c),
        None => {
            let _ = stop.send("manual");
            Ok(Vec::new())
        }
    }
}

fn run_clip(clip: Vec<f32>, engine: Box<dyn Transcribe + Send>, cfg: SessionConfig) -> (SessionOutcome, Vec<String>) {
    let (tx, rx) = mpsc::channel();
    let (texts, sink) = collector();
    let mut feed = clip_feed(clip, tx);
    let outcome = run_pipeline(&cfg, engine, &mut feed, &rx, &Arc::new(AtomicBool::new(false)), sink);
    let out = texts.lock().unwrap().clone();
    (outcome, out)
}

// ---------- Session loop ----------

#[test]
fn each_phrase_is_transcribed_in_order_with_the_chosen_language() {
    let calls = Arc::new(Calls::default());
    let clip = cat(&[quiet(0.5), tone(0.8, 0.3), quiet(0.9), tone(0.6, 0.3), quiet(0.9), tone(0.5, 0.3)]);
    let (outcome, texts) = run_clip(clip, Box::new(Numbered { calls: calls.clone(), fail: false }), config(0, 600));
    assert_eq!(texts, vec!["phrase 1", "phrase 2", "phrase 3"]);
    assert_eq!(outcome.reason, "manual");
    assert_eq!(outcome.phrases, 3);
    assert!(outcome.error.is_none());
    let seen = calls.0.lock().unwrap();
    assert!(seen.iter().all(|(_, lang)| lang.as_deref() == Some("de")));
}

#[test]
fn the_phrase_still_being_spoken_when_the_user_stops_is_not_lost() {
    let (outcome, texts) = run_clip(tone(1.0, 0.3), Box::new(Numbered { calls: Arc::default(), fail: false }), config(0, 600));
    assert_eq!(texts, vec!["phrase 1"]);
    assert_eq!(outcome.reason, "manual");
}

#[test]
fn a_session_of_silence_ends_by_the_pause_limit_having_heard_nothing() {
    let (tx, rx) = mpsc::channel::<&'static str>();
    let _keep_open = tx;
    let (texts, sink) = collector();
    let mut feed = || -> Result<Vec<f32>, &'static str> {
        std::thread::sleep(Duration::from_millis(5));
        Ok(quiet(0.1))
    };
    let t0 = Instant::now();
    let outcome = run_pipeline(
        &config(1, 600),
        Box::new(Numbered { calls: Arc::default(), fail: false }),
        &mut feed,
        &rx,
        &Arc::new(AtomicBool::new(false)),
        sink,
    );
    assert_eq!(outcome.reason, "silence");
    assert_eq!(outcome.phrases, 0);
    assert!(outcome.error.is_none());
    assert!(texts.lock().unwrap().is_empty());
    assert!(t0.elapsed() >= Duration::from_secs(1) && t0.elapsed() < Duration::from_secs(4));
}

#[test]
fn speech_keeps_the_pause_limit_from_running_out() {
    // Tone with short gaps for longer than the limit: it must end by the time limit instead.
    let (tx, rx) = mpsc::channel::<&'static str>();
    let _keep_open = tx;
    let (texts, sink) = collector();
    let mut n = 0u32;
    let mut feed = move || -> Result<Vec<f32>, &'static str> {
        std::thread::sleep(Duration::from_millis(5));
        n += 1;
        Ok(if n.is_multiple_of(8) { quiet(0.1) } else { tone(0.1, 0.3) })
    };
    let outcome = run_pipeline(
        &config(1, 2),
        Box::new(Numbered { calls: Arc::default(), fail: false }),
        &mut feed,
        &rx,
        &Arc::new(AtomicBool::new(false)),
        sink,
    );
    assert_eq!(outcome.reason, "max");
    assert!(outcome.phrases >= 1);
    assert!(!texts.lock().unwrap().is_empty());
}

#[test]
fn a_lost_microphone_ends_the_session_with_the_mic_error() {
    let (_tx, rx) = mpsc::channel::<&'static str>();
    let (_texts, sink) = collector();
    let mut n = 0;
    let mut feed = move || -> Result<Vec<f32>, &'static str> {
        n += 1;
        if n > 3 { Err("voice-mic") } else { Ok(tone(0.1, 0.3)) }
    };
    let outcome = run_pipeline(
        &config(0, 600),
        Box::new(Numbered { calls: Arc::default(), fail: false }),
        &mut feed,
        &rx,
        &Arc::new(AtomicBool::new(false)),
        sink,
    );
    assert_eq!(outcome.reason, "error");
    assert_eq!(outcome.error, Some("voice-mic"));
}

#[test]
fn a_failing_engine_ends_the_session_with_an_error() {
    let clip = cat(&[tone(0.8, 0.3), quiet(1.0), tone(0.8, 0.3), quiet(1.0)]);
    let (outcome, texts) = run_clip(clip, Box::new(Numbered { calls: Arc::default(), fail: true }), config(0, 600));
    assert_eq!(outcome.reason, "error");
    assert_eq!(outcome.error, Some("voice-failed"));
    assert!(texts.is_empty());
}

#[test]
fn a_dropped_stop_channel_counts_as_a_shutdown() {
    let (tx, rx) = mpsc::channel::<&'static str>();
    drop(tx);
    let (_texts, sink) = collector();
    let mut feed = || -> Result<Vec<f32>, &'static str> { Ok(Vec::new()) };
    let outcome = run_pipeline(
        &config(0, 600),
        Box::new(Numbered { calls: Arc::default(), fail: false }),
        &mut feed,
        &rx,
        &Arc::new(AtomicBool::new(false)),
        sink,
    );
    assert_eq!(outcome.reason, "shutdown");
    assert!(outcome.error.is_none());
}

#[test]
fn a_manual_stop_during_transcription_waits_for_the_text() {
    let (stop_tx, stop_rx) = mpsc::channel::<&'static str>();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (texts, sink) = collector();
    let handle = std::thread::spawn(move || {
        // A phrase, then endless silence: only the stop message ends the session.
        let mut q: VecDeque<Vec<f32>> = cat(&[tone(0.8, 0.3), quiet(0.9)]).chunks(1600).map(|c| c.to_vec()).collect();
        let mut feed = move || -> Result<Vec<f32>, &'static str> {
            std::thread::sleep(Duration::from_millis(2));
            Ok(q.pop_front().unwrap_or_else(|| quiet(0.1)))
        };
        run_pipeline(
            &config(0, 600),
            Box::new(Gated { started: started_tx, release: release_rx }),
            &mut feed,
            &stop_rx,
            &Arc::new(AtomicBool::new(false)),
            sink,
        )
    });
    started_rx.recv_timeout(Duration::from_secs(5)).expect("the phrase reached the engine");
    stop_tx.send("manual").unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert!(!handle.is_finished(), "the session ended before the engine finished the phrase");
    release_tx.send(()).unwrap();
    let outcome = handle.join().unwrap();
    assert_eq!(outcome.reason, "manual");
    assert_eq!(outcome.phrases, 1);
    assert_eq!(texts.lock().unwrap().clone(), vec!["late words"]);
}

fn shutdown_while_transcribing(first_stop: Option<&'static str>) -> (SessionOutcome, Vec<String>, Duration) {
    let (stop_tx, stop_rx) = mpsc::channel::<&'static str>();
    let (started_tx, started_rx) = mpsc::channel();
    let abort = Arc::new(AtomicBool::new(false));
    let engine_abort = abort.clone();
    let (texts, sink) = collector();
    let handle = std::thread::spawn(move || {
        let mut q: VecDeque<Vec<f32>> = cat(&[tone(0.8, 0.3), quiet(0.9)]).chunks(1600).map(|c| c.to_vec()).collect();
        let mut feed = move || -> Result<Vec<f32>, &'static str> {
            std::thread::sleep(Duration::from_millis(2));
            Ok(q.pop_front().unwrap_or_else(|| quiet(0.1)))
        };
        run_pipeline(&config(0, 600), Box::new(Stubborn { started: started_tx, abort: engine_abort }), &mut feed, &stop_rx, &abort, sink)
    });
    started_rx.recv_timeout(Duration::from_secs(5)).expect("the phrase reached the engine");
    if let Some(first) = first_stop {
        stop_tx.send(first).unwrap();
        std::thread::sleep(Duration::from_millis(50));
    }
    let t0 = Instant::now();
    stop_tx.send("shutdown").unwrap();
    let outcome = handle.join().unwrap();
    let took = t0.elapsed();
    let out = texts.lock().unwrap().clone();
    (outcome, out, took)
}

#[test]
fn a_shutdown_during_transcription_aborts_the_engine_at_once() {
    let (outcome, texts, took) = shutdown_while_transcribing(None);
    assert_eq!(outcome.reason, "shutdown");
    assert!(outcome.error.is_none(), "an aborted phrase is not a failure");
    assert!(texts.is_empty());
    assert!(took < Duration::from_secs(2), "took {took:?}");
}

#[test]
fn a_shutdown_while_a_manual_stop_is_still_waiting_for_text_cuts_it_short() {
    let (outcome, texts, took) = shutdown_while_transcribing(Some("manual"));
    assert_eq!(outcome.reason, "shutdown");
    assert!(outcome.error.is_none());
    assert!(texts.is_empty());
    assert!(took < Duration::from_secs(2), "took {took:?}");
}

#[test]
fn a_wav_file_goes_through_the_same_pipeline() {
    let clip = cat(&[quiet(0.5), tone(0.8, 0.3), quiet(0.9), tone(0.6, 0.3), quiet(0.5)]);
    // At 48 kHz, as most recorders write it: the resampler runs before the phrase splitter.
    let at48: Vec<f32> = clip.iter().flat_map(|s| [*s, *s, *s]).collect();
    let bytes = wav(1, 1, 48_000, 16, &pcm16(&at48));
    let calls = Arc::new(Calls::default());
    let mut engine = Numbered { calls: calls.clone(), fail: false };
    let out = transcribe_wav_bytes(&mut engine, &bytes, Some("en")).unwrap();
    assert_eq!(out, vec!["phrase 1", "phrase 2"]);
    assert!(calls.0.lock().unwrap().iter().all(|(_, l)| l.as_deref() == Some("en")));
    assert_eq!(transcribe_wav_bytes(&mut engine, b"nope", None).unwrap_err(), "wav-invalid");
    let mut failing = Numbered { calls: Arc::default(), fail: true };
    assert!(transcribe_wav_bytes(&mut failing, &bytes, None).is_err());
}

#[test]
fn a_file_with_only_silence_gives_no_text() {
    let bytes = wav(1, 1, 16_000, 16, &pcm16(&quiet(2.0)));
    let mut engine = Numbered { calls: Arc::default(), fail: false };
    assert!(transcribe_wav_bytes(&mut engine, &bytes, None).unwrap().is_empty());
}

// ---------- Model cache ----------

#[test]
fn the_cache_loads_once_per_model_and_reuses_it() {
    let cache: ModelCache<u32> = ModelCache::new();
    let loads = AtomicBool::new(false);
    let a = cache.acquire("base", || Ok(7)).unwrap();
    let b = cache.acquire("base", || {
        loads.store(true, Ordering::SeqCst);
        Ok(8)
    });
    assert_eq!(*a, 7);
    assert_eq!(*b.unwrap(), 7);
    assert!(!loads.load(Ordering::SeqCst), "a loaded model must not load again");
    let c = cache.acquire("small", || Ok(9)).unwrap();
    assert_eq!(*c, 9);
    assert!(cache.is_loaded());
}

#[test]
fn a_failed_load_leaves_the_cache_empty() {
    let cache: ModelCache<u32> = ModelCache::new();
    cache.acquire("base", || Ok(1)).unwrap();
    assert_eq!(cache.acquire("small", || Err("broken file".to_string())).unwrap_err(), "broken file");
    assert!(!cache.is_loaded());
}

#[test]
fn the_model_is_freed_only_after_five_idle_minutes() {
    let cache: ModelCache<u32> = ModelCache::new();
    let keep = MODEL_KEEP;
    assert_eq!(keep, Duration::from_secs(300));
    cache.acquire("base", || Ok(1)).unwrap();
    let t0 = Instant::now();
    // In use: never freed, however long it takes.
    assert!(matches!(cache.sweep(t0 + keep * 3, keep), Sweep::Wait(d) if d == keep));
    assert!(cache.is_loaded());
    assert!(cache.release(t0), "the first release asks for a timer");
    assert!(!cache.release(t0), "a second one does not");
    match cache.sweep(t0 + Duration::from_secs(100), keep) {
        Sweep::Wait(d) => assert_eq!(d, Duration::from_secs(200)),
        Sweep::Done => panic!("freed too early"),
    }
    assert!(cache.is_loaded());
    assert!(matches!(cache.sweep(t0 + keep, keep), Sweep::Done));
    assert!(!cache.is_loaded());
    // Nothing left: the timer ends, and a later session can start a new one.
    assert!(matches!(cache.sweep(t0 + keep * 2, keep), Sweep::Done));
    cache.acquire("base", || Ok(2)).unwrap();
    assert!(cache.release(t0 + keep * 2));
}

#[test]
fn using_the_model_again_restarts_the_idle_time() {
    let cache: ModelCache<u32> = ModelCache::new();
    let keep = Duration::from_secs(300);
    cache.acquire("base", || Ok(1)).unwrap();
    let t0 = Instant::now();
    cache.release(t0);
    cache.acquire("base", || Ok(2)).unwrap();
    assert!(matches!(cache.sweep(t0 + keep * 2, keep), Sweep::Wait(_)), "a model in use stays");
    cache.release(t0 + keep * 2);
    assert!(matches!(cache.sweep(t0 + keep * 2 + Duration::from_secs(10), keep), Sweep::Wait(_)));
    assert!(cache.is_loaded());
}

#[test]
fn the_release_timer_really_frees_an_idle_model() {
    let cache: &'static ModelCache<u32> = Box::leak(Box::new(ModelCache::new()));
    cache.acquire("base", || Ok(1)).unwrap();
    assert!(cache.release(Instant::now()));
    spawn_sweeper(cache, Duration::from_millis(60));
    let until = Instant::now() + Duration::from_secs(5);
    while cache.is_loaded() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!cache.is_loaded());
}

// ---------- Active model marker ----------

#[test]
fn the_active_marker_is_set_for_the_guard_and_cleared_on_every_way_out() {
    let slot: Mutex<Option<String>> = Mutex::new(None);
    {
        let _g = ActiveModelGuard::set(&slot, "small");
        assert_eq!(slot.lock().unwrap().as_deref(), Some("small"));
    }
    assert!(slot.lock().unwrap().is_none());
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _g = ActiveModelGuard::set(&slot, "base");
        panic!("session blew up");
    }));
    assert!(panicked.is_err());
    assert!(slot.lock().unwrap().is_none(), "a panic must not leave the model locked");
}

#[test]
fn a_model_held_by_a_guard_cannot_be_deleted() {
    let slot: Mutex<Option<String>> = Mutex::new(None);
    let dir = std::env::temp_dir().join(format!("ps-whisper-guard-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let model = &WHISPER_MODELS[0];
    std::fs::write(dir.join(model.asset.file), b"x").unwrap();
    {
        let _g = ActiveModelGuard::set(&slot, model.id);
        assert!(delete_model_in(&dir, model, &slot).is_err());
        assert!(dir.join(model.asset.file).exists());
    }
    assert!(delete_model_in(&dir, model, &slot).is_ok());
    assert!(!dir.join(model.asset.file).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------- Test hooks ----------

#[test]
fn the_test_hooks_refuse_without_the_start_up_variable() {
    use crate::whisper_cmds::*;
    // The unit-test process never ran the start-up read with the variable set.
    assert_eq!(require_test_hooks(false), Err(TEST_HOOKS_OFF.to_string()));
    assert!(require_test_hooks(true).is_ok());
    assert_eq!(whisper_transcribe_file("none.wav".to_string(), None, None), Err(TEST_HOOKS_OFF.to_string()));
    assert_eq!(whisper_test_corrupt_next_download(), Err(TEST_HOOKS_OFF.to_string()));
    assert!(!WHISPER_CORRUPT_NEXT.load(Ordering::SeqCst), "a refused hook must not arm anything");
}

#[test]
fn an_armed_download_arrives_damaged_once_and_the_hash_rejects_it() {
    use std::io::{Cursor, Read};
    let body: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
    let dir = std::env::temp_dir().join(format!("ps-whisper-corrupt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let hash: &'static str = Box::leak(hex_lower(&sha256(&body)).into_boxed_str());
    let asset = WhisperAsset { file: "m.bin", url: "https://example.invalid/m.bin", bytes: body.len() as u64, sha256: hash };
    let armed = AtomicBool::new(true);
    let served = body.clone();
    let open = |_: &str| -> Result<WhisperBody, String> {
        let plain: WhisperBody = (Box::new(Cursor::new(served.clone())) as Box<dyn Read>, Some(served.len() as u64));
        Ok(maybe_corrupt(plain, &armed))
    };
    let cancel = AtomicBool::new(false);
    assert_eq!(fetch_asset(&asset, &dir, &cancel, &open, &mut |_, _| {}).unwrap_err(), WHISPER_ERR_DAMAGED);
    assert!(!dir.join("m.bin").exists());
    assert!(!dir.join("m.bin.part").exists());
    assert!(!armed.load(Ordering::SeqCst), "the damage happens once");
    // The next download is clean again.
    assert!(fetch_asset(&asset, &dir, &cancel, &open, &mut |_, _| {}).is_ok());
    assert_eq!(std::fs::read(dir.join("m.bin")).unwrap(), body);
    let _ = std::fs::remove_dir_all(&dir);
}

// The library panics when voice-activity detection is enabled before its model path is set;
// the transcriber must not hit that on any phrase.
#[cfg(feature = "whisper")]
#[test]
fn voice_activity_detection_is_set_up_without_a_panic() {
    use whisper_rs::{FullParams, SamplingStrategy};
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    crate::whisper_engine::real::apply_vad(&mut params, "silero.bin");
}

// ---------- Live preview ----------

type Log = Arc<Mutex<Vec<(&'static str, String)>>>;

// Answers a preview with `preview` and a finished phrase with `last`.
struct Previewing {
    preview: &'static str,
    last: &'static str,
}

impl Transcribe for Previewing {
    fn transcribe(&mut self, _: &[f32], _: Option<&str>) -> Result<String, String> {
        Ok(self.last.to_string())
    }
    fn transcribe_cancellable(&mut self, _: &[f32], _: Option<&str>, _: &Arc<AtomicBool>) -> Result<String, String> {
        Ok(self.preview.to_string())
    }
}

fn run_with_previews(clip: Vec<f32>, engine: Previewing, interim: bool) -> (SessionOutcome, Vec<(&'static str, String)>) {
    let (tx, rx) = mpsc::channel();
    let log: Log = Arc::default();
    let (l1, l2) = (log.clone(), log.clone());
    let on_text: TextSink = Arc::new(move |t| l1.lock().unwrap().push(("text", t)));
    let on_partial: TextSink = Arc::new(move |t| l2.lock().unwrap().push(("partial", t)));
    // 100 ms pieces with a short real pause each, so the worker gets idle moments between them.
    let mut q: VecDeque<Vec<f32>> = clip.chunks(1600).map(|c| c.to_vec()).collect();
    let mut feed = move || -> Result<Vec<f32>, &'static str> {
        std::thread::sleep(Duration::from_millis(3));
        match q.pop_front() {
            Some(c) => Ok(c),
            None => {
                let _ = tx.send("manual");
                Ok(Vec::new())
            }
        }
    };
    let mut cfg = config(0, 600);
    cfg.interim = interim;
    let outcome = run_pipeline_with(&cfg, Box::new(engine), &mut feed, &rx, &Arc::new(AtomicBool::new(false)), on_text, Some(on_partial));
    let out = log.lock().unwrap().clone();
    (outcome, out)
}

#[test]
fn a_preview_shows_while_speaking_and_the_final_text_follows_once() {
    let clip = cat(&[quiet(0.5), talk(4.0), quiet(1.0)]);
    let (outcome, log) = run_with_previews(clip, Previewing { preview: "so far", last: "all of it" }, true);
    assert_eq!(outcome.phrases, 1);
    let finals: Vec<usize> = log.iter().enumerate().filter(|(_, e)| e.0 == "text").map(|(i, _)| i).collect();
    assert_eq!(finals.len(), 1, "{log:?}");
    assert_eq!(log[finals[0]].1, "all of it");
    let previews = log.iter().filter(|e| e.0 == "partial" && e.1 == "so far").count();
    assert!(previews >= 1, "no preview before the final: {log:?}");
    // Nothing of that phrase comes after its final text.
    assert!(log[finals[0] + 1..].iter().all(|e| e.0 != "partial" || e.1.is_empty()), "{log:?}");
}

#[test]
fn without_the_setting_there_is_no_preview() {
    let clip = cat(&[quiet(0.5), talk(4.0), quiet(1.0)]);
    let (_, log) = run_with_previews(clip, Previewing { preview: "so far", last: "all of it" }, false);
    assert_eq!(log, vec![("text", "all of it".to_string())]);
}

#[test]
fn a_preview_whose_phrase_holds_no_text_is_cleared() {
    let clip = cat(&[quiet(0.5), talk(3.0), quiet(1.0)]);
    let (outcome, log) = run_with_previews(clip, Previewing { preview: "noise words", last: "" }, true);
    assert_eq!(outcome.phrases, 0);
    assert!(log.iter().any(|e| e.1 == "noise words"), "fixture: no preview shown: {log:?}");
    assert_eq!(log.last(), Some(&("partial", String::new())), "{log:?}");
    assert!(log.iter().all(|e| e.0 != "text"));
}

#[test]
fn what_arrived_just_before_a_manual_stop_is_still_transcribed() {
    // The stop is already waiting when the loop looks for it, so the speech handed over by the
    // same poll would be lost without one last pull.
    let (tx, rx) = mpsc::channel();
    let (texts, sink) = collector();
    let mut n = 0;
    let mut feed = move || -> Result<Vec<f32>, &'static str> {
        n += 1;
        match n {
            1 => {
                let _ = tx.send("manual");
                Ok(quiet(0.3))
            }
            2 => Ok(tone(1.0, 0.3)),
            _ => Ok(Vec::new()),
        }
    };
    let outcome = run_pipeline(&config(0, 600), Box::new(Numbered { calls: Arc::default(), fail: false }), &mut feed, &rx, &Arc::new(AtomicBool::new(false)), sink);
    assert_eq!(outcome.reason, "manual");
    assert_eq!(*texts.lock().unwrap(), vec!["phrase 1".to_string()]);
}
