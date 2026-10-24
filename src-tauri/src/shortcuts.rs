use super::*;

// ---------- Global shortcuts: the app hotkey + one per prompt ----------

pub(crate) type Accel = tauri_plugin_global_shortcut::Shortcut;

// An accelerator is user input on its way to the OS shortcut API, so its length is
// capped before anything else looks at it.
pub(crate) const MAX_ACCEL_LEN: usize = 64;

// Rejection codes. The UI turns them into a localized message; they are the whole
// vocabulary the hotkey path speaks.
pub(crate) const HK_INVALID: &str = "hotkey-invalid"; // unusable shape (no Ctrl/Alt, Win key, garbage)
pub(crate) const HK_SYSTEM: &str = "hotkey-system"; // Windows or every app owns it
pub(crate) const HK_TAKEN: &str = "hotkey-taken"; // already bound inside this app
pub(crate) const HK_CONFLICT: &str = "hotkey-conflict"; // another running app holds the registration

// Keys a global hotkey may never take, whatever the modifiers: a registered accelerator
// swallows the keystroke system-wide. Tab/Escape/Enter are navigation and confirm/cancel,
// Backspace and Delete are word-wise editing (Ctrl+Backspace), Insert has the clipboard
// aliases (Ctrl/Shift+Insert), Space is the IME switch on CJK systems and the window menu
// with Alt, PrintScreen is screen capture, and the lock keys are hardware state. The list
// also covers Ctrl+Alt+Del, Ctrl+Shift+Esc, Ctrl+Esc, Alt+Tab and Alt+Space without naming
// them.
pub(crate) const RESERVED_KEYS: &[Code] = &[
    Code::Tab,
    Code::Escape,
    Code::Enter,
    Code::NumpadEnter,
    Code::Backspace,
    Code::Delete,
    Code::Insert,
    Code::Space,
    Code::PrintScreen,
    Code::CapsLock,
    Code::NumLock,
    Code::ScrollLock,
    Code::Pause,
];

// Ctrl+<key> combinations every Windows application implements the same way; taking one
// globally would remove copy, paste or save in every program. Only the plain Ctrl form is
// blocked: adding Shift or Alt makes a distinct combination, the namespace users pick their
// own shortcuts from.
pub(crate) const UNIVERSAL_CTRL: &[Code] = &[
    Code::KeyA, // select all
    Code::KeyC, // copy
    Code::KeyV, // paste
    Code::KeyX, // cut
    Code::KeyZ, // undo
    Code::KeyY, // redo
    Code::KeyS, // save
    Code::KeyF, // find
    Code::KeyP, // print
    Code::KeyN, // new
    Code::KeyO, // open
    Code::KeyW, // close document
    Code::KeyT, // new tab
    Code::F4,   // close document (MDI / browser tab)
];

// The pure decision: may this accelerator become a global hotkey? Some(code) = no.
// Kept free of any app state so the rule set is unit-testable on its own.
pub(crate) fn hotkey_block_reason(sc: &Accel) -> Option<&'static str> {
    // Windows owns the whole Win-key namespace; RegisterHotKey either fails there or
    // shadows a system shortcut the user cannot get back.
    if sc.mods.intersects(Modifiers::SUPER | Modifiers::META) {
        return Some(HK_SYSTEM);
    }
    // Shift alone is not a modifier for this purpose: Shift+A is how a capital A is
    // typed, so binding it would kill typing everywhere. Require Ctrl or Alt.
    if !sc.mods.intersects(Modifiers::CONTROL | Modifiers::ALT) {
        return Some(HK_INVALID);
    }
    if RESERVED_KEYS.contains(&sc.key) {
        return Some(HK_SYSTEM);
    }
    let plain_ctrl = sc.mods == Modifiers::CONTROL;
    if plain_ctrl && UNIVERSAL_CTRL.contains(&sc.key) {
        return Some(HK_SYSTEM);
    }
    // Alt+F4 closes the focused window in every Windows program.
    if sc.mods == Modifiers::ALT && sc.key == Code::F4 {
        return Some(HK_SYSTEM);
    }
    None
}

// The single place an accelerator is validated, with the reason it failed. Unusable:
// empty, over-long, unparseable, without a modifier (a bare key would swallow that
// key in every other app), or on the blocklist above. Pure, so validation and
// conflict handling stay testable.
pub(crate) fn accel_error(raw: &str) -> Result<Accel, &'static str> {
    let s = raw.trim();
    if s.is_empty() || s.len() > MAX_ACCEL_LEN || s.chars().any(|c| c.is_control()) {
        return Err(HK_INVALID);
    }
    let sc: Accel = s.parse().map_err(|_| HK_INVALID)?;
    if sc.mods.is_empty() {
        return Err(HK_INVALID);
    }
    match hotkey_block_reason(&sc) {
        Some(code) => Err(code),
        None => Ok(sc),
    }
}

// None = unusable, for the paths that only need to know whether to register it.
pub(crate) fn parse_accel(raw: &str) -> Option<Accel> {
    accel_error(raw).ok()
}

// Which feature a registered accelerator belongs to. A plain `Option<String>` (None = app
// hotkey) stopped being enough once voice dictation needed its own owner; a sentinel prompt
// id would pass as a real Prompt(id) in rearm_hotkeys and the handler below, so a real enum
// is used.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum HotkeyOwner {
    App,
    Prompt(String),
    Voice,
    // Registered only while a recording is running (rearm_hotkeys checks
    // RecPhase), so a stop/pause combination never steals input from another
    // app the rest of the time.
    RecStop,
    RecPause,
}

// Every accelerator that should be registered, in registration order. Unusable accelerators
// and ones an earlier entry already claimed are skipped: the app hotkey wins, then voice
// dictation, then the two recording controls, then prompts in list order. `per_prompt` is
// the promptHotkeys toggle: off leaves the stored accelerators alone and registers none.
// `voice_hotkey` is Some only when both the `voice` and `voiceHotkey` flags are on;
// `rec_stop_hotkey`/`rec_pause_hotkey` are Some only while a recording runs and their own
// toggle is on. The caller has already resolved these.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hotkey_plan(
    app_hotkey: &str,
    voice_hotkey: Option<&str>,
    rec_stop_hotkey: Option<&str>,
    rec_pause_hotkey: Option<&str>,
    prompts: &[Prompt],
    per_prompt: bool,
) -> Vec<(Accel, HotkeyOwner)> {
    let mut plan = Vec::new();
    let mut taken: std::collections::HashSet<Accel> = std::collections::HashSet::new();
    if let Some(sc) = parse_accel(app_hotkey) {
        taken.insert(sc);
        plan.push((sc, HotkeyOwner::App));
    }
    if let Some(vh) = voice_hotkey {
        if let Some(sc) = parse_accel(vh) {
            if taken.insert(sc) {
                plan.push((sc, HotkeyOwner::Voice));
            }
        }
    }
    for (hk, owner) in [(rec_stop_hotkey, HotkeyOwner::RecStop), (rec_pause_hotkey, HotkeyOwner::RecPause)] {
        if let Some(hk) = hk {
            if let Some(sc) = parse_accel(hk) {
                if taken.insert(sc) {
                    plan.push((sc, owner));
                }
            }
        }
    }
    if per_prompt {
        for p in prompts {
            if let Some(sc) = parse_accel(&p.hotkey) {
                if taken.insert(sc) {
                    plan.push((sc, HotkeyOwner::Prompt(p.id.clone())));
                }
            }
        }
    }
    plan
}

// A restored backup can carry an accelerator the app hotkey or an earlier prompt
// already owns. The import must not fail over that, so the losing binding is
// dropped and counted for the summary.
// `reserved` are the other global combinations that outrank a prompt binding (the
// voice and recording hotkeys, see `reserved_hotkeys`).
pub(crate) fn drop_colliding_hotkeys(app_hotkey: &str, reserved: &[&str], prompts: &mut [Prompt]) -> usize {
    let mut taken: std::collections::HashSet<Accel> = std::collections::HashSet::new();
    if let Some(sc) = parse_accel(app_hotkey) {
        taken.insert(sc);
    }
    taken.extend(reserved.iter().filter_map(|hk| parse_accel(hk)));
    let mut dropped = 0;
    for p in prompts.iter_mut() {
        if p.hotkey.trim().is_empty() {
            p.hotkey = String::new();
            continue;
        }
        match parse_accel(&p.hotkey) {
            Some(sc) if taken.insert(sc) => {}
            _ => {
                p.hotkey = String::new();
                dropped += 1;
            }
        }
    }
    dropped
}

// Check an accelerator from the editor against everything already bound. Empty clears the
// binding; the error codes become a localized inline message in the UI.
// rec_stop_hotkey/rec_pause_hotkey are checked only while their own feature is on.
// Checking them here matters: hotkey_plan gives the recording controls priority, so a
// prompt hotkey with the same combination would silently stop firing during every
// recording.
pub(crate) fn sanitize_hotkey(
    raw: &str,
    app_hotkey: &str,
    voice_hotkey: &str,
    rec_stop_hotkey: &str,
    rec_pause_hotkey: &str,
    others: &[&Prompt],
) -> Result<String, String> {
    let hk = raw.trim();
    if hk.is_empty() {
        return Ok(String::new());
    }
    let sc = accel_error(hk).map_err(|e| e.to_string())?;
    if parse_accel(app_hotkey) == Some(sc)
        || parse_accel(voice_hotkey) == Some(sc)
        || parse_accel(rec_stop_hotkey) == Some(sc)
        || parse_accel(rec_pause_hotkey) == Some(sc)
        || others.iter().any(|p| parse_accel(&p.hotkey) == Some(sc))
    {
        return Err(HK_TAKEN.to_string());
    }
    Ok(hk.to_string())
}

// The voice accelerator to check prompt hotkeys against, or "" while the voice
// feature or its own hotkey switch is off (the same gate rearm_hotkeys applies).
pub(crate) fn active_voice_hotkey<'a>(hk: &'a str, flags: &HashMap<String, bool>) -> &'a str {
    if flags.get("voice") != Some(&false) && flags.get("voiceHotkey") == Some(&true) {
        hk
    } else {
        ""
    }
}

// Every global combination except the app hotkey that outranks a prompt binding.
pub(crate) fn reserved_hotkeys(settings: &Settings) -> Vec<String> {
    vec![
        active_voice_hotkey(&settings.voice_hotkey, &settings.ui_flags).to_string(),
        active_rec_hotkey(&settings.rec_stop_hotkey, "recStopHotkey", &settings.ui_flags).to_string(),
        active_rec_hotkey(&settings.rec_pause_hotkey, "recPauseHotkey", &settings.ui_flags).to_string(),
    ]
}

// The rec-stop/rec-pause accelerator to check prompt hotkeys against, or ""
// when that control's own feature/hotkey toggle is off and the combination is
// therefore free to reuse. Mirrors rearm_hotkeys' own gating minus
// recording_active(): a prompt hotkey is set long before any recording runs.
pub(crate) fn active_rec_hotkey<'a>(hk: &'a str, flag_key: &str, flags: &HashMap<String, bool>) -> &'a str {
    if flags.get("screenRecord") != Some(&false) && flags.get(flag_key) != Some(&false) {
        hk
    } else {
        ""
    }
}

// What a prompt hotkey does after copying. Empty = follow the global setting, so a
// stored value is only ever one of three states.
pub(crate) fn sanitize_hotkey_mode(raw: &str) -> String {
    match raw.trim() {
        "copy" => "copy".to_string(),
        "paste" => "paste".to_string(),
        _ => String::new(),
    }
}

// Which feature each registered accelerator belongs to. Rebuilt by
// rearm_hotkeys, read by the global shortcut handler.
pub(crate) struct HotkeyTargets(pub(crate) Mutex<Vec<(Accel, HotkeyOwner)>>);

// Whether a recording is currently in flight (Recording or Paused); both configs of
// RecordingState just wrap an Option, so no windows-only gating is needed.
pub(crate) fn recording_active(app: &AppHandle) -> bool {
    app.try_state::<RecordingState>()
        .map(|s| s.0.lock().unwrap_or_else(|e| e.into_inner()).is_some())
        .unwrap_or(false)
}

// The running session's phase, for the two callers that need to tell Paused
// from Recording (the tray menu's Pause/Resume label) rather than just
// active-or-not. Windows-only build carries a real RecSession with a phase; the
// non-Windows stub's Option<()> has none, so it always reports "not recording".
#[cfg(windows)]
pub(crate) fn recording_phase(app: &AppHandle) -> Option<RecPhase> {
    app.try_state::<RecordingState>()?.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|s| s.phase)
}

// Whether the last rearm_hotkeys() actually registered the stop/pause combination, read by
// recording_status() so the bar can warn instead of the user believing in a binding that
// never fires. Unlike the app/voice hotkey, a conflict here never clears the stored
// default: it ships pre-picked, and a transient collision with another app should not
// blank it permanently.
pub(crate) static REC_STOP_HOTKEY_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
pub(crate) static REC_PAUSE_HOTKEY_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
// Whether the last rearm_hotkeys() could claim the voice hotkey. The refused key is only cleared
// from the store when that write lands, so the store alone cannot answer "did it register".
pub(crate) static VOICE_HOTKEY_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

// Runs of rearm_hotkeys never overlap: a second run interleaving with the first would
// register accelerators the first just registered, read that as another app holding them,
// and clear the user's hotkeys from the store. A caller arriving during a run does not wait
// (registering hops to the main thread, so a waiting main-thread caller would block the
// run); it asks the running one to go around once more, which re-reads the settings and so
// includes the caller's change.
static REARM_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static REARM_AGAIN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// Releases the running flag if a run panics; a normal exit releases it itself.
struct RearmRunning(bool);
impl Drop for RearmRunning {
    fn drop(&mut self) {
        if self.0 {
            REARM_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

// The single owner of the registered shortcut set: drop everything, then register the app
// hotkey plus one accelerator per prompt. A combination the OS refuses is owned by another
// app: it is cleared from the store instead of leaving a binding the user believes in.
// Returns false when the APP hotkey was the refused one. Must never run while the store
// mutex is held: registering hops to the main thread, and the shortcut handler reads the
// store.
pub(crate) fn rearm_hotkeys(app: &AppHandle) -> bool {
    rearm_hotkeys_claimed(app).unwrap_or(true)
}

// Like rearm_hotkeys, but waits (bounded) until a pass of its OWN has run, so the
// answer covers the caller's change: a coalesced caller only knows a running pass
// will go around again, not what that pass found. Only for callers off the main
// thread (async commands): registering hops to the main thread, and a main-thread
// caller waiting here would block the pass it waits for.
pub(crate) fn rearm_hotkeys_exact(app: &AppHandle) -> bool {
    for _ in 0..50 {
        if let Some(ok) = rearm_hotkeys_claimed(app) {
            return ok;
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
    true
}

// None when another run was going and was asked to repeat instead.
fn rearm_hotkeys_claimed(app: &AppHandle) -> Option<bool> {
    use std::sync::atomic::Ordering::SeqCst;
    if !claim_rearm(&REARM_RUNNING, &REARM_AGAIN) {
        return None;
    }
    let mut running = RearmRunning(true);
    loop {
        REARM_AGAIN.store(false, SeqCst);
        let ok = rearm_hotkeys_once(app);
        // Released before the last look at the request flag: a request that lands
        // after this point finds no runner and runs itself, one before it is seen here.
        REARM_RUNNING.store(false, SeqCst);
        running.0 = false;
        if REARM_AGAIN.load(SeqCst) && !REARM_RUNNING.swap(true, SeqCst) {
            running.0 = true;
            continue;
        }
        return Some(ok);
    }
}

// True when the caller became the runner; otherwise the running one is asked to repeat.
// The request is stored BEFORE the claim: stored after it, the runner could release
// the running flag and read the request flag in between and never see it, leaving
// nobody to run. A winner's early store is harmless, the runner clears it at the top
// of its loop.
pub(crate) fn claim_rearm(running: &std::sync::atomic::AtomicBool, again: &std::sync::atomic::AtomicBool) -> bool {
    use std::sync::atomic::Ordering::SeqCst;
    again.store(true, SeqCst);
    !running.swap(true, SeqCst)
}

fn rearm_hotkeys_once(app: &AppHandle) -> bool {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let rec_active = recording_active(app);
    // A binding stored by an older version can be one the blocklist now refuses.
    // It is never registered again, so the stored string is cleared here: otherwise
    // the editor keeps showing a hotkey that does nothing.
    let (plan, mut dropped, mut app_ok, mut voice_ok, mut rec_stop_ok, mut rec_pause_ok) = match app.try_state::<Db>() {
        Some(state) => {
            let store = lock(&state);
            let per_prompt = store.settings.ui_flags.get("promptHotkeys") != Some(&false);
            let stale: Vec<String> = store
                .prompts
                .iter()
                .filter(|p| !p.hotkey.trim().is_empty() && parse_accel(&p.hotkey).is_none())
                .map(|p| p.id.clone())
                .collect();
            let app_ok = store.settings.hotkey.trim().is_empty()
                || parse_accel(&store.settings.hotkey).is_some();
            // Registered only while both the feature and its own opt-in flag are on; switching either
            // off unregisters it without touching the stored string, like promptHotkeys for per_prompt
            // above.
            let voice_hotkey_on = store.settings.ui_flags.get("voice") != Some(&false)
                && store.settings.ui_flags.get("voiceHotkey") == Some(&true);
            let voice_hotkey = voice_hotkey_on.then_some(store.settings.voice_hotkey.as_str());
            let voice_ok = !voice_hotkey_on
                || store.settings.voice_hotkey.trim().is_empty()
                || parse_accel(&store.settings.voice_hotkey).is_some();
            // Registered only while a recording is actually running: see
            // recording_active above: AND their own toggle is on (both ship ON).
            let rec_stop_on = rec_active
                && store.settings.ui_flags.get("screenRecord") != Some(&false)
                && store.settings.ui_flags.get("recStopHotkey") != Some(&false);
            let rec_pause_on = rec_active
                && store.settings.ui_flags.get("screenRecord") != Some(&false)
                && store.settings.ui_flags.get("recPauseHotkey") != Some(&false);
            let rec_stop_hotkey = rec_stop_on.then_some(store.settings.rec_stop_hotkey.as_str());
            let rec_pause_hotkey = rec_pause_on.then_some(store.settings.rec_pause_hotkey.as_str());
            let rec_stop_ok = !rec_stop_on
                || store.settings.rec_stop_hotkey.trim().is_empty()
                || parse_accel(&store.settings.rec_stop_hotkey).is_some();
            let rec_pause_ok = !rec_pause_on
                || store.settings.rec_pause_hotkey.trim().is_empty()
                || parse_accel(&store.settings.rec_pause_hotkey).is_some();
            (
                hotkey_plan(&store.settings.hotkey, voice_hotkey, rec_stop_hotkey, rec_pause_hotkey, &store.prompts, per_prompt),
                stale,
                app_ok,
                voice_ok,
                rec_stop_ok,
                rec_pause_ok,
            )
        }
        None => return true,
    };
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut live = Vec::with_capacity(plan.len());
    for (sc, owner) in plan {
        if gs.register(sc).is_ok() {
            live.push((sc, owner));
        } else {
            match owner {
                HotkeyOwner::Prompt(id) => dropped.push(id),
                HotkeyOwner::App => app_ok = false,
                HotkeyOwner::Voice => voice_ok = false,
                HotkeyOwner::RecStop => rec_stop_ok = false,
                HotkeyOwner::RecPause => rec_pause_ok = false,
            }
        }
    }
    if let Some(targets) = app.try_state::<HotkeyTargets>() {
        *targets.0.lock().unwrap_or_else(|e| e.into_inner()) = live;
    }
    VOICE_HOTKEY_OK.store(voice_ok, std::sync::atomic::Ordering::Relaxed);
    REC_STOP_HOTKEY_OK.store(rec_stop_ok, std::sync::atomic::Ordering::Relaxed);
    REC_PAUSE_HOTKEY_OK.store(rec_pause_ok, std::sync::atomic::Ordering::Relaxed);
    if !app_ok || !voice_ok || !dropped.is_empty() {
        if let Some(state) = app.try_state::<Db>() {
            let mut store = lock(&state);
            clear_refused_hotkeys(&mut store, app_ok, voice_ok, &dropped, &mut |st| save_prompts(app, st), &mut |st| save_settings_paired(app, &st.settings));
        }
    }
    app_ok
}

// Clears the refused hotkeys from the store and writes the result. The clear stays only when
// every write landed; otherwise the old values go back in memory (and on disk, if the prompt rows
// were already rewritten), so the next start does not meet a state the disk never had. The next
// rearm simply refuses the key again, so a persistent write failure repeats this quietly.
pub(crate) fn clear_refused_hotkeys(
    store: &mut Store,
    app_ok: bool,
    voice_ok: bool,
    dropped: &[String],
    save_prompts_fn: &mut dyn FnMut(&Store) -> bool,
    save_settings_fn: &mut dyn FnMut(&Store) -> bool,
) -> bool {
    let old_hotkey = store.settings.hotkey.clone();
    let old_voice = store.settings.voice_hotkey.clone();
    let old_prompt_keys: Vec<(String, String)> = store
        .prompts
        .iter()
        .filter(|p| dropped.contains(&p.id))
        .map(|p| (p.id.clone(), p.hotkey.clone()))
        .collect();
    if !app_ok {
        store.settings.hotkey = String::new();
    }
    if !voice_ok {
        store.settings.voice_hotkey = String::new();
    }
    for p in store.prompts.iter_mut().filter(|p| dropped.contains(&p.id)) {
        p.hotkey = String::new();
    }
    let prompts_written = dropped.is_empty() || save_prompts_fn(store);
    if prompts_written && save_settings_fn(store) {
        return true;
    }
    store.settings.hotkey = old_hotkey;
    store.settings.voice_hotkey = old_voice;
    for (id, key) in old_prompt_keys {
        if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
            p.hotkey = key;
        }
    }
    if prompts_written && !dropped.is_empty() && !save_prompts_fn(store) {
        append_crash_log("persist failed: the undo of a cleared hotkey could not be written");
    }
    append_crash_log("persist failed: a refused hotkey could not be cleared in the store");
    false
}

// A prompt hotkey runs the very same activation a tile click does. The UI owns
// copying, usage recording, chain handling and placeholder filling, so the backend
// only names the prompt that was triggered.
pub(crate) fn fire_prompt_hotkey(app: &AppHandle, id: &str) {
    let _ = app.emit_to("main", "prompt-hotkey", id.to_string());
}

// One key press starts, the next stops, like the button but without bringing the window
// forward. A running session is stopped as voice_stop does (send "manual" on its channel);
// starting is left to the main window's JS, which owns the setting lookups, command-word
// post-processing and where the text goes, and works while hidden like a prompt hotkey copy.
pub(crate) fn toggle_voice_from_hotkey(app: &AppHandle) {
    if let Some(vstate) = app.try_state::<VoiceState>() {
        let guard = vstate.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = guard.as_ref() {
            if !h.ended.load(std::sync::atomic::Ordering::SeqCst) {
                let _ = h.stop.send("manual");
                return;
            }
        }
    }
    let _ = app.emit_to("main", "voice-hotkey-toggle", ());
}

// The tray/hotkey counterparts of the recording bar's Stop/Pause buttons: same commands,
// never a second implementation. Both run on a throwaway thread: the global-shortcut
// handler fires on the main thread, and stopping finalizes the encoder (disk I/O), which
// must not stall the message loop.
#[cfg(windows)]
pub(crate) fn trigger_rec_stop_hotkey(app: &AppHandle) {
    let app = app.clone();
    let _ = std::thread::Builder::new().name("rec-hotkey-stop".into()).spawn(move || {
        // A refused stop (a continue holds the session out of the state) leaves the
        // bar up: it still controls the recording and closes itself once that ends.
        if stop_recording_now(&app).is_ok() {
            if let Some(w) = app.get_webview_window("rec-bar") {
                let _ = w.hide();
            }
        }
    });
}

// The one place that pauses/resumes a session: the recbar buttons, the global hotkey and
// the tray item all end up here, so the key-overlay capture (keylog::pause/resume) is
// paused in exactly one spot.
#[cfg(windows)]
pub(crate) fn do_pause_recording(session: &mut recording::RecSession) {
    recording::pause(session);
    session.phase = RecPhase::Paused;
    // The key-overlay capture drops every key struck while paused and freezes the
    // active-recording clock keys are timed against, like RecordClock does for the video track.
    keylog::pause();
}

#[cfg(windows)]
pub(crate) fn do_resume_recording(session: &mut recording::RecSession) {
    recording::resume(session);
    session.phase = RecPhase::Recording;
    keylog::resume();
}

#[cfg(windows)]
pub(crate) fn trigger_rec_pause_hotkey(app: &AppHandle) {
    let toggled = if let Some(rstate) = app.try_state::<RecordingState>() {
        let mut guard = rstate.0.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_mut() {
            Some(session) if rec_can_pause(session.phase) => {
                do_pause_recording(session);
                true
            }
            Some(session) if rec_can_resume(session.phase) => {
                do_resume_recording(session);
                true
            }
            _ => false,
        }
    } else {
        false
    };
    if toggled {
        apply_tray_menu(app); // keep the tray's Pause/Resume label in sync
        emit_recording_state(app);
    }
}

// While a hotkey field is capturing, every global hotkey has to be off. Windows
// delivers a registered combination to the owning process instead of the focused
// window, so the field would never see the key press: the user just watches the
// prompt behind it get copied and gets no error at all.
#[tauri::command]
pub(crate) async fn set_hotkey_capture(app: AppHandle, active: bool) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    if active {
        let _ = app.global_shortcut().unregister_all();
        if let Some(t) = app.try_state::<HotkeyTargets>() {
            t.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
    } else {
        rearm_hotkeys(&app);
    }
    Ok(())
}

// Ask the OS whether a combination is free: register it, then immediately give it
// back. Windows only refuses a combination another PROCESS holds, which is exactly
// the conflict the user needs to hear about before the binding is stored. A
// combination we already hold is ours, not a conflict.
pub(crate) fn accel_is_free(app: &AppHandle, sc: Accel) -> bool {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let gs = app.global_shortcut();
    if gs.is_registered(sc) {
        return true;
    }
    match gs.register(sc) {
        Ok(()) => {
            let _ = gs.unregister(sc);
            true
        }
        Err(_) => false,
    }
}

// Whether a combination is already bound inside the app. The recording stop and
// pause keys only count while recording and their own switch are on, the same gate
// the arming side applies.
pub(crate) fn hotkey_in_use(settings: &Settings, prompts: &[Prompt], editing: Option<&str>, sc: Accel) -> bool {
    let flags = &settings.ui_flags;
    parse_accel(&settings.hotkey) == Some(sc)
        || parse_accel(&settings.voice_hotkey) == Some(sc)
        || parse_accel(active_rec_hotkey(&settings.rec_stop_hotkey, "recStopHotkey", flags)) == Some(sc)
        || parse_accel(active_rec_hotkey(&settings.rec_pause_hotkey, "recPauseHotkey", flags)) == Some(sc)
        || prompts.iter().any(|p| Some(p.id.as_str()) != editing && parse_accel(&p.hotkey) == Some(sc))
}

// Everything the editor needs to know BEFORE a binding is stored: shape, blocklist,
// duplicates inside the app, and whether another running app already owns it.
// `id` is the prompt being edited (its own current binding is not a duplicate).
#[tauri::command]
pub(crate) async fn check_hotkey(
    app: AppHandle,
    state: State<'_, Db>,
    hotkey: String,
    id: Option<String>,
) -> Result<(), String> {
    if hotkey.trim().is_empty() {
        return Ok(()); // clearing a binding needs no checks
    }
    let sc = accel_error(&hotkey).map_err(|e| e.to_string())?;
    // Read what is bound, then drop the lock: registering hops to the main thread.
    let taken = {
        let store = lock(&state);
        hotkey_in_use(&store.settings, &store.prompts, id.as_deref(), sc)
    };
    if taken {
        return Err(HK_TAKEN.to_string());
    }
    if !accel_is_free(&app, sc) {
        return Err(HK_CONFLICT.to_string());
    }
    Ok(())
}

// The four global hotkey settings. Each is checked against the other three and every
// prompt binding before it is stored.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum HotkeySlot {
    App,
    Voice,
    RecStop,
    RecPause,
}

const HOTKEY_SLOTS: [HotkeySlot; 4] = [HotkeySlot::App, HotkeySlot::Voice, HotkeySlot::RecStop, HotkeySlot::RecPause];

impl HotkeySlot {
    fn get(self, s: &Settings) -> &str {
        match self {
            HotkeySlot::App => &s.hotkey,
            HotkeySlot::Voice => &s.voice_hotkey,
            HotkeySlot::RecStop => &s.rec_stop_hotkey,
            HotkeySlot::RecPause => &s.rec_pause_hotkey,
        }
    }

    fn set(self, s: &mut Settings, hk: &str) {
        match self {
            HotkeySlot::App => s.hotkey = hk.to_string(),
            HotkeySlot::Voice => s.voice_hotkey = hk.to_string(),
            HotkeySlot::RecStop => s.rec_stop_hotkey = hk.to_string(),
            HotkeySlot::RecPause => s.rec_pause_hotkey = hk.to_string(),
        }
    }

    // The stop/pause keys are only registered while a recording runs, so another
    // app's claim on them cannot be probed when they are set.
    fn probes_for_conflicts(self) -> bool {
        matches!(self, HotkeySlot::App | HotkeySlot::Voice)
    }
}

// Whether `sc` is already held by another hotkey slot or a prompt. The stop/pause and voice keys count
// only while their feature and switch are on (the same gate hotkey_in_use applies), so a key
// nothing uses is never reported as taken.
pub(crate) fn slot_key_taken(settings: &Settings, prompts: &[Prompt], slot: HotkeySlot, sc: Accel) -> bool {
    let flags = &settings.ui_flags;
    HOTKEY_SLOTS
        .iter()
        .filter(|other| **other != slot)
        .any(|other| {
            let hk = match other {
                HotkeySlot::RecStop => active_rec_hotkey(other.get(settings), "recStopHotkey", flags),
                HotkeySlot::RecPause => active_rec_hotkey(other.get(settings), "recPauseHotkey", flags),
                HotkeySlot::Voice => active_voice_hotkey(other.get(settings), flags),
                _ => other.get(settings),
            };
            parse_accel(hk) == Some(sc)
        })
        || prompts.iter().any(|p| parse_accel(&p.hotkey) == Some(sc))
}

// The shared body of the four set_*hotkey commands: validated BEFORE the setting is
// written, so a refused combination leaves the previous one live and the UI field in
// step with what is actually bound; then stored and re-armed together with every
// prompt accelerator (one place owns the registered set).
fn apply_hotkey(app: &AppHandle, state: &State<'_, Db>, slot: HotkeySlot, accel: &str) -> Result<(), String> {
    let hk = accel.trim().to_string();
    if !hk.is_empty() {
        let sc = accel_error(&hk).map_err(|e| e.to_string())?;
        let taken = {
            let store = lock(state);
            slot_key_taken(&store.settings, &store.prompts, slot, sc)
        };
        if taken {
            return Err(HK_TAKEN.to_string());
        }
        if slot.probes_for_conflicts() && !accel_is_free(app, sc) {
            return Err(HK_CONFLICT.to_string());
        }
    }
    {
        let mut store = lock(state);
        slot.set(&mut store.settings, &hk);
        save_settings(app, &store.settings);
    }
    let armed = rearm_hotkeys_exact(app);
    match slot {
        // rearm_hotkeys' return value reports the APP hotkey's fate.
        HotkeySlot::App if !armed => Err(HK_CONFLICT.to_string()),
        // A refused voice hotkey is cleared from the store by rearm_hotkeys itself (unless that
        // write failed), so re-reading it and the registration flag answers "did this one land".
        HotkeySlot::Voice if !hk.is_empty() && (lock(state).settings.voice_hotkey != hk || !VOICE_HOTKEY_OK.load(std::sync::atomic::Ordering::Relaxed)) => Err(HK_CONFLICT.to_string()),
        _ => Ok(()),
    }
}

// Register a new global hotkey (empty = disable); see apply_hotkey.
#[tauri::command]
pub(crate) async fn set_hotkey(app: AppHandle, state: State<'_, Db>, hotkey: String) -> Result<(), String> {
    apply_hotkey(&app, &state, HotkeySlot::App, &hotkey)
}

// The voice-dictation counterpart of set_hotkey.
#[tauri::command]
pub(crate) async fn set_voice_hotkey(app: AppHandle, state: State<'_, Db>, hotkey: String) -> Result<(), String> {
    apply_hotkey(&app, &state, HotkeySlot::Voice, &hotkey)
}

// Stop/pause counterparts of set_voice_hotkey, validated against every other hotkey kind
// (including each other) before the write. They are registered only while a recording runs
// (rearm_hotkeys), so a successful set can still show as blocked later if another app
// claims the combination only then; recording_status() reports that, not this call.
#[tauri::command]
pub(crate) async fn set_rec_stop_hotkey(app: AppHandle, state: State<'_, Db>, hotkey: String) -> Result<(), String> {
    apply_hotkey(&app, &state, HotkeySlot::RecStop, &hotkey)
}

#[tauri::command]
pub(crate) async fn set_rec_pause_hotkey(app: AppHandle, state: State<'_, Db>, hotkey: String) -> Result<(), String> {
    apply_hotkey(&app, &state, HotkeySlot::RecPause, &hotkey)
}

// A hotkey-triggered prompt that has to go through the grid (chain mode) needs
// the main window in front first, so the UI asks for it before activating the tile.
#[tauri::command]
pub(crate) fn focus_main_window(app: AppHandle) {
    show_main(&app);
}
