use super::{VoiceLang, VoiceStoppedEvent, VoiceSupport, VoiceTextEvent};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use windows::core::HSTRING;
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

// Stable error key from a WinRT HRESULT -- never the raw code, never any
// recognized text. Takes the raw code rather than a `windows::core::Error`
// so the mapping table itself is a plain, unit-testable function (see the
// "voice HRESULT mapping" tests).
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

// The Completed handler's end-reason table: TimeoutExceeded (7) is always
// "silence" -- Windows' own AutoStopSilenceTimeout fired.
// Success (0) means WE asked it to stop; `requested` is whatever the caller
// (voice_stop, the hard cap, or the own idle watchdog) set right before
// calling StopAsync, defaulting to "manual" when nothing was recorded (a
// plain externally-triggered stop). Anything else is a genuine error.
pub(super) fn stop_reason(status: i32, requested: Option<&'static str>) -> &'static str {
    match status {
        7 => "silence",
        0 => requested.unwrap_or("manual"),
        _ => "error",
    }
}

// The idle/hard-cap watchdog's decision, extracted as a pure function so the
// timing rules (0 = auto-stop off, the hard cap always wins once reached) are
// unit-testable without a real session. `elapsed`/`idle` are both measured
// from the session's own clocks (see run_session's loop).
pub(super) fn should_stop(idle: Duration, elapsed: Duration, auto_stop_sec: u32, max_sec: u32) -> Option<&'static str> {
    if elapsed >= Duration::from_secs(max_sec.max(1) as u64) {
        return Some("max");
    }
    if auto_stop_sec > 0 && idle >= Duration::from_secs(auto_stop_sec as u64) {
        return Some("silence");
    }
    None
}

// The frontend's own slider caps (voiceMaxMin/voiceAutoStopSec, ui/expertConfig.js
// EXPERT_VALUES) are UI convenience only -- these are the real limits, so a
// stale/tampered/future caller cannot bypass them. Extracted as a pure
// function so the bounds themselves are unit-tested, not just trusted.
pub(super) fn clamp_durations(max_sec: u32, auto_stop_sec: u32) -> (u32, u32) {
    (max_sec.clamp(60, 3600), auto_stop_sec.clamp(0, 60))
}

// Installed recognition languages. Measured 21.08.: SupportedTopicLanguages
// (and SupportedGrammarLanguages) only ever list tags with an installed
// Language.Speech pack -- so unlike OCR's AvailableRecognizerLanguages there
// is no separate "installed" flag to compute, it is always true here.
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

// Whether Windows' own online-speech consent is already on -- exactly one
// REG_DWORD, no elevated rights needed to read or write it. Missing key =
// not accepted, same as 0.
fn online_speech_accepted() -> bool {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Speech_OneCore\\Settings\\OnlineSpeechPrivacy")
        .and_then(|k| k.get_value::<u32, _>("HasAccepted"))
        .map(|v| v == 1)
        .unwrap_or(false)
}

pub(super) fn enable_online_speech() -> Result<(), String> {
    let (key, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .create_subkey("Software\\Microsoft\\Speech_OneCore\\Settings\\OnlineSpeechPrivacy")
        .map_err(|e| e.to_string())?;
    key.set_value("HasAccepted", &1u32).map_err(|e| e.to_string())
}

// "auto" -> the system speech language IF a pack for it is installed,
// otherwise (or for an explicit tag whose pack is missing, when auto_pick is
// on) whatever recognition language IS installed -- same "use whatever is
// already there" fallback as OCR's create_auto_engine/resolve_engine. The
// actual decision is the plain, unit-testable pick_lang below; this only
// gathers the two WinRT-sourced inputs it needs.
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

fn resolve_lang(lang: &str, auto_pick: bool) -> Result<String, String> {
    let installed: Vec<String> = languages()?.into_iter().map(|v| v.tag).collect();
    let system_tag = SpeechRecognizer::SystemSpeechLanguage().ok().and_then(|l| l.LanguageTag().ok()).map(|t| t.to_string());
    pick_lang(lang, auto_pick, &installed, system_tag.as_deref())
}

// voice_support(): cheap "does Windows have a speech engine at all" probe --
// deliberately does not start a real session (Create() alone is enough to
// learn this; actually opening the mic waits for the user's first real
// voice_start, so the OS mic-access prompt never appears before that).
pub(super) fn support() -> VoiceSupport {
    ensure_winrt();
    let langs = match languages() {
        Ok(v) => v,
        Err(e) => {
            // A real WinRT/registry failure, not just "no language pack
            // installed" -- both collapse to the same supported=false below
            // (voice genuinely cannot work either way from here), but only
            // this one is a bug worth a trace instead of an ordinary, expected
            // state that needs none.
            super::append_crash_log(&format!("voice_support: language enumeration failed: {e}"));
            Vec::new()
        }
    };
    let supported = !langs.is_empty() && SpeechRecognizer::new().is_ok();
    VoiceSupport { supported, engine: "winrt", langs, needs_online: !online_speech_accepted() }
}

// One spawn_blocking round trip for voice_start: resolves "auto"/an explicit
// tag to a real installed BCP-47 tag AND reports the current online-consent
// state together, so the command handler needs only one hop into COM.
pub(super) fn prepare(lang: String, auto_pick: bool) -> Result<(String, bool), String> {
    ensure_winrt();
    let resolved = resolve_lang(&lang, auto_pick)?;
    Ok((resolved, !online_speech_accepted()))
}

// Runs on its own thread for the whole session lifetime. Never returns
// early on a recoverable error -- run_session already reports it via
// voice-error + voice-stopped before returning Err, this only guards
// against a failure happening BEFORE `ended` could be set by anything else
// (e.g. Create() itself failing, so Completed never fires at all).
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
    recognizer.CompileConstraintsAsync().and_then(|a| a.get()).map_err(|e| error_key(&e))?;
    let session = recognizer.ContinuousRecognitionSession().map_err(|e| error_key(&e))?;

    // Windows' own AutoStopSilenceTimeout is the belt of two intentionally
    // redundant silence checks -- the app's own idle watchdog below is the
    // one that actually decides. When
    // voiceAutoStop is off (auto_stop_sec == 0), point Windows' property at
    // the hard cap instead, so it never fires earlier than max_sec on its own.
    let watch_secs = if auto_stop_sec > 0 { auto_stop_sec } else { max_sec };
    let _ = session.SetAutoStopSilenceTimeout(windows::Foundation::TimeSpan { Duration: watch_secs as i64 * 10_000_000 });

    let last_activity = Arc::new(Mutex::new(Instant::now()));
    // What voice_stop/the watchdog set right before calling StopAsync, so the
    // Completed handler (status == Success) knows WHY it was asked to stop.
    // A real Windows-detected silence timeout reports status == 7 on its own
    // and never needs this.
    let requested_reason: Arc<Mutex<Option<&'static str>>> = Arc::new(Mutex::new(None));

    if interim {
        let app2 = app.clone();
        let last2 = last_activity.clone();
        // An error here means partial-result delivery never gets wired up --
        // recording would silently start anyway with nothing ever reaching the
        // window, so this must fail the session up front like every other WinRT
        // setup call in this function, not be swallowed.
        recognizer
            .HypothesisGenerated(&TypedEventHandler::new(
                move |_sender: &Option<SpeechRecognizer>, args: &Option<SpeechRecognitionHypothesisGeneratedEventArgs>| {
                    if let Some(args) = args {
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
    // Same reasoning as HypothesisGenerated above: a session that starts anyway
    // with this unregistered would record indefinitely and never deliver a
    // single word of text.
    session
        .ResultGenerated(&TypedEventHandler::new(
            move |_sender: &Option<SpeechContinuousRecognitionSession>, args: &Option<SpeechContinuousRecognitionResultGeneratedEventArgs>| {
                *last_r.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
                if let Some(args) = args {
                    if let Ok(result) = args.Result() {
                        if let Ok(text) = result.Text() {
                            let text = text.to_string();
                            // Nothing recognized in this segment -> nothing to insert,
                            // same "empty is not an error" rule as OCR's recognize().
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
    // The most important of the three registrations to catch: if this one alone
    // fails to register, a session that otherwise starts fine will run to
    // completion (or be stopped) and never tell the window it did -- "recording"
    // stays true, the microphone icon never clears, forever.
    session
        .Completed(&TypedEventHandler::new(
            move |_sender: &Option<SpeechContinuousRecognitionSession>, args: &Option<SpeechContinuousRecognitionCompletedEventArgs>| {
                // Fires exactly once per session; the swap makes a second call (should
                // never happen, but WinRT event delivery guarantees are not a Rust
                // guarantee) a no-op instead of a duplicated event.
                if ended_c.swap(true, Ordering::SeqCst) {
                    return Ok(());
                }
                // status.0 is out of range for the stop_reason table's two named
                // cases (0/7) only on genuine failure -- map that to -1, which the
                // table already resolves to "error".
                let status = args.as_ref().and_then(|a| a.Status().ok()).map(|s| s.0).unwrap_or(-1);
                let requested = reason_c.lock().unwrap_or_else(|e| e.into_inner()).take();
                let reason = stop_reason(status, requested);
                let _ = app_c.emit("voice-stopped", VoiceStoppedEvent { id, token, reason: reason.to_string() });
                super::set_tray_recording(&app_c, false);
                Ok(())
            },
        ))
        .map_err(|e| error_key(&e))?;

    session.StartAsync().and_then(|a| a.get()).map_err(|e| error_key(&e))?;

    // A stop the thread itself decided on (hard cap / own idle watchdog) --
    // fires StopAsync and then just waits for Completed to actually flip
    // `ended`, same as a stop requested from outside.
    let self_stop = |reason: &'static str| {
        *requested_reason.lock().unwrap_or_else(|e| e.into_inner()) = Some(reason);
        if session.StopAsync().is_err() {
            // Windows refused the stop request ITSELF (not merely "already
            // stopped" -- a session that ended on its own already set `ended`
            // via Completed, which this check catches). Completed will almost
            // certainly never fire now, so waiting the usual timeout would only
            // delay reporting a stop that never really happened -- the mic stays
            // open the whole time. Report the truth immediately instead of the
            // caller's intended (and now false) reason.
            if !ended.swap(true, Ordering::SeqCst) {
                let _ = app.emit("voice-stopped", VoiceStoppedEvent { id, token, reason: "error".to_string() });
                super::set_tray_recording(app, false);
            }
            return;
        }
        // Normally Completed fires within a beat and flips `ended` itself.
        // If it never does (a stuck driver, WinRT not delivering the
        // event), do not hold this thread -- and the microphone -- open
        // forever; report the same end after a bounded wait instead. The
        // swap keeps this a no-op if Completed fires in the same instant.
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
                // Idle means the engine has recognized nothing since the last
                // Hypothesis/Result, measured independently of the engine's
                // own timeout so the behavior does not depend on Windows
                // honoring AutoStopSilenceTimeout. should_stop also enforces
                // the hard cap (voiceMaxMin), which always wins once reached.
                let idle = last_activity.lock().unwrap_or_else(|e| e.into_inner()).elapsed();
                if let Some(reason) = should_stop(idle, session_started.elapsed(), auto_stop_sec, max_sec) {
                    self_stop(reason);
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // The sending half was dropped without a message (should not
                // normally happen -- voice_shutdown always sends "shutdown" --
                // but treat it the same way rather than looping forever).
                self_stop("shutdown");
                break;
            }
        }
    }
    Ok(())
}
