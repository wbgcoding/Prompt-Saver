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
