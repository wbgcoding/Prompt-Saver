use super::*;

// ---------------------------------------------------------------------------
// Local speech recognition commands: the model catalog, the consented download and the
// removal of a model. The logic lives in `whisper.rs`; these are the thin Tauri wrappers.
// ---------------------------------------------------------------------------

// The setting that picks the dictation engine (a `ui_texts` entry): "windows" (default) or
// "whisper".
pub(crate) const VOICE_ENGINE_KEY: &str = "voiceEngine";
pub(crate) const VOICE_ENGINE_WHISPER: &str = "whisper";

#[derive(Serialize, Clone)]
pub(crate) struct WhisperModelInfo {
    pub(crate) id: &'static str,
    pub(crate) file: &'static str,
    pub(crate) bytes: u64,
    pub(crate) label_key: &'static str,
    pub(crate) quality_key: &'static str,
    pub(crate) downloaded: bool,
}

#[derive(Serialize, Clone)]
pub(crate) struct WhisperCatalog {
    pub(crate) models: Vec<WhisperModelInfo>,
    pub(crate) recommended: &'static str,
    pub(crate) profile: SystemProfile,
    pub(crate) vad_downloaded: bool,
    pub(crate) downloading: bool,
    // False on a processor without AVX2: the engine cannot run there.
    pub(crate) supported: bool,
}

pub(crate) fn build_catalog(dir: Option<&Path>, profile: SystemProfile) -> WhisperCatalog {
    let models = WHISPER_MODELS
        .iter()
        .map(|m| WhisperModelInfo {
            id: m.id,
            file: m.asset.file,
            bytes: m.asset.bytes,
            label_key: m.label_key,
            quality_key: m.quality_key,
            downloaded: dir.is_some_and(|d| model_downloaded(d, m)),
        })
        .collect();
    WhisperCatalog {
        models,
        recommended: recommend(&profile),
        supported: whisper_supported(&profile),
        profile,
        vad_downloaded: dir.is_some_and(vad_downloaded),
        downloading: WHISPER_DOWNLOAD_RUNNING.load(Ordering::SeqCst),
    }
}

// The models with their downloaded state, the one that suits this PC and the PC's profile.
#[tauri::command(async)]
pub(crate) fn whisper_catalog() -> WhisperCatalog {
    build_catalog(whisper_dir().as_deref(), system_profile())
}

// Called by the confirm dialog's accept path (and nowhere else) right before the download
// starts. A renderer that skips the dialog has no consent for download_whisper_model to use.
#[tauri::command]
pub(crate) fn grant_whisper_download_consent(model: String) -> Result<(), String> {
    let m = whisper_model(&model).ok_or_else(|| WHISPER_ERR_UNKNOWN.to_string())?;
    *WHISPER_CONSENT.lock().unwrap_or_else(|e| e.into_inner()) = Some((m.id.to_string(), crate::bg_ops::now_ms().max(1)));
    Ok(())
}

#[tauri::command]
pub(crate) fn cancel_whisper_download() {
    WHISPER_DOWNLOAD_CANCEL.store(true, Ordering::SeqCst);
}

#[derive(Serialize, Clone)]
pub(crate) struct WhisperDownloadResult {
    pub(crate) model: String,
    // True when this download switched the dictation engine to Whisper (the first model).
    pub(crate) engine_set: bool,
}

// The first model a user downloads also switches dictation to Whisper; later downloads leave
// a deliberate choice of engine alone.
pub(crate) fn engine_after_download(was_first: bool, current: Option<&str>) -> Option<&'static str> {
    (was_first && current != Some(VOICE_ENGINE_WHISPER)).then_some(VOICE_ENGINE_WHISPER)
}

// Downloads the model (and the voice-activity file with the first one), verifies and installs
// it. The frontend asks through its confirm dialog and records the answer with
// grant_whisper_download_consent; the command refuses to start without that one-shot consent.
// Progress goes out as "whisper-download-progress" ({model, done, total}); a cancel or any
// failure leaves nothing behind.
#[tauri::command]
pub(crate) async fn download_whisper_model(
    app: AppHandle,
    state: State<'_, Db>,
    model: String,
) -> Result<WhisperDownloadResult, String> {
    let m = whisper_model(&model).ok_or_else(|| WHISPER_ERR_UNKNOWN.to_string())?;
    let _claim = FlagClaim::take(&WHISPER_DOWNLOAD_RUNNING).ok_or_else(|| WHISPER_ERR_BUSY.to_string())?;
    if !take_whisper_consent(&WHISPER_CONSENT, m.id, crate::bg_ops::now_ms()) {
        return Err(WHISPER_ERR_CONSENT.to_string());
    }
    // Only the claim holder may clear a cancel request.
    WHISPER_DOWNLOAD_CANCEL.store(false, Ordering::SeqCst);
    let dir = whisper_dir().ok_or_else(|| "target dir: no per-user folder".to_string())?;
    let was_first = !any_model_downloaded(&dir);
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // The test hook damages the model file only, never the voice-activity file.
        let open = |url: &str| -> Result<WhisperBody, String> {
            let body = open_https(url)?;
            Ok(if url == m.asset.url { maybe_corrupt(body, &WHISPER_CORRUPT_NEXT) } else { body })
        };
        install_model(&dir, &WHISPER_VAD, &m.asset, &WHISPER_DOWNLOAD_CANCEL, &open, &mut |done, total| {
            let _ = progress_app
                .emit("whisper-download-progress", serde_json::json!({ "model": m.id, "done": done, "total": total }));
        })
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()))?;
    let mut engine_set = false;
    {
        let mut store = lock(&state);
        let current = store.settings.ui_texts.get(VOICE_ENGINE_KEY).cloned();
        if let Some(engine) = engine_after_download(was_first, current.as_deref()) {
            store.settings.ui_texts.insert(VOICE_ENGINE_KEY.to_string(), engine.to_string());
            engine_set = save_settings(&app, &store.settings);
        }
    }
    if engine_set {
        notify_float_settings(&app);
    }
    Ok(WhisperDownloadResult { model: m.id.to_string(), engine_set })
}

// Deletes a downloaded model; refused while a dictation session holds it.
#[tauri::command(async)]
pub(crate) fn delete_whisper_model(model: String) -> Result<(), String> {
    let m = whisper_model(&model).ok_or_else(|| WHISPER_ERR_UNKNOWN.to_string())?;
    let dir = whisper_dir().ok_or_else(|| "target dir: no per-user folder".to_string())?;
    delete_model_in(&dir, m, &WHISPER_ACTIVE_MODEL)
}

// ---------- Test hooks ----------

// Two commands exist only for the automated tests: they act when the process was started with
// PROMPT_SAVER_TEST_HOOKS=1 (read once at start-up) and refuse otherwise.
pub(crate) const TEST_HOOKS_ENV: &str = "PROMPT_SAVER_TEST_HOOKS";
pub(crate) const TEST_HOOKS_OFF: &str = "test-hooks-off";
// The biggest WAV file the file command reads.
#[cfg_attr(not(feature = "whisper"), allow(dead_code))]
const TEST_WAV_MAX_BYTES: u64 = 200 * 1024 * 1024;
static TEST_HOOKS: AtomicBool = AtomicBool::new(false);

pub(crate) fn init_test_hooks() {
    TEST_HOOKS.store(std::env::var(TEST_HOOKS_ENV).is_ok_and(|v| v == "1"), Ordering::SeqCst);
}

pub(crate) fn require_test_hooks(on: bool) -> Result<(), String> {
    if on {
        Ok(())
    } else {
        Err(TEST_HOOKS_OFF.to_string())
    }
}

// A WAV file through the same pipeline a dictation session uses; returns the phrases' texts.
// `model` is a catalog id (default: the one a session would pick), `lang` a BCP-47 tag or "auto".
#[tauri::command(async)]
pub(crate) fn whisper_transcribe_file(path: String, model: Option<String>, lang: Option<String>) -> Result<Vec<String>, String> {
    require_test_hooks(TEST_HOOKS.load(Ordering::SeqCst))?;
    #[cfg(feature = "whisper")]
    {
        let dir = whisper_dir().ok_or_else(|| "no per-user folder".to_string())?;
        let chosen = model.unwrap_or_default();
        let m = pick_session_model(&|m| model_downloaded(&dir, m), &chosen, recommend(&system_profile()))
            .ok_or_else(|| "voice-whisper-no-model".to_string())?;
        let len = fs::metadata(&path).map_err(|e| e.to_string())?.len();
        if len > TEST_WAV_MAX_BYTES {
            return Err("wav-too-large".to_string());
        }
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        let code = lang.as_deref().and_then(crate::whisper_audio::whisper_lang);
        crate::whisper_engine::real::transcribe_file(m, &bytes, code)
    }
    #[cfg(not(feature = "whisper"))]
    {
        let _ = (path, model, lang);
        Err("voice-whisper-no-model".to_string())
    }
}

// Arms the damaged-download hook: the next model download is corrupted once.
#[tauri::command]
pub(crate) fn whisper_test_corrupt_next_download() -> Result<(), String> {
    require_test_hooks(TEST_HOOKS.load(Ordering::SeqCst))?;
    WHISPER_CORRUPT_NEXT.store(true, Ordering::SeqCst);
    Ok(())
}
