use super::*;

// ---------------------------------------------------------------------------
// Voice dictation commands: thin, never-cfg-gated `#[tauri::command]` wrappers (same shape
// as the ocr_* ones) whose body branches on `#[cfg(windows)]` and calls into `mod voice`.
// ---------------------------------------------------------------------------

pub(crate) static VOICE_SESSION_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
// How long self_stop waits for WinRT's own Completed callback after
// StopAsync() before giving up and reporting the end itself, so a stuck
// callback never holds the session thread (and the microphone) open.
pub(crate) const VOICE_SELF_STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[tauri::command]
pub(crate) async fn voice_support() -> VoiceSupport {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(voice::support).await.unwrap_or(VoiceSupport {
            supported: false,
            engine: "none",
            langs: Vec::new(),
            needs_online: false,
        })
    }
    #[cfg(not(windows))]
    {
        VoiceSupport { supported: false, engine: "none", langs: Vec::new(), needs_online: false }
    }
}

#[tauri::command]
pub(crate) async fn voice_languages() -> Result<Vec<VoiceLang>, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(voice::languages).await.map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        Err("voice is only available on Windows".to_string())
    }
}

// `lang` is "auto" or an explicit BCP-47 tag; `auto_stop_sec` (0 = off) and `max_sec` are the
// voiceAutoStopSec/voiceMaxMin expert settings, sent by the frontend like ocr_image's. `interim`
// mirrors voiceInterim. Returns the new session id; recognized text only travels through the
// voice-partial/voice-text events.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn voice_start(
    app: AppHandle,
    state: State<'_, Db>,
    vstate: State<'_, VoiceState>,
    lang: String,
    auto_stop_sec: u32,
    max_sec: u32,
    interim: bool,
    token: u64,
) -> Result<VoiceStartResult, String> {
    {
        let guard = vstate.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = guard.as_ref() {
            if !h.ended.load(std::sync::atomic::Ordering::SeqCst) {
                return Err("voice-busy".to_string());
            }
        }
        // Claim the slot here, under the same lock the check above read: voice::prepare() below is an
        // await point, so two concurrent voice_start calls could both pass the check, both spawn a
        // session thread, and the second handle would overwrite the first, orphaning a live
        // microphone session voice_stop can never reach. Cleared on every early return below.
        if vstate.1.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err("voice-busy".to_string());
        }
    }
    #[cfg(windows)]
    {
        let online_ok = lock(&state).settings.ui_flags.get("voiceOnlineOk").copied().unwrap_or(false);
        let prepared =
            tauri::async_runtime::spawn_blocking(move || voice::prepare(lang, true)).await.map_err(|e| e.to_string());
        let (resolved_lang, needs_online) = match prepared.and_then(|r| r) {
            Ok(v) => v,
            Err(e) => {
                vstate.1.store(false, std::sync::atomic::Ordering::SeqCst);
                return Err(e);
            }
        };
        // Never opens the microphone without consent: checked before the session thread (and Create())
        // starts. A later 0x80045509 from Windows itself (declined at the OS level after voiceOnlineOk
        // was set) is handled by the session as voice-privacy-off.
        if needs_online && !online_ok {
            vstate.1.store(false, std::sync::atomic::Ordering::SeqCst);
            return Err("voice-online-required".to_string());
        }
        let id = VOICE_SESSION_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (max_sec, auto_stop_sec) = voice::clamp_durations(max_sec, auto_stop_sec); // voiceMaxMin: 1-60 min
        let (tx, rx) = std::sync::mpsc::channel::<&'static str>();
        let ended = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let app2 = app.clone();
        let ended2 = ended.clone();
        let lang2 = resolved_lang.clone();
        // Set before the session thread starts: a fast failure inside the thread could otherwise race
        // its own set_tray_recording(false) ahead of this one and leave the tooltip on "recording".
        set_tray_recording(&app, true);
        if let Err(e) = std::thread::Builder::new()
            .name("voice-session".into())
            .spawn(move || voice::start_session(app2, id, token, lang2, auto_stop_sec, max_sec, interim, ended2, rx))
        {
            set_tray_recording(&app, false);
            vstate.1.store(false, std::sync::atomic::Ordering::SeqCst);
            return Err(e.to_string());
        }
        *vstate.0.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(VoiceHandle { id, stop: tx, ended });
        vstate.1.store(false, std::sync::atomic::Ordering::SeqCst);
        Ok(VoiceStartResult { id, lang: resolved_lang })
    }
    #[cfg(not(windows))]
    {
        vstate.1.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = (app, state, lang, auto_stop_sec, max_sec, interim, token);
        Err("voice is only available on Windows".to_string())
    }
}

// Whether voice_stop should signal the session thread: never for a session that already ended
// itself (silence/max/error), since a second StopAsync on a Completed session throws
// (0x80131509). Pure decision, so it is unit-testable.
pub(crate) fn should_send_stop(target: Option<u64>, running_id: u64, already_ended: bool) -> bool {
    (target.is_none() || target == Some(running_id)) && !already_ended
}

// Stops gracefully (StopAsync, so the sentence in progress still arrives). `id` names the
// session to stop; None stops whichever is running.
#[tauri::command]
pub(crate) fn voice_stop(vstate: State<'_, VoiceState>, id: Option<u64>) -> Result<(), String> {
    let guard = vstate.0.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(h) = guard.as_ref() {
        if should_send_stop(id, h.id, h.ended.load(std::sync::atomic::Ordering::SeqCst)) {
            let _ = h.stop.send("manual");
        }
    }
    Ok(())
}

// Only called from the voice-privacy-off dialog, never on startup or silently. Writes Windows'
// online-speech consent registry value and the app's voiceOnlineOk flag in one step (one
// consent, both switches), so the next voice_start proceeds without another round trip.
#[tauri::command]
pub(crate) async fn voice_enable_online_speech(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(voice::enable_online_speech).await.map_err(|e| e.to_string())??;
        let mut store = lock(&state);
        store.settings.ui_flags.insert("voiceOnlineOk".to_string(), true);
        save_settings(&app, &store.settings);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state);
        Err("voice is only available on Windows".to_string())
    }
}

// Opens Windows' microphone privacy page, offered from the voice-denied dialog: the app cannot
// grant mic access itself. Plain "open" verb on ShellExecuteW; a settings URI needs no admin rights.
#[tauri::command]
pub(crate) fn open_privacy_mic() -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::core::{HSTRING, PCWSTR};
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let code = unsafe {
            ShellExecuteW(
                HWND::default(),
                &HSTRING::from("open"),
                &HSTRING::from("ms-settings:privacy-microphone"),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if (code.0 as isize) <= 32 { return Err("open-failed".to_string()); }
        Ok(())
    }
    #[cfg(not(windows))]
    Err("voice is only available on Windows".to_string())
}

// Cancels whatever session is running without waiting for it to finish; called on window
// close/hide and app exit, since a hidden window never keeps listening.
pub(crate) fn voice_shutdown(app: &AppHandle) {
    if let Some(vs) = app.try_state::<VoiceState>() {
        let guard = vs.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = guard.as_ref() {
            if !h.ended.load(std::sync::atomic::Ordering::SeqCst) {
                let _ = h.stop.send("shutdown");
            }
        }
    }
}
