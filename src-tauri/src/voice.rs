use super::{VoiceLang, VoiceStoppedEvent, VoiceSupport, VoiceTextEvent};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use windows::core::{Ref, HSTRING};
use windows::Foundation::TypedEventHandler;
use windows::Globalization::Language;
use windows::Media::SpeechRecognition::{
    SpeechContinuousRecognitionCompletedEventArgs, SpeechContinuousRecognitionResultGeneratedEventArgs,
    SpeechContinuousRecognitionSession, SpeechRecognitionHypothesisGeneratedEventArgs,
    SpeechRecognitionScenario, SpeechRecognitionTopicConstraint, SpeechRecognizer,
};
use super::ocr::ensure_winrt;

// HRESULTs measured against a real machine with a working microphone.
const HRESULT_NO_LANGUAGE: i32 = 0x800455BCu32 as i32; // "The requested language is not supported"
const HRESULT_ACCESS_DENIED: i32 = 0x80070005u32 as i32; // no mic OR mic access denied -- one HRESULT covers both
const HRESULT_PRIVACY_OFF: i32 = 0x80045509u32 as i32; // "speech privacy policy was not accepted"

// Stable error key from a WinRT HRESULT, never the raw code or any recognized text. Takes the
// raw code so the mapping is a plain, unit-testable function.
pub(super) fn error_key_for_hresult(code: i32) -> &'static str {
    match code {
        HRESULT_NO_LANGUAGE => "voice-no-language",
        HRESULT_ACCESS_DENIED => "voice-denied",
        HRESULT_PRIVACY_OFF => "voice-privacy-off",
        _ => "voice-failed",
    }
}
fn error_key(e: &windows::core::Error) -> &'static str {
    error_key_for_hresult(e.code().0)
}

// The Completed handler's end-reason table. TimeoutExceeded (7) is always "silence" (Windows'
// AutoStopSilenceTimeout fired). Success (0) means we asked it to stop; `requested` is the
// reason set right before StopAsync, "manual" when none was recorded. Anything else is an error.
pub(super) fn stop_reason(status: i32, requested: Option<&'static str>) -> &'static str {
    match status {
        7 => "silence",
        0 => requested.unwrap_or("manual"),
        _ => "error",
    }
}

// The idle/hard-cap watchdog's decision as a pure function (0 = auto-stop off, the hard cap
// always wins once reached). `elapsed`/`idle` come from the session's own clocks.
pub(super) fn should_stop(idle: Duration, elapsed: Duration, auto_stop_sec: u32, max_sec: u32) -> Option<&'static str> {
    if elapsed >= Duration::from_secs(max_sec.max(1) as u64) {
        return Some("max");
    }
    if auto_stop_sec > 0 && idle >= Duration::from_secs(auto_stop_sec as u64) {
        return Some("silence");
    }
    None
}

// The frontend's slider caps (voiceMaxMin/voiceAutoStopSec) are only convenience; these are
// the real limits, so a stale or tampered caller cannot bypass them.
pub(super) fn clamp_durations(max_sec: u32, auto_stop_sec: u32) -> (u32, u32) {
    (max_sec.clamp(60, 3600), auto_stop_sec.clamp(0, 60))
}

// Installed recognition languages. SupportedTopicLanguages only lists tags with an installed
// Language.Speech pack, so unlike OCR there is no separate "installed" flag: it is always true.
pub(super) fn languages() -> Result<Vec<VoiceLang>, String> {
    ensure_winrt();
    let langs = SpeechRecognizer::SupportedTopicLanguages().map_err(|e| e.to_string())?;
    let size = langs.Size().map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(size as usize);
    for i in 0..size {
        let lang = langs.GetAt(i).map_err(|e| e.to_string())?;
        out.push(VoiceLang {
            tag: lang.LanguageTag().map_err(|e| e.to_string())?.to_string(),
            label: lang.DisplayName().map_err(|e| e.to_string())?.to_string(),
            installed: true,
        });
    }
    Ok(out)
}

// Whether Windows' online-speech consent is on: one REG_DWORD, readable and writable without
// elevation. A missing key counts as not accepted.
fn online_speech_accepted() -> bool {
    crate::registry::Key::open_read(crate::registry::Hive::CurrentUser, r"Software\Microsoft\Speech_OneCore\Settings\OnlineSpeechPrivacy")
        .and_then(|k| k.get_u32("HasAccepted"))
        .map(|v| v == 1)
        .unwrap_or(false)
}

pub(super) fn enable_online_speech() -> Result<(), String> {
    crate::registry::Key::create_write(crate::registry::Hive::CurrentUser, r"Software\Microsoft\Speech_OneCore\Settings\OnlineSpeechPrivacy")
        .and_then(|key| key.set_u32("HasAccepted", 1))
        .map_err(|e| e.to_string())
}

// "auto" -> the system speech language if a pack for it is installed, otherwise (or for an
// explicit tag whose pack is missing, when auto_pick is on) any installed recognition language.
pub(super) fn pick_lang(lang: &str, auto_pick: bool, installed: &[String], system_tag: Option<&str>) -> Result<String, String> {
    if installed.is_empty() {
        return Err("voice-no-language".to_string());
    }
    if lang != "auto" && !lang.trim().is_empty() {
        if installed.iter().any(|t| t == lang) {
            return Ok(lang.to_string());
        }
        if !auto_pick {
            return Err("voice-no-language".to_string());
        }
    }
    if let Some(tag) = system_tag {
        if installed.iter().any(|t| t == tag) {
            return Ok(tag.to_string());
        }
    }
    Ok(installed[0].clone())
}

// Gathers pick_lang's two WinRT-sourced inputs.
fn resolve_lang(lang: &str, auto_pick: bool) -> Result<String, String> {
    let installed: Vec<String> = languages()?.into_iter().map(|v| v.tag).collect();
    let system_tag = SpeechRecognizer::SystemSpeechLanguage().ok().and_then(|l| l.LanguageTag().ok()).map(|t| t.to_string());
    pick_lang(lang, auto_pick, &installed, system_tag.as_deref())
}

// voice_support(): a cheap "does Windows have a speech engine at all" probe. It does not
// start a real session (Create() alone suffices), so the OS mic-access prompt only appears
// at the first voice_start.
pub(super) fn support() -> VoiceSupport {
    ensure_winrt();
    let langs = match languages() {
        Ok(v) => v,
        Err(e) => {
            // A real WinRT/registry failure, not just a missing language pack (both give supported=false
            // below), so it is worth a trace.
            super::append_crash_log(&format!("voice_support: language enumeration failed: {e}"));
            Vec::new()
        }
    };
    let supported = !langs.is_empty() && SpeechRecognizer::new().is_ok();
    VoiceSupport { supported, engine: "winrt", langs, needs_online: !online_speech_accepted() }
}

// One spawn_blocking round trip for voice_start: resolves "auto"/an explicit
// tag to a real installed BCP-47 tag.
pub(super) fn prepare(lang: String, auto_pick: bool) -> Result<String, String> {
    ensure_winrt();
    resolve_lang(&lang, auto_pick)
}

// Runs on its own thread for the whole session. run_session reports a recoverable error via
// voice-error + voice-stopped before returning Err; this only guards a failure before `ended`
// could be set elsewhere (e.g. Create() failing, so Completed never fires).
#[allow(clippy::too_many_arguments)]
pub(super) fn start_session(
    app: AppHandle,
    id: u64,
    token: u64,
    lang: String,
    auto_stop_sec: u32,
    max_sec: u32,
    interim: bool,
    ended: Arc<AtomicBool>,
    stop_rx: mpsc::Receiver<&'static str>,
) {
    ensure_winrt();
    if let Err(key) = run_session(&app, id, token, &lang, auto_stop_sec, max_sec, interim, &ended, &stop_rx) {
        if !ended.swap(true, Ordering::SeqCst) {
            let _ = app.emit("voice-error", super::VoiceErrorEvent { id, token, code: key.to_string() });
            let _ = app.emit("voice-stopped", VoiceStoppedEvent { id, token, reason: "error".to_string() });
            super::set_tray_recording(&app, false);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_session(
    app: &AppHandle,
    id: u64,
    token: u64,
    lang: &str,
    auto_stop_sec: u32,
    max_sec: u32,
    interim: bool,
    ended: &Arc<AtomicBool>,
    stop_rx: &mpsc::Receiver<&'static str>,
) -> Result<(), &'static str> {
    let session_started = Instant::now();
    let language = Language::CreateLanguage(&HSTRING::from(lang)).map_err(|e| error_key(&e))?;
    let recognizer = SpeechRecognizer::Create(&language).map_err(|e| error_key(&e))?;
    let constraint = SpeechRecognitionTopicConstraint::Create(SpeechRecognitionScenario::Dictation, &HSTRING::from("dictation"))
        .map_err(|e| error_key(&e))?;
    recognizer.Constraints().and_then(|c| c.Append(&constraint)).map_err(|e| error_key(&e))?;
    recognizer.CompileConstraintsAsync().and_then(|a| a.join()).map_err(|e| error_key(&e))?;
    let session = recognizer.ContinuousRecognitionSession().map_err(|e| error_key(&e))?;

    // Windows' AutoStopSilenceTimeout is a redundant backstop; the app's own idle watchdog below
    // decides. With voiceAutoStop off (auto_stop_sec == 0) it is set to the hard cap so it never
    // fires earlier than max_sec.
    let watch_secs = if auto_stop_sec > 0 { auto_stop_sec } else { max_sec };
    let _ = session.SetAutoStopSilenceTimeout(windows::Foundation::TimeSpan { Duration: watch_secs as i64 * 10_000_000 });

    let last_activity = Arc::new(Mutex::new(Instant::now()));
    // What voice_stop or the watchdog set right before StopAsync, so the Completed handler
    // (status == Success) knows why it was asked to stop. A Windows-detected silence timeout
    // reports status 7 on its own.
    let requested_reason: Arc<Mutex<Option<&'static str>>> = Arc::new(Mutex::new(None));

    if interim {
        let app2 = app.clone();
        let last2 = last_activity.clone();
        // A failure here must fail the session up front like every other WinRT setup call: otherwise
        // recording would start with partial results never reaching the window.
        recognizer
            .HypothesisGenerated(&TypedEventHandler::new(
                move |_sender: Ref<SpeechRecognizer>, args: Ref<SpeechRecognitionHypothesisGeneratedEventArgs>| {
                    if let Some(args) = args.as_ref() {
                        if let Ok(h) = args.Hypothesis() {
                            if let Ok(text) = h.Text() {
                                *last2.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
                                let _ = app2.emit("voice-partial", VoiceTextEvent { id, token, text: text.to_string() });
                            }
                        }
                    }
                    Ok(())
                },
            ))
            .map_err(|e| error_key(&e))?;
    }

    let app_r = app.clone();
    let last_r = last_activity.clone();
    // Same as HypothesisGenerated: a session started without this would record forever and
    // never deliver text.
    session
        .ResultGenerated(&TypedEventHandler::new(
            move |_sender: Ref<SpeechContinuousRecognitionSession>, args: Ref<SpeechContinuousRecognitionResultGeneratedEventArgs>| {
                *last_r.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
                if let Some(args) = args.as_ref() {
                    if let Ok(result) = args.Result() {
                        if let Ok(text) = result.Text() {
                            let text = text.to_string();
                            // Nothing recognized in this segment -> nothing to insert.
                            if !text.is_empty() {
                                let _ = app_r.emit("voice-text", VoiceTextEvent { id, token, text });
                            }
                        }
                    }
                }
                Ok(())
            },
        ))
        .map_err(|e| error_key(&e))?;

    let app_c = app.clone();
    let ended_c = ended.clone();
    let reason_c = requested_reason.clone();
    // The most important registration: if it fails, a session otherwise running fine would end
    // without telling the window, and the microphone icon would never clear.
    session
        .Completed(&TypedEventHandler::new(
            move |_sender: Ref<SpeechContinuousRecognitionSession>, args: Ref<SpeechContinuousRecognitionCompletedEventArgs>| {
                // Fires once per session; the swap makes a repeated call a no-op instead of a duplicate event.
                if ended_c.swap(true, Ordering::SeqCst) {
                    return Ok(());
                }
                // A status outside the stop_reason table's named cases (0/7) only occurs on failure: map it
                // to -1, which resolves to "error".
                let status = args.as_ref().and_then(|a| a.Status().ok()).map(|s| s.0).unwrap_or(-1);
                let requested = reason_c.lock().unwrap_or_else(|e| e.into_inner()).take();
                let reason = stop_reason(status, requested);
                let _ = app_c.emit("voice-stopped", VoiceStoppedEvent { id, token, reason: reason.to_string() });
                super::set_tray_recording(&app_c, false);
                Ok(())
            },
        ))
        .map_err(|e| error_key(&e))?;

    session.StartAsync().and_then(|a| a.join()).map_err(|e| error_key(&e))?;

    // A stop the thread itself decided on (hard cap / idle watchdog): fires StopAsync, then
    // waits for Completed to flip `ended`, like an external stop.
    let self_stop = |reason: &'static str| {
        *requested_reason.lock().unwrap_or_else(|e| e.into_inner()) = Some(reason);
        if session.StopAsync().is_err() {
            // Windows refused the stop request itself (a session that ended on its own already set
            // `ended` via Completed). Completed will almost certainly never fire, so report the failure
            // immediately instead of waiting the timeout with the mic open, and not with the intended reason.
            if !ended.swap(true, Ordering::SeqCst) {
                let _ = app.emit("voice-stopped", VoiceStoppedEvent { id, token, reason: "error".to_string() });
                super::set_tray_recording(app, false);
            }
            return;
        }
        // Completed normally flips `ended` within a beat. If it never does (stuck driver, event not
        // delivered), report the same end after a bounded wait instead of holding the thread and the
        // microphone open forever. The swap keeps this a no-op if Completed fires at the same instant.
        let deadline = Instant::now() + super::VOICE_SELF_STOP_TIMEOUT;
        while !ended.load(Ordering::SeqCst) {
            if Instant::now() >= deadline {
                if !ended.swap(true, Ordering::SeqCst) {
                    let _ = app.emit("voice-stopped", VoiceStoppedEvent { id, token, reason: reason.to_string() });
                    super::set_tray_recording(app, false);
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    loop {
        if ended.load(Ordering::SeqCst) {
            break;
        }
        match stop_rx.recv_timeout(Duration::from_millis(250)) {
            Ok(reason) => {
                self_stop(reason);
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Idle = nothing recognized since the last Hypothesis/Result, measured independently of
                // Windows' own timeout. should_stop also enforces the hard cap (voiceMaxMin).
                let idle = last_activity.lock().unwrap_or_else(|e| e.into_inner()).elapsed();
                if let Some(reason) = should_stop(idle, session_started.elapsed(), auto_stop_sec, max_sec) {
                    self_stop(reason);
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // The sending half was dropped without a message (voice_shutdown normally sends "shutdown");
                // treat it the same way rather than looping forever.
                self_stop("shutdown");
                break;
            }
        }
    }
    Ok(())
}
