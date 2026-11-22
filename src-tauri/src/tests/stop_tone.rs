use crate::stop_tone::*;

#[test]
fn only_the_ends_nobody_asked_for_get_the_chime() {
    assert!(is_automatic_stop("silence"));
    assert!(is_automatic_stop("max"));
    for reason in ["manual", "error", "shutdown", ""] {
        assert!(!is_automatic_stop(reason), "{reason}");
    }
}

#[test]
fn the_chime_is_short_soft_and_ends_in_silence() {
    let s = chime_samples();
    // 0.75 s at 44.1 kHz.
    assert_eq!(s.len(), 33_075);
    let loudest = s.iter().map(|v| v.unsigned_abs()).max().unwrap();
    // Never louder than about a fifth of full scale.
    assert!(loudest > 6_000 && loudest < 7_500, "{loudest}");
    // Starts and ends at (near) zero, so it never clicks.
    assert!(s[0].unsigned_abs() < 50, "{}", s[0]);
    assert!(s[s.len() - 1].unsigned_abs() < 50, "{}", s[s.len() - 1]);
    // The second note is still sounding near the end of its first half.
    let late = &s[s.len() / 2..s.len() / 2 + 2_000];
    assert!(late.iter().any(|v| v.unsigned_abs() > 500));
}

#[test]
fn the_chime_is_a_valid_wav_file() {
    let wav = chime_wav();
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[8..16], b"WAVEfmt ");
    assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()) as usize, wav.len() - 8);
    assert_eq!(u16::from_le_bytes(wav[20..22].try_into().unwrap()), 1, "PCM");
    assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1, "mono");
    assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 44_100);
    assert_eq!(&wav[36..40], b"data");
    assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize, chime_samples().len() * 2);
    // The same bytes the parser of the speech engine accepts.
    let (rate, mono) = crate::whisper_audio::parse_wav(&wav).expect("parses");
    assert_eq!((rate, mono.len()), (44_100, 33_075));
}
