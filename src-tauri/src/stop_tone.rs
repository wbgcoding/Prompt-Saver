// The short chime played when dictation ends by itself (the pause or the time limit), so a
// session that stops while the person looks elsewhere does not go unnoticed. It is synthesised
// once and played by Windows directly, so it sounds even when the window is in the background
// and at the moment listening ends, not once the last phrase has been written out.

use std::f32::consts::TAU;

const RATE: u32 = 44_100;
// Two bell-like notes falling a fifth, the second starting while the first rings out.
const NOTES: [(f32, f32); 2] = [(783.99, 0.0), (523.25, 0.13)];
const LENGTH_SECS: f32 = 0.75;
const PEAK: f32 = 0.22;

// Whether a session end reason gets the chime: only the ends nobody pressed a button for.
pub(crate) fn is_automatic_stop(reason: &str) -> bool {
    matches!(reason, "silence" | "max")
}

// The chime as 16-bit mono samples: soft attack, a little warmth from two overtones, and an
// exponential fade so it never clicks.
pub(crate) fn chime_samples() -> Vec<i16> {
    let n = (LENGTH_SECS * RATE as f32) as usize;
    let mut out = vec![0f32; n];
    for (freq, start) in NOTES {
        let first = (start * RATE as f32) as usize;
        for (i, s) in out.iter_mut().enumerate().skip(first) {
            let t = (i - first) as f32 / RATE as f32;
            let attack = (t / 0.006).min(1.0);
            let decay = (-t / 0.16).exp();
            let tone = (TAU * freq * t).sin() + 0.25 * (TAU * 2.0 * freq * t).sin() * (-t / 0.06).exp() + 0.08 * (TAU * 3.0 * freq * t).sin() * (-t / 0.04).exp();
            *s += attack * decay * tone;
        }
    }
    let loudest = out.iter().fold(0f32, |m, s| m.max(s.abs())).max(f32::EPSILON);
    let tail = (0.02 * RATE as f32) as usize;
    out.iter()
        .enumerate()
        .map(|(i, s)| {
            let fade = ((n - i) as f32 / tail as f32).min(1.0);
            (s / loudest * PEAK * fade * i16::MAX as f32) as i16
        })
        .collect()
}

// The chime as a complete WAV file in memory.
pub(crate) fn chime_wav() -> Vec<u8> {
    let samples = chime_samples();
    let data_len = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    wav
}

// Plays the chime without waiting for it. A missing output device just means no sound.
#[cfg(windows)]
pub(crate) fn play_stop_chime() {
    use windows::core::PCWSTR;
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    // Windows reads the buffer while the sound plays, so it lives for the whole run.
    static WAV: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let wav = WAV.get_or_init(chime_wav);
    unsafe {
        let _ = PlaySoundW(PCWSTR(wav.as_ptr().cast()), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

#[cfg(not(windows))]
pub(crate) fn play_stop_chime() {}
