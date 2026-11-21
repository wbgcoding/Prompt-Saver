// Local dictation session: the loop that turns the microphone into phrases and the phrases into
// `voice-text` events, the cache that keeps the speech model in memory for a while after a session,
// and the engine that wraps whisper.cpp. Everything except the engine itself runs in unit tests
// with a fake one.
#[cfg_attr(not(feature = "whisper"), allow(unused_imports))]
use super::*;
use crate::whisper_audio::*;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

// How long the model stays loaded after the last session before it is freed.
pub(crate) const MODEL_KEEP: Duration = Duration::from_secs(5 * 60);
// How often the session loop looks at the microphone queue and the stop channel. The capture
// queue holds 2 s, so this stays far below that.
pub(crate) const SESSION_POLL: Duration = Duration::from_millis(100);

// ---------- Model cache ----------

struct CacheEntry<T> {
    id: String,
    model: Arc<T>,
    // None while a session uses the model; the moment the last one ended otherwise.
    idle_since: Option<Instant>,
}

struct CacheState<T> {
    entry: Option<CacheEntry<T>>,
    // A release timer is already waiting, so no second one is started.
    sweeper: bool,
}

pub(crate) enum Sweep {
    // Nothing left to watch: the model is gone (or was never there).
    Done,
    // Look again after this long.
    Wait(Duration),
}

// One loaded model, shared by consecutive sessions. Generic so the timing rules are tested with a
// stand-in instead of a real model file.
pub(crate) struct ModelCache<T> {
    inner: Mutex<CacheState<T>>,
}

impl<T> ModelCache<T> {
    pub(crate) const fn new() -> Self {
        Self { inner: Mutex::new(CacheState { entry: None, sweeper: false }) }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, CacheState<T>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    // The loaded model with this id (marking it in use), or the one `load` builds. A different
    // model that is loaded is freed first, so two are never in memory at once.
    pub(crate) fn acquire(&self, id: &str, load: impl FnOnce() -> Result<T, String>) -> Result<Arc<T>, String> {
        let mut s = self.state();
        if let Some(e) = s.entry.as_mut() {
            if e.id == id {
                e.idle_since = None;
                return Ok(e.model.clone());
            }
        }
        s.entry = None;
        let model = Arc::new(load()?);
        s.entry = Some(CacheEntry { id: id.to_string(), model: model.clone(), idle_since: None });
        Ok(model)
    }

    // The session is over. True when the caller has to start the release timer.
    pub(crate) fn release(&self, now: Instant) -> bool {
        let mut s = self.state();
        if let Some(e) = s.entry.as_mut() {
            e.idle_since = Some(now);
        }
        if s.entry.is_some() && !s.sweeper {
            s.sweeper = true;
            return true;
        }
        false
    }

    // The release timer's check: frees the model once it has been idle for `keep`.
    pub(crate) fn sweep(&self, now: Instant, keep: Duration) -> Sweep {
        let mut s = self.state();
        let Some(e) = s.entry.as_ref() else {
            s.sweeper = false;
            return Sweep::Done;
        };
        match e.idle_since {
            None => Sweep::Wait(keep),
            Some(since) => {
                let idle = now.saturating_duration_since(since);
                if model_expired(idle, keep) {
                    s.entry = None;
                    s.sweeper = false;
                    Sweep::Done
                } else {
                    Sweep::Wait(keep - idle)
                }
            }
        }
    }

    pub(crate) fn clear(&self) {
        let mut s = self.state();
        s.entry = None;
        s.sweeper = false;
    }

    #[cfg(test)]
    pub(crate) fn is_loaded(&self) -> bool {
        self.state().entry.is_some()
    }
}

// Starts the timer that frees the model `keep` after its last use. A refused thread frees it at
// once instead of leaving a model nothing would ever release.
pub(crate) fn spawn_sweeper<T: Send + Sync + 'static>(cache: &'static ModelCache<T>, keep: Duration) {
    let spawned = std::thread::Builder::new().name("whisper-release".into()).spawn(move || {
        let mut wait = keep;
        loop {
            std::thread::sleep(wait);
            match cache.sweep(Instant::now(), keep) {
                Sweep::Done => break,
                Sweep::Wait(d) => wait = d,
            }
        }
    });
    if spawned.is_err() {
        cache.clear();
    }
}

// ---------- Active model marker ----------

// Holds the "this model is in use" marker for as long as it lives, so a delete cannot take the
// file out from under a session or a file transcription, whichever way that ends.
pub(crate) struct ActiveModelGuard<'a> {
    slot: &'a Mutex<Option<String>>,
}

impl<'a> ActiveModelGuard<'a> {
    pub(crate) fn set(slot: &'a Mutex<Option<String>>, id: &str) -> Self {
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(id.to_string());
        Self { slot }
    }
}

impl Drop for ActiveModelGuard<'_> {
    fn drop(&mut self) {
        *self.slot.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

// ---------- Session loop ----------

pub(crate) struct SessionConfig {
    // The engine's language code; None lets it detect the language.
    pub(crate) lang: Option<&'static str>,
    // 0 = off; otherwise the pause without speech that ends the session.
    pub(crate) auto_stop_sec: u32,
    pub(crate) max_sec: u32,
    pub(crate) poll: Duration,
}

pub(crate) struct SessionOutcome {
    // manual | silence | max | error | shutdown, the same words the window already knows.
    pub(crate) reason: &'static str,
    // How many phrases produced text.
    pub(crate) phrases: u32,
    pub(crate) error: Option<&'static str>,
}

// The session proper. `pull` hands over the 16 kHz mono samples that arrived since its last
// call (Err = the microphone is gone); `engine` runs on a worker thread so listening goes on
// while a phrase is transcribed; `on_text` gets every non-empty result in order.
//
// A manual stop, the pause limit and the time limit end the listening but let the phrase in
// progress and the ones still queued come out. A shutdown, or a sender that went away,
// abandons them and sets `abort`, which the engine watches so even a long phrase ends quickly.
pub(crate) fn run_pipeline(
    cfg: &SessionConfig,
    mut engine: Box<dyn Transcribe + Send>,
    pull: &mut dyn FnMut() -> Result<Vec<f32>, &'static str>,
    stop_rx: &mpsc::Receiver<&'static str>,
    abort: &Arc<AtomicBool>,
    on_text: Arc<dyn Fn(String) + Send + Sync>,
) -> SessionOutcome {
    let (phrase_tx, phrase_rx) = mpsc::channel::<Vec<f32>>();
    let (err_tx, err_rx) = mpsc::channel::<&'static str>();
    let heard = Arc::new(AtomicU32::new(0));
    let (w_abort, w_heard, lang) = (abort.clone(), heard.clone(), cfg.lang);
    let spawned = std::thread::Builder::new().name("whisper-worker".into()).spawn(move || {
        for phrase in phrase_rx {
            if w_abort.load(Ordering::SeqCst) {
                break;
            }
            match engine.transcribe(&phrase, lang) {
                Ok(raw) => {
                    let text = clean_text(&raw);
                    if !text.is_empty() && !w_abort.load(Ordering::SeqCst) {
                        w_heard.fetch_add(1, Ordering::SeqCst);
                        on_text(text);
                    }
                }
                Err(_) => {
                    let _ = err_tx.send("voice-failed");
                    break;
                }
            }
        }
    });
    let Ok(worker) = spawned else {
        return SessionOutcome { reason: "error", phrases: 0, error: Some("voice-failed") };
    };

    let started = Instant::now();
    let mut idle_since = started;
    let mut segmenter = Segmenter::new();
    let mut loop_error: Option<&'static str> = None;
    let reason: &'static str = loop {
        match stop_rx.recv_timeout(cfg.poll) {
            Ok(r) => break r,
            Err(mpsc::RecvTimeoutError::Disconnected) => break "shutdown",
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if err_rx.try_recv().is_ok() {
            // The worker reported its failure; it is read again after the wait below.
            break "error";
        }
        match pull() {
            Ok(samples) => {
                for phrase in segmenter.push(&samples) {
                    let _ = phrase_tx.send(phrase);
                }
                if segmenter.speaking() {
                    idle_since = Instant::now();
                }
            }
            Err(key) => {
                loop_error = Some(key);
                break "error";
            }
        }
        if let Some(r) = crate::voice::should_stop(idle_since.elapsed(), started.elapsed(), cfg.auto_stop_sec, cfg.max_sec) {
            break r;
        }
    };

    if matches!(reason, "manual" | "silence" | "max") {
        if let Some(last) = segmenter.flush() {
            let _ = phrase_tx.send(last);
        }
    } else {
        abort.store(true, Ordering::SeqCst);
    }
    drop(phrase_tx);
    // Let the worker finish what is queued; a shutdown that arrives meanwhile cuts it short.
    let mut shutdown = reason == "shutdown";
    while !worker.is_finished() {
        match stop_rx.recv_timeout(cfg.poll) {
            Ok("shutdown") | Err(mpsc::RecvTimeoutError::Disconnected) => {
                shutdown = true;
                abort.store(true, Ordering::SeqCst);
                break;
            }
            _ => {}
        }
    }
    let worker_panicked = worker.join().is_err();
    let worker_failed = worker_panicked || err_rx.try_recv().is_ok() || (reason == "error" && loop_error.is_none());
    let (reason, error) = if let Some(key) = loop_error {
        ("error", Some(key))
    } else if shutdown {
        // The engine failing to finish an aborted phrase is not an error.
        ("shutdown", None)
    } else if worker_failed {
        ("error", Some("voice-failed"))
    } else {
        (reason, None)
    };
    SessionOutcome { reason, phrases: heard.load(Ordering::SeqCst), error }
}

// The file path of the pipeline, used by the test hook and by the unit tests: a WAV through
// the same phrase splitting and engine as a live session.
pub(crate) fn transcribe_wav_bytes(t: &mut dyn Transcribe, bytes: &[u8], lang: Option<&str>) -> Result<Vec<String>, String> {
    let (rate, mono) = parse_wav(bytes)?;
    transcribe_samples(t, &resample_to_16k(&mono, rate), lang)
}

// The tag `voice_start` reports back: the one the user chose, or the interface language when
// the choice is "auto" (the engine detects the spoken language, but text commands need one).
pub(crate) fn resolved_lang_tag(setting: &str, ui_lang: &str) -> String {
    let s = setting.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("auto") {
        ui_lang.to_string()
    } else {
        s.to_string()
    }
}

// What the session thread is told at start.
#[cfg(feature = "whisper")]
pub(crate) struct SessionArgs {
    pub(crate) app: AppHandle,
    pub(crate) id: u64,
    pub(crate) token: u64,
    pub(crate) model: &'static WhisperModel,
    pub(crate) mic_id: String,
    pub(crate) mic_name: Option<String>,
    pub(crate) lang_tag: String,
    pub(crate) cfg: SessionConfig,
    pub(crate) ended: Arc<AtomicBool>,
    pub(crate) stop_rx: mpsc::Receiver<&'static str>,
}

// ---------- The whisper.cpp engine ----------

#[cfg(feature = "whisper")]
pub(crate) mod real {
    use super::*;
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};
    use windows::Win32::Media::Audio::eCapture;

    static CACHE: ModelCache<WhisperContext> = ModelCache::new();

    // The engine wants at least a second of audio; a short phrase is padded with silence.
    const MIN_SAMPLES: usize = 16_800;

    struct WhisperTranscriber {
        state: WhisperState,
        threads: i32,
        vad: Option<String>,
        abort: Arc<AtomicBool>,
    }

    impl Transcribe for WhisperTranscriber {
        fn transcribe(&mut self, samples: &[f32], lang: Option<&str>) -> Result<String, String> {
            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            params.set_language(Some(lang.unwrap_or("auto")));
            params.set_n_threads(self.threads);
            params.set_translate(false);
            params.set_no_context(true);
            params.set_suppress_blank(true);
            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);
            if let Some(vad) = &self.vad {
                params.enable_vad(true);
                params.set_vad_model_path(Some(vad.as_str()));
            }
            // The library keeps the closure's box for good (one small allocation per phrase). The
            // box type is spelled out because the library reads the pointer back as exactly that.
            let flag = self.abort.clone();
            let watch: Box<dyn FnMut() -> bool> = Box::new(move || flag.load(Ordering::Relaxed));
            params.set_abort_callback_safe::<Option<Box<dyn FnMut() -> bool>>, Box<dyn FnMut() -> bool>>(Some(watch));
            let padded;
            let audio: &[f32] = if samples.len() < MIN_SAMPLES {
                let mut v = samples.to_vec();
                v.resize(MIN_SAMPLES, 0.0);
                padded = v;
                &padded
            } else {
                samples
            };
            self.state.full(params, audio).map_err(|e| e.to_string())?;
            let mut text = String::new();
            for segment in self.state.as_iter() {
                text.push_str(&segment.to_str_lossy().map_err(|e| e.to_string())?);
            }
            Ok(text)
        }
    }

    fn load(path: &Path) -> Result<WhisperContext, String> {
        let p = path.to_str().ok_or_else(|| "model path".to_string())?;
        let mut params = WhisperContextParameters::default();
        params.use_gpu(false);
        WhisperContext::new_with_params(p, params).map_err(|e| e.to_string())
    }

    // The model from the cache (loaded on first use) and a transcriber with its own working state.
    fn build_engine(model: &'static WhisperModel, dir: &Path, abort: Arc<AtomicBool>) -> Result<Box<dyn Transcribe + Send>, String> {
        let ctx = load_if_supported(&system_profile(), || CACHE.acquire(model.id, || load(&dir.join(model.asset.file))))?;
        let state = ctx.create_state().map_err(|e| e.to_string())?;
        let vad = vad_downloaded(dir).then(|| dir.join(WHISPER_VAD.file).to_string_lossy().into_owned());
        let threads = engine_threads(system_profile().physical_cores);
        Ok(Box::new(WhisperTranscriber { state, threads, vad, abort }))
    }

    // The session is over: the model stays for MODEL_KEEP in case dictation starts again.
    fn release_model() {
        if CACHE.release(Instant::now()) {
            spawn_sweeper(&CACHE, MODEL_KEEP);
        }
    }

    fn session_body(a: &SessionArgs) -> SessionOutcome {
        let (model, mic_id, cfg) = (a.model, a.mic_id.as_str(), &a.cfg);
        let fail = |key: &'static str| SessionOutcome { reason: "error", phrases: 0, error: Some(key) };
        let Some(dir) = whisper_dir() else { return fail("voice-failed") };
        // The microphone opens first, so what is said while the model loads is not lost.
        let queue = Arc::new(AudioQueueShared::new());
        let cap_stop = Arc::new(AtomicBool::new(false));
        if crate::audio::spawn_capture("dictation", eCapture, false, mic_id.to_string(), queue.clone(), cap_stop.clone()).is_err() {
            return fail("voice-mic");
        }
        // The window shows "listening" on the first event of a session; this engine says nothing
        // until a phrase is transcribed, so the open microphone announces itself with an empty
        // partial (the window inserts nothing for it).
        let _ = a.app.emit("voice-partial", VoiceTextEvent { id: a.id, token: a.token, text: String::new() });
        let abort = Arc::new(AtomicBool::new(false));
        let engine = match build_engine(model, &dir, abort.clone()) {
            Ok(e) => e,
            Err(e) => {
                cap_stop.store(true, Ordering::SeqCst);
                append_crash_log(&format!("voice: local engine did not start: {e}"));
                return fail("voice-failed");
            }
        };
        let mut carry: Vec<i16> = Vec::new();
        let q = queue.clone();
        let mut pull = move || -> Result<Vec<f32>, &'static str> {
            let pcm = q.drain_available(usize::MAX);
            if pcm.is_empty() && q.is_lost() {
                return Err("voice-mic");
            }
            Ok(mono16k_from_capture(&pcm, &mut carry))
        };
        let app = a.app.clone();
        let (id, token) = (a.id, a.token);
        let on_text: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |text| {
            let _ = app.emit("voice-text", VoiceTextEvent { id, token, text });
        });
        let outcome = run_pipeline(cfg, engine, &mut pull, &a.stop_rx, &abort, on_text);
        cap_stop.store(true, Ordering::SeqCst);
        outcome
    }

    // Runs on the session thread for the whole session and always ends it the way the window
    // expects: an error first (if any), then voice-stopped. Never logs what was recognized.
    pub(crate) fn run_session(a: SessionArgs, active: ActiveModelGuard<'static>) {
        let started = Instant::now();
        append_crash_log(&format!(
            "voice: local session start model={} lang={} auto_stop={}s max={}s",
            a.model.id,
            a.lang_tag,
            a.cfg.auto_stop_sec,
            a.cfg.max_sec
        ));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| session_body(&a)))
            .unwrap_or(SessionOutcome { reason: "error", phrases: 0, error: Some("voice-failed") });
        release_model();
        drop(active);
        let error = outcome.error.or((outcome.reason == "silence" && outcome.phrases == 0).then_some("voice-nothing-heard"));
        append_crash_log(&format!(
            "voice: local session end reason={} phrases={} error={} after {} ms",
            outcome.reason,
            outcome.phrases,
            error.unwrap_or("-"),
            started.elapsed().as_millis()
        ));
        a.ended.store(true, Ordering::SeqCst);
        crate::voice::emit_end(&a.app, a.id, a.token, outcome.reason, error, a.mic_name.as_deref());
    }

    // A WAV file through the live pipeline, for the test hook.
    pub(crate) fn transcribe_file(model: &'static WhisperModel, bytes: &[u8], lang: Option<&str>) -> Result<Vec<String>, String> {
        let dir = whisper_dir().ok_or_else(|| "no per-user folder".to_string())?;
        let _active = ActiveModelGuard::set(&WHISPER_ACTIVE_MODEL, model.id);
        let result = build_engine(model, &dir, Arc::new(AtomicBool::new(false)))
            .and_then(|mut engine| transcribe_wav_bytes(&mut *engine, bytes, lang));
        release_model();
        result
    }
}
