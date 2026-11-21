// Audio side of local dictation, free of the speech engine itself so every piece runs in unit tests:
// capture format to 16 kHz mono, splitting a live stream into phrases at pauses, WAV reading for the
// file path, the language-tag mapping and the cleanup of an engine result.
use std::collections::VecDeque;

pub(crate) const WHISPER_RATE: u32 = 16_000;
// One analysis frame: 30 ms.
const FRAME: usize = 480;
// A phrase ends after this much silence (20 frames = 600 ms).
const PHRASE_GAP_FRAMES: usize = 20;
// A phrase never grows past this; a longer one is cut where it stands.
pub(crate) const PHRASE_MAX_SECS: usize = 25;
const PHRASE_MAX_SAMPLES: usize = PHRASE_MAX_SECS * WHISPER_RATE as usize;
// Quiet lead-in kept before the first loud frame so the start of a word is not clipped.
const PRE_ROLL_FRAMES: usize = 10;
// Silence kept after the last loud frame when the phrase is cut at a pause.
const TAIL_KEEP_FRAMES: usize = 5;
// A phrase with fewer loud frames than this (about 200 ms) is a click or a bump, not speech.
const MIN_SPEECH_FRAMES: usize = 7;
// A frame is loud above this level and above the tracked background level times the margin.
const SPEECH_FLOOR_RMS: f32 = 0.008;
const NOISE_MARGIN: f32 = 3.0;

// The capture queue's fixed format (see rec_engine): 48 kHz interleaved stereo. Three input
// frames make one output sample, so the conversion is an exact average.
const CAPTURE_RATE: u32 = crate::REC_AUDIO_SAMPLE_RATE;
const CAPTURE_CHANNELS: u32 = crate::REC_AUDIO_CHANNELS;
const CAPTURE_GROUP: usize = (CAPTURE_RATE / WHISPER_RATE * CAPTURE_CHANNELS) as usize;

// Converts captured 48 kHz stereo i16 into 16 kHz mono f32. A partial group at the end of a chunk
// waits in `carry` for the next call, so the stream stays aligned across any chunk size.
pub(crate) fn mono16k_from_capture(pcm: &[i16], carry: &mut Vec<i16>) -> Vec<f32> {
    carry.extend_from_slice(pcm);
    let usable = carry.len() / CAPTURE_GROUP * CAPTURE_GROUP;
    let out = carry[..usable]
        .chunks(CAPTURE_GROUP)
        .map(|g| g.iter().map(|&s| s as f32).sum::<f32>() / (CAPTURE_GROUP as f32 * 32768.0))
        .collect();
    carry.drain(..usable);
    out
}

// Linear resampling of a mono signal to 16 kHz, for audio files at other rates.
pub(crate) fn resample_to_16k(mono: &[f32], rate: u32) -> Vec<f32> {
    if rate == WHISPER_RATE || rate == 0 || mono.is_empty() {
        return mono.to_vec();
    }
    let ratio = rate as f64 / WHISPER_RATE as f64;
    let out_len = (mono.len() as f64 / ratio).floor() as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let at = pos.floor() as usize;
            let frac = (pos - at as f64) as f32;
            let a = mono[at.min(mono.len() - 1)];
            let b = mono[(at + 1).min(mono.len() - 1)];
            a + (b - a) * frac
        })
        .collect()
}

fn frame_rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt()
}

// Cuts a live 16 kHz mono stream into phrases. Feed it samples in any chunk size; every finished
// phrase comes back from `push` (a pause of 600 ms ends one) or from `flush` at the end of the session.
pub(crate) struct Segmenter {
    pending: Vec<f32>,
    pre_roll: VecDeque<Vec<f32>>,
    phrase: Vec<f32>,
    in_phrase: bool,
    speech_frames: usize,
    trailing_silence: usize,
    floor: f32,
    last_frame_loud: bool,
}

impl Segmenter {
    pub(crate) fn new() -> Self {
        Self {
            pending: Vec::new(),
            pre_roll: VecDeque::new(),
            phrase: Vec::new(),
            in_phrase: false,
            speech_frames: 0,
            trailing_silence: 0,
            floor: 0.0,
            last_frame_loud: false,
        }
    }

    // Whether the most recent frame was loud: the session's idle clock restarts while it is true.
    pub(crate) fn speaking(&self) -> bool {
        self.last_frame_loud
    }

    pub(crate) fn push(&mut self, samples: &[f32]) -> Vec<Vec<f32>> {
        let mut done = Vec::new();
        self.pending.extend_from_slice(samples);
        while self.pending.len() >= FRAME {
            let frame: Vec<f32> = self.pending.drain(..FRAME).collect();
            self.frame(frame, &mut done);
        }
        done
    }

    // The phrase still in progress, if it holds speech.
    pub(crate) fn flush(&mut self) -> Option<Vec<f32>> {
        let mut done = Vec::new();
        if self.in_phrase {
            self.finish(&mut done);
        }
        self.pending.clear();
        done.pop()
    }

    fn frame(&mut self, frame: Vec<f32>, done: &mut Vec<Vec<f32>>) {
        let rms = frame_rms(&frame);
        let loud = rms > SPEECH_FLOOR_RMS.max(self.floor * NOISE_MARGIN);
        self.last_frame_loud = loud;
        if !loud && !self.in_phrase {
            // Quiet and between phrases: track the background level and keep the lead-in.
            self.floor = self.floor * 0.98 + rms * 0.02;
            self.pre_roll.push_back(frame);
            while self.pre_roll.len() > PRE_ROLL_FRAMES {
                self.pre_roll.pop_front();
            }
            return;
        }
        if loud && !self.in_phrase {
            self.in_phrase = true;
            self.speech_frames = 0;
            self.trailing_silence = 0;
            self.phrase = self.pre_roll.drain(..).flatten().collect();
        }
        self.phrase.extend_from_slice(&frame);
        if loud {
            self.speech_frames += 1;
            self.trailing_silence = 0;
        } else {
            self.trailing_silence += 1;
            if self.trailing_silence >= PHRASE_GAP_FRAMES {
                self.finish(done);
                return;
            }
        }
        if self.phrase.len() >= PHRASE_MAX_SAMPLES {
            self.finish(done);
        }
    }

    fn finish(&mut self, done: &mut Vec<Vec<f32>>) {
        let drop_frames = self.trailing_silence.saturating_sub(TAIL_KEEP_FRAMES);
        let keep = self.phrase.len().saturating_sub(drop_frames * FRAME);
        self.phrase.truncate(keep);
        let phrase = std::mem::take(&mut self.phrase);
        if self.speech_frames >= MIN_SPEECH_FRAMES {
            done.push(phrase);
        }
        self.in_phrase = false;
        self.speech_frames = 0;
        self.trailing_silence = 0;
        self.pre_roll.clear();
    }
}

// What the engine does with one phrase; the real one wraps whisper.cpp, tests use a fake.
pub(crate) trait Transcribe {
    fn transcribe(&mut self, samples: &[f32], lang: Option<&str>) -> Result<String, String>;
}

// The file path of the pipeline: all samples through the segmenter, every phrase through the
// engine, the non-empty results in order.
pub(crate) fn transcribe_samples(t: &mut dyn Transcribe, mono16k: &[f32], lang: Option<&str>) -> Result<Vec<String>, String> {
    let mut seg = Segmenter::new();
    let mut phrases = Vec::new();
    for chunk in mono16k.chunks(FRAME * 10) {
        phrases.extend(seg.push(chunk));
    }
    phrases.extend(seg.flush());
    let mut out = Vec::new();
    for phrase in phrases {
        let text = clean_text(&t.transcribe(&phrase, lang)?);
        if !text.is_empty() {
            out.push(text);
        }
    }
    Ok(out)
}

// Engine output to the text that is inserted: trimmed, and empty for the placeholder the engine
// prints for audio with no speech ("[BLANK_AUDIO]", "(silence)", "*music*").
pub(crate) fn clean_text(raw: &str) -> String {
    let t = raw.trim();
    let wrapped = |open: char, close: char| t.len() > 1 && t.starts_with(open) && t.ends_with(close);
    if wrapped('[', ']') || wrapped('(', ')') || wrapped('*', '*') || wrapped('\u{266a}', '\u{266a}') {
        return String::new();
    }
    t.to_string()
}

// The engine's language codes.
const WHISPER_LANGS: [&str; 100] = [
    "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv", "it", "id", "hi", "fi", "vi",
    "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no", "th", "ur", "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk",
    "te", "fa", "lv", "bn", "sr", "az", "sl", "kn", "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw",
    "gl", "mr", "pa", "si", "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo",
    "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln", "ha", "ba", "jw", "su", "yue",
];

// BCP-47 tag to the engine's language code. "auto", an empty tag and anything the engine does not
// know give None, which means it detects the language itself.
pub(crate) fn whisper_lang(tag: &str) -> Option<&'static str> {
    let primary = tag.split(['-', '_']).next().unwrap_or("").trim().to_ascii_lowercase();
    let primary = match primary.as_str() {
        "nb" => "no",
        "iw" => "he",
        "jv" => "jw",
        "fil" => "tl",
        other => other,
    };
    WHISPER_LANGS.iter().copied().find(|c| *c == primary)
}

// Worker threads for the engine: the physical cores, at most 8 (more only add contention).
pub(crate) fn engine_threads(physical_cores: u32) -> i32 {
    physical_cores.clamp(1, 8) as i32
}

// Whether a loaded model is past its idle time and should be freed.
pub(crate) fn model_expired(idle: std::time::Duration, keep: std::time::Duration) -> bool {
    idle >= keep
}

const MAX_WAV_BYTES: usize = 200 * 1024 * 1024;

// Reads a RIFF/WAVE file (8/16/24-bit PCM or 32-bit float, any channel count) into mono f32 and
// its sample rate.
pub(crate) fn parse_wav(bytes: &[u8]) -> Result<(u32, Vec<f32>), String> {
    if bytes.len() > MAX_WAV_BYTES {
        return Err("wav-too-large".to_string());
    }
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("wav-invalid".to_string());
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let (mut format, mut channels, mut rate, mut bits) = (0u16, 0u16, 0u32, 0u16);
    let mut data: Option<&[u8]> = None;
    let mut at = 12usize;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(at + 4) as usize;
        let body = at + 8;
        let end = body.saturating_add(size).min(bytes.len());
        if id == b"fmt " && end - body >= 16 {
            format = u16_at(body);
            channels = u16_at(body + 2);
            rate = u32_at(body + 4);
            bits = u16_at(body + 14);
            // WAVE_FORMAT_EXTENSIBLE: the real format tag leads the sub-format GUID.
            if format == 0xFFFE && end - body >= 26 {
                format = u16_at(body + 24);
            }
        } else if id == b"data" {
            data = Some(&bytes[body..end]);
            break;
        }
        at = body.saturating_add(size).saturating_add(size & 1);
    }
    let data = data.ok_or_else(|| "wav-invalid".to_string())?;
    if channels == 0 || rate == 0 {
        return Err("wav-invalid".to_string());
    }
    if !matches!((format, bits), (1, 8) | (1, 16) | (1, 24) | (3, 32)) {
        return Err("wav-unsupported".to_string());
    }
    let sample = |chunk: &[u8]| -> f32 {
        match bits {
            8 => (chunk[0] as f32 - 128.0) / 128.0,
            16 => i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0,
            24 => (i32::from_le_bytes([0, chunk[0], chunk[1], chunk[2]]) >> 8) as f32 / 8_388_608.0,
            _ => f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
        }
    };
    let width = (bits / 8) as usize;
    let mono = data
        .chunks_exact(width * channels as usize)
        .map(|frame| frame.chunks_exact(width).map(sample).sum::<f32>() / channels as f32)
        .collect();
    Ok((rate, mono))
}
