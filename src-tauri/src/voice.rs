use super::{VoiceErrorEvent, VoiceLang, VoiceStoppedEvent, VoiceSupport, VoiceTextEvent};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use windows::core::{Ref, HSTRING};
use windows::Foundation::TypedEventHandler;
use windows::Globalization::Language;
use windows::Media::SpeechRecognition::{
    SpeechContinuousRecognitionCompletedEventArgs, SpeechContinuousRecognitionResultGeneratedEventArgs,
    SpeechContinuousRecognitionSession, SpeechRecognitionHypothesisGeneratedEventArgs,
    SpeechRecognitionQualityDegradingEventArgs, SpeechRecognitionScenario, SpeechRecognitionTopicConstraint, SpeechRecognizer,
    SpeechRecognizerStateChangedEventArgs,
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

// SpeechRecognitionResultStatus codes the session cares about (Windows.Media.SpeechRecognition).
const STATUS_SUCCESS: i32 = 0;
const STATUS_TOPIC_LANGUAGE_NOT_SUPPORTED: i32 = 1;
const STATUS_AUDIO_QUALITY_FAILURE: i32 = 4;
const STATUS_TIMEOUT_EXCEEDED: i32 = 7;
const STATUS_NETWORK_FAILURE: i32 = 9;
const STATUS_MICROPHONE_UNAVAILABLE: i32 = 10;

// CompileConstraintsAsync result -> error key. None = the constraints compiled and the session
// may start; any other status used to be ignored and left a recognizer that never hears anything.
pub(super) fn compile_error_key(status: i32) -> Option<&'static str> {
    match status {
        STATUS_SUCCESS => None,
        STATUS_TOPIC_LANGUAGE_NOT_SUPPORTED => Some("voice-no-topic"),
        STATUS_NETWORK_FAILURE => Some("voice-network"),
        _ => Some("voice-failed"),
    }
}

// Completed status -> error key. Success and the silence timeout are normal ends (None); every
// other status is a failure the person must be told about.
pub(super) fn completed_error_key(status: i32) -> Option<&'static str> {
    match status {
        STATUS_SUCCESS | STATUS_TIMEOUT_EXCEEDED => None,
        STATUS_NETWORK_FAILURE => Some("voice-network"),
        STATUS_MICROPHONE_UNAVAILABLE => Some("voice-mic"),
        STATUS_AUDIO_QUALITY_FAILURE => Some("voice-audio"),
        _ => Some("voice-failed"),
    }
}

// The error a finished session reports before voice-stopped: a failure status, or a session that
// ended on silence (Windows' timeout or the idle watchdog) without a single hypothesis or result.
// `heard` = at least one hypothesis or result arrived.
pub(super) fn end_error_key(status: i32, reason: &str, heard: bool) -> Option<&'static str> {
    if let Some(key) = completed_error_key(status) {
        return Some(key);
    }
    if reason == "silence" && !heard {
        return Some("voice-nothing-heard");
    }
    None
}

// One session summary line for error.log: codes and counts only, never recognized text.
pub(super) fn session_summary(
    status: Option<i32>,
    reason: &str,
    secs: u64,
    hyps: u32,
    results: u32,
    states: &[i32],
    problems: &[i32],
) -> String {
    let status = status.map_or_else(|| "none".to_string(), |s| s.to_string());
    format!(
        "voice: session ended status={status} reason={reason} secs={secs} hypotheses={hyps} results={results} states={states:?} quality_problems={problems:?}"
    )
}

// What the session threads and WinRT callbacks count and note for the diagnostic line. Only
// numbers: hypothesis/result counts, recognizer state codes, audio-problem codes.
#[derive(Default)]
struct Diag {
    hyps: AtomicU32,
    results: AtomicU32,
    states: Mutex<Vec<i32>>,
    problems: Mutex<Vec<i32>>,
}

impl Diag {
    fn heard(&self) -> bool {
        self.hyps.load(Ordering::SeqCst) + self.results.load(Ordering::SeqCst) > 0
    }
    // Each distinct code once, in the order first seen.
    fn note(list: &Mutex<Vec<i32>>, code: i32) {
        let mut v = list.lock().unwrap_or_else(|e| e.into_inner());
        if !v.contains(&code) {
            v.push(code);
        }
    }
    fn line(&self, status: Option<i32>, reason: &str, started: Instant) -> String {
        let states = self.states.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let problems = self.problems.lock().unwrap_or_else(|e| e.into_inner()).clone();
        session_summary(
            status,
            reason,
            started.elapsed().as_secs(),
            self.hyps.load(Ordering::SeqCst),
            self.results.load(Ordering::SeqCst),
            &states,
            &problems,
        )
    }
}

fn log(msg: &str) {
    super::append_crash_log(&format!("voice: {msg}"));
}

// The session start line. Whether a default microphone exists is enough for support; its
// friendly name can carry a person's name ("<Name>'s AirPods") and the log gets shared.
pub(super) fn session_start_line(
    lang: &str,
    auto_stop_sec: u32,
    max_sec: u32,
    interim: bool,
    mic_console: Option<&str>,
    mic_communications: Option<&str>,
) -> String {
    let yn = |m: Option<&str>| if m.is_some() { "yes" } else { "no" };
    format!(
        "session start lang={lang} auto_stop_sec={auto_stop_sec} max_sec={max_sec} interim={interim} mic_console={} mic_communications={}",
        yn(mic_console),
        yn(mic_communications)
    )
}

// A failed WinRT setup step: logged with its HRESULT, mapped to the stable key the window shows.
fn fail(step: &str, e: &windows::core::Error) -> &'static str {
    let key = error_key(e);
    log(&format!("setup step {step} failed hr={:#010x} key={key}", e.code().0 as u32));
    key
}

// The end of a session as the window sees it: the error first (if any), then voice-stopped. The mic
// name rides only on the nothing-heard error, where it is the whole explanation.
fn emit_end(app: &AppHandle, id: u64, token: u64, reason: &str, error: Option<&'static str>, mic: Option<&str>) {
    if let Some(code) = error {
        let mic = if code == "voice-nothing-heard" { mic.map(str::to_string) } else { None };
        let _ = app.emit("voice-error", VoiceErrorEvent { id, token, code: code.to_string(), mic });
    }
    let _ = app.emit("voice-stopped", VoiceStoppedEvent { id, token, reason: reason.to_string() });
    super::set_tray_recording(app, false);
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
        log(&format!("session failed before it ran key={key}"));
        if !ended.swap(true, Ordering::SeqCst) {
            emit_end(&app, id, token, "error", Some(key), None);
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
    // The recognizer records from a default capture device Windows picks; both default roles are
    // logged (present or not, never the name) because the likeliest cause of "nothing heard" is a default that is not the mic in use.
    let mic = crate::audio::default_mic_name();
    let mic_comm = crate::audio::default_mic_name_for(windows::Win32::Media::Audio::eCommunications);
    log(&session_start_line(lang, auto_stop_sec, max_sec, interim, mic.as_deref(), mic_comm.as_deref()));
    let diag = Arc::new(Diag::default());

    let language = Language::CreateLanguage(&HSTRING::from(lang)).map_err(|e| fail("language", &e))?;
    let recognizer = SpeechRecognizer::Create(&language).map_err(|e| fail("create", &e))?;
    let constraint = SpeechRecognitionTopicConstraint::Create(SpeechRecognitionScenario::Dictation, &HSTRING::from("dictation"))
        .map_err(|e| fail("constraint", &e))?;
    recognizer.Constraints().and_then(|c| c.Append(&constraint)).map_err(|e| fail("constraint-append", &e))?;

    // Diagnostics only: a failed registration here never fails the session, it just leaves the
    // summary line without that detail.
    {
        let d = diag.clone();
        if let Err(e) = recognizer.StateChanged(&TypedEventHandler::new(
            move |_sender: Ref<SpeechRecognizer>, args: Ref<SpeechRecognizerStateChangedEventArgs>| {
                if let Some(Ok(state)) = args.as_ref().map(|a| a.State()) {
                    Diag::note(&d.states, state.0);
                }
                Ok(())
            },
        )) {
            log(&format!("StateChanged not registered hr={:#010x}", e.code().0 as u32));
        }
        let d = diag.clone();
        if let Err(e) = recognizer.RecognitionQualityDegrading(&TypedEventHandler::new(
            move |_sender: Ref<SpeechRecognizer>, args: Ref<SpeechRecognitionQualityDegradingEventArgs>| {
                if let Some(Ok(problem)) = args.as_ref().map(|a| a.Problem()) {
                    Diag::note(&d.problems, problem.0);
                }
                Ok(())
            },
        )) {
            log(&format!("RecognitionQualityDegrading not registered hr={:#010x}", e.code().0 as u32));
        }
    }

    let compiled = recognizer.CompileConstraintsAsync().and_then(|a| a.join()).map_err(|e| fail("compile", &e))?;
    let compile_status = compiled.Status().map_err(|e| fail("compile-status", &e))?.0;
    log(&format!("compile status={compile_status}"));
    if let Some(key) = compile_error_key(compile_status) {
        log(&format!("compile refused key={key}"));
        return Err(key);
    }
    let session = recognizer.ContinuousRecognitionSession().map_err(|e| fail("session", &e))?;

    // Windows' AutoStopSilenceTimeout is a redundant backstop; the app's own idle watchdog below
    // decides. With voiceAutoStop off (auto_stop_sec == 0) it is set to the hard cap so it never
    // fires earlier than max_sec.
    let watch_secs = if auto_stop_sec > 0 { auto_stop_sec } else { max_sec };
    if let Err(e) = session.SetAutoStopSilenceTimeout(windows::Foundation::TimeSpan { Duration: watch_secs as i64 * 10_000_000 }) {
        log(&format!("AutoStopSilenceTimeout not set hr={:#010x}", e.code().0 as u32));
    }

    let last_activity = Arc::new(Mutex::new(Instant::now()));
    // What voice_stop or the watchdog set right before StopAsync, so the Completed handler
    // (status == Success) knows why it was asked to stop. A Windows-detected silence timeout
    // reports status 7 on its own.
    let requested_reason: Arc<Mutex<Option<&'static str>>> = Arc::new(Mutex::new(None));

    // Registered whether or not partial text is shown: a hypothesis is also proof the person is
    // still talking, and the idle watchdog must not stop a long sentence that has no result yet.
    {
        let app2 = app.clone();
        let last2 = last_activity.clone();
        let diag2 = diag.clone();
        // A failure here must fail the session up front like every other WinRT setup call: otherwise
        // recording would start with the idle clock blind to speech in progress.
        recognizer
            .HypothesisGenerated(&TypedEventHandler::new(
                move |_sender: Ref<SpeechRecognizer>, args: Ref<SpeechRecognitionHypothesisGeneratedEventArgs>| {
                    *last2.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
                    diag2.hyps.fetch_add(1, Ordering::SeqCst);
                    if interim {
                        if let Some(args) = args.as_ref() {
                            if let Ok(h) = args.Hypothesis() {
                                if let Ok(text) = h.Text() {
                                    let _ = app2.emit("voice-partial", VoiceTextEvent { id, token, text: text.to_string() });
                                }
                            }
                        }
                    }
                    Ok(())
                },
            ))
            .map_err(|e| fail("hypothesis-handler", &e))?;
    }

    let app_r = app.clone();
    let last_r = last_activity.clone();
    let diag_r = diag.clone();
    // Same as HypothesisGenerated: a session started without this would record forever and
    // never deliver text.
    session
        .ResultGenerated(&TypedEventHandler::new(
            move |_sender: Ref<SpeechContinuousRecognitionSession>, args: Ref<SpeechContinuousRecognitionResultGeneratedEventArgs>| {
                *last_r.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
                if let Some(args) = args.as_ref() {
                    if let Ok(result) = args.Result() {
                        let status = result.Status().map(|s| s.0).unwrap_or(-1);
                        let confidence = result.Confidence().map(|c| c.0).unwrap_or(-1);
                        match result.Text() {
                            Ok(text) => {
                                let text = text.to_string();
                                // Only the code and the empty flag are logged, never the text itself.
                                log(&format!("result status={status} confidence={confidence} empty={}", text.is_empty()));
                                // Nothing recognized in this segment -> nothing to insert.
                                if !text.is_empty() {
                                    diag_r.results.fetch_add(1, Ordering::SeqCst);
                                    let _ = app_r.emit("voice-text", VoiceTextEvent { id, token, text });
                                }
                            }
                            Err(e) => log(&format!("result text unreadable status={status} hr={:#010x}", e.code().0 as u32)),
                        }
                    } else {
                        log("result unreadable");
                    }
                }
                Ok(())
            },
        ))
        .map_err(|e| fail("result-handler", &e))?;

    let app_c = app.clone();
    let ended_c = ended.clone();
    let reason_c = requested_reason.clone();
    let diag_c = diag.clone();
    let mic_c = mic.clone();
    // The most important registration: if it fails, a session otherwise running fine would end
    // without telling the window, and the microphone icon would never clear.
    session
        .Completed(&TypedEventHandler::new(
            move |_sender: Ref<SpeechContinuousRecognitionSession>, args: Ref<SpeechContinuousRecognitionCompletedEventArgs>| {
                // Fires once per session; the swap makes a repeated call a no-op instead of a duplicate event.
                if ended_c.swap(true, Ordering::SeqCst) {
                    return Ok(());
                }
                // A status outside the stop_reason table's named cases (0/7) only occurs on failure: an
                // unreadable one maps to -1, which resolves to "error".
                let status = match args.as_ref().map(|a| a.Status()) {
                    Some(Ok(s)) => s.0,
                    _ => {
                        log("completed status unreadable");
                        -1
                    }
                };
                let requested = reason_c.lock().unwrap_or_else(|e| e.into_inner()).take();
                let reason = stop_reason(status, requested);
                let error = end_error_key(status, reason, diag_c.heard());
                log(&diag_c.line(Some(status), reason, session_started));
                if let Some(key) = error {
                    log(&format!("reported to the window key={key}"));
                }
                emit_end(&app_c, id, token, reason, error, mic_c.as_deref());
                Ok(())
            },
        ))
        .map_err(|e| fail("completed-handler", &e))?;

    session.StartAsync().and_then(|a| a.join()).map_err(|e| fail("start", &e))?;
    log("recognition started");

    // A stop the thread itself decided on (hard cap / idle watchdog): fires StopAsync, then
    // waits for Completed to flip `ended`, like an external stop.
    let self_stop = |reason: &'static str| {
        *requested_reason.lock().unwrap_or_else(|e| e.into_inner()) = Some(reason);
        if let Err(e) = session.StopAsync() {
            // Windows refused the stop request itself (a session that ended on its own already set
            // `ended` via Completed). Completed will almost certainly never fire, so report the failure
            // immediately instead of waiting the timeout with the mic open, and not with the intended reason.
            log(&format!("StopAsync refused reason={reason} hr={:#010x}", e.code().0 as u32));
            if !ended.swap(true, Ordering::SeqCst) {
                log(&diag.line(None, "error", session_started));
                emit_end(app, id, token, "error", Some("voice-failed"), None);
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
                    log("Completed never arrived after StopAsync");
                    log(&diag.line(None, reason, session_started));
                    let error = end_error_key(STATUS_SUCCESS, reason, diag.heard());
                    emit_end(app, id, token, reason, error, mic.as_deref());
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
