// Accuracy check of the local speech recognition on recorded speech. Needs the model files and
// WAV clips on disk, so it is ignored by default and never touches the network:
//
//   WHISPER_EVAL_DIR=<dir> cargo test --lib whisper_eval -- --ignored --nocapture
//
// <dir>/models holds the pinned model files (and the voice-activity file), <dir>/clips holds
// pairs of <name>.wav and <name>.txt (the spoken text); a name starting with "de" is German,
// "en" English. The word error rate is printed per model and language.

use std::fs;
use std::path::{Path, PathBuf};

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

// Word-level edit distance (substitutions, insertions, deletions).
fn edit_distance(a: &[String], b: &[String]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let sub = prev[j - 1] + usize::from(a[i - 1] != b[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

#[test]
fn word_error_rate_counts_each_kind_of_mistake() {
    let w = |s: &str| words(s);
    assert_eq!(edit_distance(&w("a b c"), &w("a b c")), 0);
    assert_eq!(edit_distance(&w("a b c"), &w("a x c")), 1);
    assert_eq!(edit_distance(&w("a b c"), &w("a c")), 1);
    assert_eq!(edit_distance(&w("a b c"), &w("a b x c")), 1);
    assert_eq!(words("Ten o'clock, E-Mail!"), ["ten", "o", "clock", "e", "mail"]);
}

// The models live where the app looks for them: <local app data>/<app folder>/whisper.
fn stage_models(eval: &Path) -> PathBuf {
    let base = eval.join("la");
    let dir = base.join(crate::brand::DATA_FOLDER).join("whisper");
    fs::create_dir_all(&dir).expect("model folder");
    for entry in fs::read_dir(eval.join("models")).expect("models folder").flatten() {
        let to = dir.join(entry.file_name());
        if !to.exists() {
            fs::hard_link(entry.path(), &to).or_else(|_| fs::copy(entry.path(), &to).map(|_| ())).expect("stage model");
        }
    }
    std::env::set_var("LOCALAPPDATA", &base);
    dir
}

#[cfg(feature = "whisper")]
#[test]
#[ignore = "needs model files and clips on disk (WHISPER_EVAL_DIR)"]
fn whisper_eval_word_error_rate_per_model_and_language() {
    let Some(eval) = std::env::var_os("WHISPER_EVAL_DIR").map(PathBuf::from) else {
        panic!("set WHISPER_EVAL_DIR to a folder with models/ and clips/");
    };
    stage_models(&eval);
    let mut clips: Vec<(String, Vec<u8>, String)> = Vec::new();
    for entry in fs::read_dir(eval.join("clips")).expect("clips folder").flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "wav") {
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            let text = fs::read_to_string(path.with_extension("txt")).expect("reference text");
            clips.push((name, fs::read(&path).expect("clip"), text));
        }
    }
    clips.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!clips.is_empty(), "no clips found");
    for id in ["base", "small", "turbo"] {
        let model = crate::whisper::whisper_model(id).expect("catalog model");
        for lang in ["de", "en"] {
            let (mut errors, mut total) = (0usize, 0usize);
            for (name, wav, reference) in clips.iter().filter(|c| c.0.starts_with(lang)) {
                let started = std::time::Instant::now();
                let phrases = crate::whisper_engine::real::transcribe_file(model, wav, Some(lang)).expect("transcribe");
                let heard = phrases.join(" ");
                let (r, h) = (words(reference), words(&heard));
                let e = edit_distance(&r, &h);
                errors += e;
                total += r.len();
                println!("[{id} {name}] {e}/{} errors, {} ms: {}", r.len(), started.elapsed().as_millis(), heard.trim());
            }
            println!("WER {id} {lang}: {:.1} % ({errors}/{total})", 100.0 * errors as f64 / total.max(1) as f64);
        }
    }
}

// A copy of the model folder without the voice-activity file, to compare both.
fn stage_without_vad(eval: &Path, with_vad: &Path) -> PathBuf {
    let dir = eval.join("novad");
    fs::create_dir_all(&dir).expect("folder");
    for entry in fs::read_dir(with_vad).expect("models").flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().contains("silero") && !dir.join(&name).exists() {
            fs::hard_link(entry.path(), dir.join(&name)).expect("stage model");
        }
    }
    dir
}

// The clip over a steady background hiss, like a cheap headset.
fn with_hiss(clean: &[f32]) -> Vec<f32> {
    let mut x: u32 = 0x9e37_79b9;
    clean
        .iter()
        .map(|s| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            s + 0.012 * 1.732 * ((x as f32 / u32::MAX as f32) * 2.0 - 1.0)
        })
        .collect()
}

// Live behaviour on recorded speech: each clip is fed at real speed through the same pipeline as
// a session (phrase splitting, live preview, final text), once clean and once over background
// hiss, with and without the voice-activity file. Prints when each preview and each final text
// arrived, so lost sentences and slow previews show up.
//
//   WHISPER_EVAL_DIR=<dir> cargo test --lib whisper_live_replay -- --ignored --nocapture
//
// WHISPER_EVAL_MODEL picks the model (default small), WHISPER_EVAL_ONLY filters clip names.
#[cfg(feature = "whisper")]
#[test]
#[ignore = "needs model files and clips on disk (WHISPER_EVAL_DIR)"]
fn whisper_live_replay() {
    use crate::whisper_engine::{run_pipeline_with, LiveHooks, SessionConfig, TextSink};
    use std::sync::atomic::AtomicBool;
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::{Duration, Instant};
    let Some(eval) = std::env::var_os("WHISPER_EVAL_DIR").map(PathBuf::from) else {
        panic!("set WHISPER_EVAL_DIR to a folder with models/ and clips/");
    };
    let with_vad = stage_models(&eval);
    let without_vad = stage_without_vad(&eval, &with_vad);
    let model_id = std::env::var("WHISPER_EVAL_MODEL").unwrap_or_else(|_| "small".into());
    let model = crate::whisper::whisper_model(&model_id).expect("catalog model");
    let only = std::env::var("WHISPER_EVAL_ONLY").unwrap_or_default();
    let mut clips: Vec<PathBuf> = fs::read_dir(eval.join("clips"))
        .expect("clips")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "wav") && only.split('|').any(|o| p.to_string_lossy().contains(o)))
        .collect();
    clips.sort();
    for clip in &clips {
        let (rate, mono) = crate::whisper_audio::parse_wav(&fs::read(clip).expect("clip")).expect("wav");
        let clean = crate::whisper_audio::resample_to_16k(&mono, rate);
        let hiss = with_hiss(&clean);
        let reference = fs::read_to_string(clip.with_extension("txt")).unwrap_or_default();
        for (noise, audio) in [("clean", &clean), ("hiss", &hiss)] {
            for (vad, dir) in [("vad", &with_vad), ("novad", &without_vad)] {
                let engine = crate::whisper_engine::real::build_engine(model, dir, Arc::new(AtomicBool::new(false))).expect("engine");
                let t0 = Instant::now();
                let log: Arc<Mutex<Vec<String>>> = Arc::default();
                let finals: Arc<Mutex<Vec<String>>> = Arc::default();
                let (l1, f1, l2) = (log.clone(), finals.clone(), log.clone());
                let on_text: TextSink = Arc::new(move |t: String| {
                    l1.lock().unwrap().push(format!("{:>6} ms FINAL   {t}", t0.elapsed().as_millis()));
                    f1.lock().unwrap().push(t);
                });
                let on_partial: TextSink = Arc::new(move |t: String| {
                    l2.lock().unwrap().push(format!("{:>6} ms preview {t}", t0.elapsed().as_millis()));
                });
                let total = audio.len();
                let audio = audio.clone();
                let mut fed = 0usize;
                let mut pull = move || -> Result<Vec<f32>, &'static str> {
                    let due = ((t0.elapsed().as_secs_f64() * 16_000.0) as usize).min(total);
                    let out = audio[fed..due].to_vec();
                    fed = due;
                    Ok(out)
                };
                let (stop_tx, stop_rx) = mpsc::channel();
                let clip_ms = (total as u64 * 1000) / 16_000;
                // The stop button, pressed right after the clip; the sender stays alive so the
                // session drains what is queued instead of treating it as a shutdown.
                let stopper = std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(clip_ms + 200));
                    let _ = stop_tx.send("manual");
                    std::thread::sleep(Duration::from_secs(120));
                });
                let cfg = SessionConfig { lang: Some("de"), auto_stop_sec: 0, max_sec: 600, poll: Duration::from_millis(100), interim: true };
                let outcome = run_pipeline_with(&cfg, engine, &mut pull, &stop_rx, &Arc::new(AtomicBool::new(false)), on_text, LiveHooks { partial: Some(on_partial), ..Default::default() });
                drop(stopper);
                let heard = finals.lock().unwrap().join(" ");
                let e = edit_distance(&words(&reference), &words(&heard));
                println!(
                    "=== {} {noise} {vad}: {} phrases, {e}/{} word errors, done {} ms after the stop",
                    clip.file_stem().unwrap().to_string_lossy(),
                    outcome.phrases,
                    words(&reference).len(),
                    t0.elapsed().as_millis().saturating_sub(clip_ms as u128 + 200)
                );
                for line in log.lock().unwrap().iter() {
                    println!("  {line}");
                }
            }
        }
    }
}
