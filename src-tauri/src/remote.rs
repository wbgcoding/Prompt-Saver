// promptsaver:// links and the local remote-access receiver.
//
// Both let another program or a clicked link reach this app without a real command line.
// Neither implements its own command set: a link's "copy" and every receiver request run
// through cli::run_in_process, the function the running instance's command-line pipe uses
// (see cli.rs), so a store-side rule added there (a locked entry, a disabled command, a new
// validation) covers both automatically. "open" is the one extra action, since the headless
// CLI never needed to put the real window in front of the user.
//
// Both are off by default. The receiver also refuses to start without a password already set
// (see `start`), and a link's first "open" and first "copy" each ask for a one-time native
// confirmation before acting.

use crate::{data_dir, ERROR_LOG_MAX, error_log_enabled, lock, record_error_log, save_settings, show_main, Db, Settings};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_rustls::TlsAcceptor;

// ---------- Settings ----------

const DEFAULT_PORT: u16 = 51823;
const DEFAULT_RATE_LIMIT: f64 = 60.0;
const MAX_BODY_BYTES: usize = 1_000_000; // 1 MB, the spec's own cap
const MAX_HEAD_BYTES: usize = 16 * 1024; // request line + headers only
const MAX_ARGV_LEN: usize = 64;
const READ_TIMEOUT: Duration = Duration::from_secs(10);
const FAIL_LOCKOUT_THRESHOLD: u32 = 10;
const FAIL_LOCKOUT_SECS: u64 = 10 * 60;
const RATE_WINDOW_SECS: u64 = 60;
const SCHEME_NAME: &str = "promptsaver";

fn remote_api_enabled(settings: &Settings) -> bool {
    settings.ui_flags.get("remoteApi") == Some(&true)
}
fn network_filter_enabled(settings: &Settings) -> bool {
    settings.ui_flags.get("remoteNetworkFilter") == Some(&true)
}
// What a reset that turned the stored flags off still owes the outside world: the receiver
// keeps its socket until stopped and the address scheme stays in the registry until removed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ResetSideEffects {
    pub stop_receiver: bool,
    pub unregister_scheme: bool,
}

pub(crate) fn reset_side_effects(old: &Settings, new: &Settings) -> ResetSideEffects {
    ResetSideEffects {
        stop_receiver: remote_api_enabled(old) && !remote_api_enabled(new),
        unregister_scheme: url_scheme_enabled(old) && !url_scheme_enabled(new),
    }
}

// Carries out the effects after the reset was saved.
pub(crate) fn run_reset_side_effects(app: &AppHandle, effects: &ResetSideEffects) {
    if effects.stop_receiver {
        stop(app);
    }
    if effects.unregister_scheme && !crate::url_scheme_removal_skipped() {
        if let Err(e) = apply_url_scheme_registration(false) {
            crate::append_crash_log(&format!("reset: the address scheme could not be removed: {e}"));
        }
    }
}

pub(crate) fn url_scheme_enabled(settings: &Settings) -> bool {
    settings.ui_flags.get("urlScheme") == Some(&true)
}
// The port a stored value really means: clamped, with the default for none.
fn port_from(value: Option<f64>) -> u16 {
    value
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(1024.0, 65535.0) as u16)
        .unwrap_or(DEFAULT_PORT)
}
fn remote_port(settings: &Settings) -> u16 {
    port_from(settings.ui_values.get("remotePort").copied())
}
// 0 means no throttle at all (the slider's rightmost stop); every other
// stored value still clamps into the normal 1..=100000 range.
fn remote_rate_limit(settings: &Settings) -> u32 {
    settings
        .ui_values
        .get("remoteRateLimit")
        .copied()
        .filter(|v| v.is_finite())
        .map(|v| if v <= 0.0 { 0.0 } else { v.clamp(1.0, 100_000.0) })
        .unwrap_or(DEFAULT_RATE_LIMIT) as u32
}
fn split_ranges(s: &str) -> Vec<String> {
    s.split(|c: char| c == ',' || c.is_whitespace()).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
}
fn allowed_ranges(settings: &Settings) -> Vec<String> {
    settings.ui_texts.get("remoteAllowedRanges").map(|s| split_ranges(s)).unwrap_or_default()
}
fn password_hash_hex(settings: &Settings) -> Option<String> {
    settings.ui_texts.get("remotePasswordHash").filter(|s| !s.is_empty()).cloned()
}

fn log_remote(settings: &Settings, origin: &str, text: &str) {
    log_remote_if(error_log_enabled(settings), origin, text);
}

fn log_remote_if(enabled: bool, origin: &str, text: &str) {
    if enabled {
        record_error_log(text, origin, None, ERROR_LOG_MAX);
    }
}

// The few settings every connection looks at, copied out under the store lock so a
// polling client never makes the receiver clone the whole library's settings.
pub(crate) struct ReceiverAccess {
    network_filter: bool,
    ranges: Vec<String>,
    rate_limit: u32,
    log: bool,
    password_hash: Option<String>,
}

impl ReceiverAccess {
    pub(crate) fn from_settings(settings: &Settings) -> Self {
        Self {
            network_filter: network_filter_enabled(settings),
            ranges: allowed_ranges(settings),
            rate_limit: remote_rate_limit(settings),
            log: error_log_enabled(settings),
            password_hash: password_hash_hex(settings),
        }
    }

    fn read(app: &AppHandle) -> Self {
        let state: State<Db> = app.state();
        let guard = lock(&state);
        Self::from_settings(&guard.settings)
    }
}

// The same, for a spot that only has the app: the switch is read from the live settings.
fn log_remote_app(app: &AppHandle, origin: &str, text: &str) {
    let settings = {
        let db: State<Db> = app.state();
        let guard = lock(&db);
        guard.settings.clone()
    };
    log_remote(&settings, origin, text);
}

// ---------- Password: generate, hash, compare ----------

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn from_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

// 24 random bytes: long enough that guessing is not the weak point, short enough to type by
// hand if the copy fails.
pub(crate) fn generate_password() -> String {
    let mut buf = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut buf);
    to_hex(&buf)
}

// Only the hash is ever persisted; the plaintext exists only in memory, for
// as long as it takes the caller to show it once (see `stash_shown_password`).
pub(crate) fn hash_password(password: &str) -> String {
    to_hex(&Sha256::digest(password.as_bytes()))
}

// Both sides are hashed to a fixed 32 bytes first, so a length mismatch (a fast,
// non-constant-time check) can only come from a corrupt stored hash, never from attacker
// input.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn verify_password(candidate: &str, stored_hash_hex: &str) -> bool {
    let candidate_hash = Sha256::digest(candidate.as_bytes());
    match from_hex(stored_hash_hex) {
        Some(stored) => constant_time_eq(&candidate_hash, &stored),
        None => false,
    }
}

// A freshly generated password is shown exactly once through this one-shot slot; never
// logged, returned from a settings getter or put in a URL.
static LAST_SHOWN_PASSWORD: Mutex<Option<String>> = Mutex::new(None);

pub(crate) fn stash_shown_password(password: String) {
    *LAST_SHOWN_PASSWORD.lock().unwrap_or_else(|e| e.into_inner()) = Some(password);
}

#[tauri::command]
pub(crate) fn remote_api_take_shown_password() -> Option<String> {
    LAST_SHOWN_PASSWORD.lock().unwrap_or_else(|e| e.into_inner()).take()
}

// ---------- Durable remote-access changes ----------
//
// Every change to the receiver's settings goes through one of these cores, which take the
// save as a closure so failure paths are provable without an AppHandle. The rule is
// asymmetric on purpose: anything that opens the receiver up (switching it on, a new
// password, wider ranges) is rolled back and refused when the write does not land, so a
// restart cannot bring back something the user was not told about. Anything that closes it
// (switching it off) takes effect at once and only reports that the choice was not saved.

pub(crate) const ERR_SAVE_FAILED: &str = "save-failed";
pub(crate) const ERR_NETWORK_OFF_NOT_SAVED: &str = "remote-network-off-not-saved";
pub(crate) const REMOTE_PASSWORD_KEY: &str = "remotePasswordHash";

// Stored values no setter, reader or export may touch: the password hash and
// the receiver's certificate values. Each has its own command.
pub(crate) fn is_remote_secret_key(key: &str) -> bool {
    key == REMOTE_PASSWORD_KEY || crate::remote_tls::REMOTE_TLS_KEYS.contains(&key)
}
pub(crate) const ERR_OFF_NOT_SAVED: &str = "remote-off-not-saved";

fn restore_text(settings: &mut Settings, key: &str, old: Option<String>) {
    match old {
        Some(v) => {
            settings.ui_texts.insert(key.to_string(), v);
        }
        None => {
            settings.ui_texts.remove(key);
        }
    }
}

fn restore_flag(settings: &mut Settings, key: &str, old: Option<bool>) {
    match old {
        Some(v) => {
            settings.ui_flags.insert(key.to_string(), v);
        }
        None => {
            settings.ui_flags.remove(key);
        }
    }
}

// What the caller must do to the running receiver once a switch was applied.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum SwitchAction {
    None,
    Start,
    Stop,
}

pub(crate) struct SwitchResult {
    pub action: SwitchAction,
    // The first password ever minted, shown once by the caller; only ever set
    // after the write that stores its hash has landed.
    pub password: Option<String>,
    pub error: Option<String>,
}

// The `remoteApi` switch. On: the flag and (the first time) a password hash are saved
// together; a failed write restores both and starts nothing. Off: the flag goes off in memory
// and the receiver is told to stop whatever the write does; a failed write is reported as
// ERR_OFF_NOT_SAVED.
pub(crate) fn apply_remote_switch(settings: &mut Settings, enabled: bool, mut save: impl FnMut(&Settings) -> bool) -> SwitchResult {
    if !enabled {
        settings.ui_flags.insert("remoteApi".to_string(), false);
        let saved = save(settings);
        return SwitchResult {
            action: SwitchAction::Stop,
            password: None,
            error: if saved { None } else { Some(ERR_OFF_NOT_SAVED.to_string()) },
        };
    }
    let old_flag = settings.ui_flags.get("remoteApi").copied();
    let old_hash = settings.ui_texts.get("remotePasswordHash").cloned();
    settings.ui_flags.insert("remoteApi".to_string(), true);
    let mut minted = None;
    if password_hash_hex(settings).is_none() {
        let password = generate_password();
        settings.ui_texts.insert("remotePasswordHash".to_string(), hash_password(&password));
        minted = Some(password);
    }
    if !save(settings) {
        restore_flag(settings, "remoteApi", old_flag);
        restore_text(settings, "remotePasswordHash", old_hash);
        return SwitchResult { action: SwitchAction::None, password: None, error: Some(ERR_SAVE_FAILED.to_string()) };
    }
    SwitchResult { action: SwitchAction::Start, password: minted, error: None }
}

// A new password replaces the stored hash. If the write fails the old hash is restored and
// the caller must stop the receiver: the user was never shown the new password, and the
// disk still holds the old one.
pub(crate) fn regenerate_password_core(settings: &mut Settings, mut save: impl FnMut(&Settings) -> bool) -> Result<String, String> {
    if !remote_api_enabled(settings) {
        return Err("remote access is off".to_string());
    }
    let password = generate_password();
    let old_hash = settings.ui_texts.get("remotePasswordHash").cloned();
    settings.ui_texts.insert("remotePasswordHash".to_string(), hash_password(&password));
    if !save(settings) {
        restore_text(settings, "remotePasswordHash", old_hash);
        return Err(ERR_SAVE_FAILED.to_string());
    }
    Ok(password)
}

pub(crate) fn set_allowed_ranges_core(settings: &mut Settings, ranges: String, mut save: impl FnMut(&Settings) -> bool) -> Result<(), String> {
    validate_range_syntax(&ranges)?;
    let old = settings.ui_texts.get("remoteAllowedRanges").cloned();
    settings.ui_texts.insert("remoteAllowedRanges".to_string(), ranges);
    if !save(settings) {
        restore_text(settings, "remoteAllowedRanges", old);
        return Err(ERR_SAVE_FAILED.to_string());
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn remote_api_regenerate_password(app: AppHandle, state: State<Db>) -> Result<String, String> {
    let result = {
        let mut store = lock(&state);
        regenerate_password_core(&mut store.settings, |s| save_settings(&app, s))
    };
    if matches!(&result, Err(e) if e == ERR_SAVE_FAILED) {
        stop(&app);
    }
    result
}

#[tauri::command]
pub(crate) fn set_remote_allowed_ranges(app: AppHandle, state: State<Db>, ranges: String) -> Result<(), String> {
    let mut store = lock(&state);
    set_allowed_ranges_core(&mut store.settings, ranges, |s| save_settings(&app, s))
}

// The `remoteApi` branch of set_ui_flag: applies the switch, then does the one
// matching runtime transition. Returns the caller's error, if any.
pub(crate) fn set_remote_api_flag(app: &AppHandle, state: &State<Db>, enabled: bool) -> Result<(), String> {
    let result = {
        let mut store = lock(state);
        apply_remote_switch(&mut store.settings, enabled, |s| save_settings(app, s))
    };
    match result.action {
        SwitchAction::Start => {
            // start() returns before the bind, so only waiting for it tells whether
            // the port was really usable.
            if let Err(e) = start(app.clone()).and_then(|_| wait_bound(app)) {
                stop(app);
                // The receiver did not come up: the switch must not stay on, and a password nobody was shown
                // must not stay on record.
                let mut store = lock(state);
                store.settings.ui_flags.insert("remoteApi".to_string(), false);
                if result.password.is_some() {
                    store.settings.ui_texts.remove(REMOTE_PASSWORD_KEY);
                }
                let _ = save_settings(app, &store.settings);
                return Err(e);
            }
            if let Some(password) = result.password {
                stash_shown_password(password);
            }
        }
        SwitchAction::Stop => stop(app),
        SwitchAction::None => {}
    }
    match result.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

// The outcome of the `remoteNetworkFilter` switch: whether the running
// receiver has to be restarted, and the error to report, if any.
pub(crate) struct NetworkResult {
    pub restart: bool,
    pub error: Option<String>,
}

// The `remoteNetworkFilter` switch while the receiver may be running. Widening is
// asymmetric like the receiver's own switch: the identity is made first, then the flag is
// set and saved once, and any failure leaves memory and disk as they were and restarts
// nothing. Narrowing back to loopback takes effect whatever the write does (the receiver
// restarts bound to loopback) and a failed write is reported as not saved.
pub(crate) fn apply_network_switch(
    settings: &mut Settings,
    enabled: bool,
    names: &[String],
    mut save: impl FnMut(&Settings) -> bool,
) -> NetworkResult {
    let running = remote_api_enabled(settings);
    if !enabled {
        settings.ui_flags.insert("remoteNetworkFilter".to_string(), false);
        let saved = save(settings);
        return NetworkResult { restart: running, error: if saved { None } else { Some(ERR_NETWORK_OFF_NOT_SAVED.to_string()) } };
    }
    if running {
        if let Err(e) = crate::remote_tls::ensure_tls_identity(settings, names, &mut save) {
            return NetworkResult { restart: false, error: Some(e) };
        }
    }
    let old = settings.ui_flags.get("remoteNetworkFilter").copied();
    settings.ui_flags.insert("remoteNetworkFilter".to_string(), true);
    if !save(settings) {
        restore_flag(settings, "remoteNetworkFilter", old);
        return NetworkResult { restart: false, error: Some(ERR_SAVE_FAILED.to_string()) };
    }
    NetworkResult { restart: running, error: None }
}

// The stored value of `remoteNetworkFilter` once the restart for a switch failed.
pub(crate) fn flag_after_failed_restart(enabled: bool, old: Option<bool>) -> Option<bool> {
    if enabled {
        old
    } else {
        Some(false)
    }
}

// The `remoteNetworkFilter` branch of set_ui_flag.
pub(crate) fn set_network_flag(app: &AppHandle, state: &State<Db>, enabled: bool) -> Result<(), String> {
    let names = crate::remote_tls::host_names();
    let old = { lock(state).settings.ui_flags.get("remoteNetworkFilter").copied() };
    let result = {
        let mut store = lock(state);
        apply_network_switch(&mut store.settings, enabled, &names, |s| save_settings(app, s))
    };
    if result.restart {
        if let Err(e) = restart_receiver(app).and_then(|_| wait_bound(app)) {
            // A widening that could not start goes back to the old, narrower mode and runs again in it.
            // Narrowing never goes back: the flag stays off and the receiver stays down, so a failure
            // cannot open it to the network.
            {
                let mut store = lock(state);
                restore_flag(&mut store.settings, "remoteNetworkFilter", flag_after_failed_restart(enabled, old));
                let _ = save_settings(app, &store.settings);
            }
            if enabled {
                let _ = start(app.clone());
            } else {
                stop(app);
            }
            return Err(e);
        }
    }
    match result.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

// The port is fixed for one run, so a new value takes effect only through a restart: needed
// when the receiver is on and the value really moved.
pub(crate) fn needs_receiver_restart(key: &str, old: Option<f64>, new: f64, settings: &Settings) -> bool {
    key == "remotePort" && port_from(old) != port_from(Some(new)) && remote_api_enabled(settings)
}

pub(crate) const ERR_PORT_UNAVAILABLE: &str = "port-unavailable";

// Tries the address the receiver would bind, so a port something else holds is
// refused before anything is stored.
pub(crate) fn probe_port(ip: Ipv4Addr, port: u16) -> Result<(), String> {
    std::net::TcpListener::bind((ip, port)).map(drop).map_err(|e| {
        crate::append_crash_log(&format!("remote access: port {port} cannot be bound: {e}"));
        ERR_PORT_UNAVAILABLE.to_string()
    })
}

// Stores a new `remotePort`. When the running receiver would have to move, the new port is
// probed first and one that cannot be bound leaves the stored value untouched; a failed
// write restores the old value. Ok(true) means the receiver must be restarted.
pub(crate) fn commit_port_value(
    settings: &mut Settings,
    value: f64,
    probe: impl Fn(Ipv4Addr, u16) -> Result<(), String>,
    mut save: impl FnMut(&Settings) -> bool,
) -> Result<bool, String> {
    let old = settings.ui_values.get("remotePort").copied();
    let restart = needs_receiver_restart("remotePort", old, value, settings);
    if restart {
        let (ip, _) = bind_plan(settings);
        probe(ip, port_from(Some(value)))?;
    }
    settings.ui_values.insert("remotePort".to_string(), value);
    if !save(settings) {
        match old {
            Some(v) => settings.ui_values.insert("remotePort".to_string(), v),
            None => settings.ui_values.remove("remotePort"),
        };
        return Err(ERR_SAVE_FAILED.to_string());
    }
    Ok(restart)
}

// Waits until the run that start() queued has bound its port. start() returns
// before the bind, so only this tells whether the port was really usable.
fn wait_bound(app: &AppHandle) -> Result<(), String> {
    let state: State<RemoteApiState> = app.state();
    for _ in 0..120 {
        if state.port.load(Ordering::SeqCst) != 0 {
            return Ok(());
        }
        if !state.running.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err(ERR_PORT_UNAVAILABLE.to_string())
}

// The `remotePort` branch of set_ui_value, run after the new value was saved. A receiver
// that cannot come up on the new port goes back to the old value and run; the error reaches
// the UI.
pub(crate) fn apply_port_change(app: &AppHandle, state: &State<Db>, old: Option<f64>) -> Result<(), String> {
    if let Err(e) = restart_receiver(app).and_then(|_| wait_bound(app)) {
        crate::append_crash_log(&format!("remote access: the new port could not be used: {e}"));
        {
            let mut store = lock(state);
            match old {
                Some(v) => store.settings.ui_values.insert("remotePort".to_string(), v),
                None => store.settings.ui_values.remove("remotePort"),
            };
            let _ = save_settings(app, &store.settings);
        }
        let _ = start(app.clone());
        return Err(e);
    }
    Ok(())
}

// Stops the running receiver and starts it again from the stored settings. The
// new run waits for the old one to let go of its port.
pub(crate) fn restart_receiver(app: &AppHandle) -> Result<(), String> {
    stop(app);
    start(app.clone())
}

#[tauri::command]
pub(crate) fn remote_tls_fingerprint(state: State<Db>) -> Option<String> {
    let store = lock(&state);
    crate::remote_tls::stored_ca(&store.settings).and_then(|pem| crate::remote_tls::fingerprint(&pem))
}

// Writes the CA certificate (never the key) for the user to put on another
// device. Makes the identity first when none exists yet.
#[tauri::command]
pub(crate) async fn remote_tls_save_cert(app: AppHandle, state: State<'_, Db>) -> Result<String, String> {
    let names = crate::remote_tls::host_names();
    let (ca, restart) = {
        let mut store = lock(&state);
        let (id, source) = crate::remote_tls::ensure_tls_identity(&mut store.settings, &names, |s| save_settings(&app, s))?;
        let renewed = source != crate::remote_tls::IdentitySource::Stored;
        (id.ca_pem, renewed && remote_api_enabled(&store.settings) && network_filter_enabled(&store.settings))
    };
    // A running receiver still serves the certificate that was just replaced: restart
    // it so the exported CA is the one it offers.
    if restart {
        if let Err(e) = restart_receiver(&app) {
            log_remote_app(&app, "remoteApi", &format!("the receiver did not restart after the certificate was renewed: {e}"));
        }
        let _ = app.emit("remote-tls-renewed", ());
    }
    let file = crate::file_dialog(&app)
        .set_file_name(format!("{}-remote.pem", crate::brand::SLUG))
        .add_filter("PEM", &["pem"])
        .save_file();
    match file {
        Some(path) => {
            std::fs::write(&path, ca).map_err(|e| e.to_string())?;
            Ok(path.display().to_string())
        }
        None => Err("canceled".to_string()),
    }
}

// A fresh CA and server certificate. The receiver restarts at once when it is
// serving TLS, so the old certificate stops being offered; an unsaved renewal
// changes nothing.
#[tauri::command]
pub(crate) fn remote_tls_renew(app: AppHandle, state: State<Db>) -> Result<String, String> {
    let names = crate::remote_tls::host_names();
    let (fingerprint, restart) = {
        let mut store = lock(&state);
        let id = crate::remote_tls::renew_tls_identity(&mut store.settings, &names, |s| save_settings(&app, s))?;
        (crate::remote_tls::fingerprint(&id.ca_pem), remote_api_enabled(&store.settings) && network_filter_enabled(&store.settings))
    };
    if restart {
        restart_receiver(&app)?;
    }
    fingerprint.ok_or_else(|| "no-certificate".to_string())
}

// The expert switch for the certificate lifetime: the flag and a new certificate with that
// lifetime are saved in one write (see `set_short_lived_core`), then the receiver restarts
// as after a manual renewal. Only this command sets the flag; the generic setters, the
// command line and imports refuse it.
#[tauri::command]
pub(crate) fn remote_tls_set_short_lived(app: AppHandle, state: State<Db>, enabled: bool) -> Result<String, String> {
    let names = crate::remote_tls::host_names();
    let (fingerprint, restart) = {
        let mut store = lock(&state);
        // Nothing to switch: keep the certificate devices already trust.
        let current = store.settings.ui_flags.get(crate::remote_tls::SHORT_LIVED_KEY) == Some(&true);
        if current == enabled {
            if let Some(fp) = crate::remote_tls::stored_ca(&store.settings).and_then(|pem| crate::remote_tls::fingerprint(&pem)) {
                return Ok(fp);
            }
        }
        let id = crate::remote_tls::set_short_lived_core(&mut store.settings, enabled, &names, |s| save_settings(&app, s))?;
        (crate::remote_tls::fingerprint(&id.ca_pem), remote_api_enabled(&store.settings) && network_filter_enabled(&store.settings))
    };
    // From here the flag and the certificate are saved: a receiver that does not come
    // back up is reported under its own code, so the switch keeps the new state.
    if restart {
        restart_receiver(&app).map_err(|e| format!("{RESTART_FAILED_PREFIX}{e}"))?;
    }
    fingerprint.ok_or_else(|| "no-certificate".to_string())
}

pub(crate) const RESTART_FAILED_PREFIX: &str = "restart-failed:";

// ---------- IPv4 CIDR matching ----------

fn parse_ipv4_cidr(s: &str) -> Option<(u32, u32)> {
    let (addr_part, prefix_part) = match s.split_once('/') {
        Some(parts) => parts,
        None => (s, "32"),
    };
    let addr: Ipv4Addr = addr_part.trim().parse().ok()?;
    let prefix: u32 = prefix_part.trim().parse().ok()?;
    if prefix > 32 {
        return None;
    }
    Some((u32::from(addr), prefix))
}

fn ipv4_in_cidr(ip: Ipv4Addr, network: u32, prefix: u32) -> bool {
    let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix) };
    (u32::from(ip) & mask) == (network & mask)
}

// Loopback is always allowed (the receiver's default and its fallback when the network
// switch goes off). Anything else needs the switch on and a matching range. IPv6 ranges are
// not supported, so a non-loopback IPv6 peer never matches: fail closed rather than guess.
pub(crate) fn peer_allowed(peer: IpAddr, filter_on: bool, ranges: &[String]) -> bool {
    if peer.is_loopback() {
        return true;
    }
    if !filter_on {
        return false;
    }
    match peer {
        IpAddr::V4(v4) => ranges.iter().any(|r| parse_ipv4_cidr(r).is_some_and(|(net, prefix)| ipv4_in_cidr(v4, net, prefix))),
        IpAddr::V6(_) => false,
    }
}

pub(crate) fn validate_range_syntax(s: &str) -> Result<(), String> {
    for part in split_ranges(s) {
        if parse_ipv4_cidr(&part).is_none() {
            return Err(format!("not a valid address or range: {part}"));
        }
    }
    Ok(())
}

// ---------- Per-peer rate limit + failed-password lockout ----------

struct PeerState {
    window_start: Instant,
    count_in_window: u32,
    fail_count: u32,
    locked_until: Option<Instant>,
}
impl Default for PeerState {
    fn default() -> Self {
        PeerState { window_start: Instant::now(), count_in_window: 0, fail_count: 0, locked_until: None }
    }
}

// Past this many tracked peers, entries that are neither locked out nor inside their rate
// window are dropped. A peer is tracked before it authenticates, so without a bound a LAN
// peer cycling addresses could grow the table without limit.
const MAX_TRACKED_PEERS: usize = 1024;

fn prune_peers(peers: &mut HashMap<IpAddr, PeerState>, now: Instant) {
    if peers.len() <= MAX_TRACKED_PEERS {
        return;
    }
    let window = Duration::from_secs(RATE_WINDOW_SECS);
    peers.retain(|_, p| {
        p.locked_until.is_some_and(|until| now < until) || now.saturating_duration_since(p.window_start) < window
    });
    if peers.len() <= MAX_TRACKED_PEERS {
        return;
    }
    // Still over the bound: every survivor is inside its window. Evict the oldest
    // windows first, and peers currently locked out only when nothing else is left.
    let mut order: Vec<(bool, Instant, IpAddr)> = peers
        .iter()
        .map(|(ip, p)| (p.locked_until.is_some_and(|until| now < until), p.window_start, *ip))
        .collect();
    order.sort();
    let excess = peers.len() - MAX_TRACKED_PEERS;
    for (_, _, ip) in order.into_iter().take(excess) {
        peers.remove(&ip);
    }
}

pub(crate) struct RemoteGuard {
    peers: Mutex<HashMap<IpAddr, PeerState>>,
}
impl RemoteGuard {
    fn new() -> Self {
        RemoteGuard { peers: Mutex::new(HashMap::new()) }
    }

    // true while this peer is still serving out an earlier lockout.
    fn locked(&self, ip: IpAddr) -> bool {
        let mut peers = self.peers.lock().unwrap_or_else(|e| e.into_inner());
        prune_peers(&mut peers, Instant::now());
        let entry = peers.entry(ip).or_default();
        match entry.locked_until {
            Some(until) if Instant::now() < until => true,
            Some(_) => {
                entry.locked_until = None;
                entry.fail_count = 0;
                false
            }
            None => false,
        }
    }

    // true = still within this peer's per-minute allowance (and counted); false = over the
    // limit. A limit of 0 is the setting's unlimited stop: no throttle, and no window is tracked.
    fn check_rate(&self, ip: IpAddr, limit: u32) -> bool {
        if limit == 0 {
            return true;
        }
        let mut peers = self.peers.lock().unwrap_or_else(|e| e.into_inner());
        prune_peers(&mut peers, Instant::now());
        let entry = peers.entry(ip).or_default();
        if entry.window_start.elapsed() >= Duration::from_secs(RATE_WINDOW_SECS) {
            entry.window_start = Instant::now();
            entry.count_in_window = 0;
        }
        entry.count_in_window += 1;
        entry.count_in_window <= limit
    }

    // Returns true exactly when this failure triggered the lockout, so the caller can log the
    // distinction.
    fn record_auth_failure(&self, ip: IpAddr) -> bool {
        let mut peers = self.peers.lock().unwrap_or_else(|e| e.into_inner());
        let entry = peers.entry(ip).or_default();
        entry.fail_count += 1;
        if entry.fail_count >= FAIL_LOCKOUT_THRESHOLD {
            entry.locked_until = Some(Instant::now() + Duration::from_secs(FAIL_LOCKOUT_SECS));
            entry.fail_count = 0;
            true
        } else {
            false
        }
    }

    fn record_auth_success(&self, ip: IpAddr) {
        let mut peers = self.peers.lock().unwrap_or_else(|e| e.into_inner());
        let entry = peers.entry(ip).or_default();
        entry.fail_count = 0;
        entry.locked_until = None;
    }
}

// ---------- Managed state: the running receiver, if any ----------

// One receiver run: its own shutdown signal and the task that owns the listener. Nothing is
// shared between runs, so a stop cannot leave a pending wake-up for the next start.
#[derive(Default)]
struct RunSlot {
    cancel: Option<oneshot::Sender<()>>,
    join: Option<tauri::async_runtime::JoinHandle<()>>,
}

// Hands out runs one at a time. A new run cancels the previous one and gets its task handle
// so it can wait for the port to be free; the generation tells a late-finishing run it is no
// longer current.
pub(crate) struct RunController {
    generation: AtomicU64,
    slot: Mutex<RunSlot>,
}

struct RunTicket {
    generation: u64,
    cancel: oneshot::Receiver<()>,
    previous: Option<tauri::async_runtime::JoinHandle<()>>,
}

impl RunController {
    fn new() -> Self {
        RunController { generation: AtomicU64::new(0), slot: Mutex::new(RunSlot::default()) }
    }

    fn current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == generation
    }

    // Signals the running run, if any. Its task handle stays in the slot so
    // the next start can still wait for that run to let go of its port.
    fn stop(&self) {
        let mut slot = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(tx) = slot.cancel.take() {
            let _ = tx.send(());
        }
    }

    // Holds the slot while the caller spawns, so two starts can never
    // interleave. `spawn` gets the ticket and returns the new run's task.
    fn start_run(&self, spawn: impl FnOnce(RunTicket) -> tauri::async_runtime::JoinHandle<()>) {
        let mut slot = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(tx) = slot.cancel.take() {
            let _ = tx.send(());
        }
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = oneshot::channel();
        slot.cancel = Some(tx);
        let previous = slot.join.take();
        slot.join = Some(spawn(RunTicket { generation, cancel: rx, previous }));
    }
}

// The body of one run: wait for the previous run to be gone, bind, serve until this run's
// signal, release the port. Returns without binding if cancelled while waiting.
async fn serve_run<F>(
    ticket: RunTicket,
    addr: SocketAddr,
    controller: Arc<RunController>,
    on_bound: impl FnOnce(u16),
    on_bind_failed: impl FnOnce(std::io::Error, bool),
    on_connection: F,
    on_finished: impl FnOnce(bool),
) where
    F: Fn(TcpStream, SocketAddr),
{
    let RunTicket { generation, mut cancel, previous } = ticket;
    if let Some(mut prev) = previous {
        tokio::select! {
            _ = &mut prev => {}
            _ = &mut cancel => {
                // Cancelled while waiting: still wait for the older run, so a chain of starts and stops never
                // lets two tasks near the port.
                let _ = prev.await;
                on_finished(false);
                return;
            }
        }
    }
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            on_bind_failed(e, controller.current(generation));
            return;
        }
    };
    let bound_port = listener.local_addr().map(|a| a.port()).unwrap_or(addr.port());
    if !controller.current(generation) {
        // Stopped between the wait and the bind: never announce, never serve.
        drop(listener);
        on_finished(false);
        return;
    }
    on_bound(bound_port);
    loop {
        tokio::select! {
            _ = &mut cancel => break,
            accepted = listener.accept() => {
                if let Ok((stream, peer)) = accepted {
                    on_connection(stream, peer);
                }
            }
        }
    }
    drop(listener);
    on_finished(controller.current(generation));
}

pub(crate) struct RemoteApiState {
    running: AtomicBool,
    port: AtomicU16,
    guard: Arc<RemoteGuard>,
    runs: Arc<RunController>,
}
impl RemoteApiState {
    pub(crate) fn new() -> Self {
        RemoteApiState {
            running: AtomicBool::new(false),
            port: AtomicU16::new(0),
            guard: Arc::new(RemoteGuard::new()),
            runs: Arc::new(RunController::new()),
        }
    }
}

#[tauri::command]
pub(crate) fn remote_api_status(app: AppHandle) -> serde_json::Value {
    let state: State<RemoteApiState> = app.state();
    serde_json::json!({
        "running": state.running.load(Ordering::SeqCst),
        "port": state.port.load(Ordering::SeqCst),
    })
}

// The banner's off switch: the receiver stops whether or not "off" could be written; a
// failed write comes back as an error so the user knows a restart may bring it back.
#[tauri::command]
pub(crate) fn remote_api_stop_now(app: AppHandle, state: State<Db>) -> Result<(), String> {
    let result = {
        let mut store = lock(&state);
        apply_remote_switch(&mut store.settings, false, |s| save_settings(&app, s))
    };
    stop(&app);
    match result.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

// Starts the receiver if the setting is on and a password already exists, otherwise leaves it
// not running. The one place this rule is enforced, so every caller (boot, the expert-menu
// switch) gets it by calling start() instead of binding a socket itself.
pub(crate) fn start(app: AppHandle) -> Result<(), String> {
    let settings = {
        let db: State<Db> = app.state();
        let guard = lock(&db);
        guard.settings.clone()
    };
    if !remote_api_enabled(&settings) {
        return Ok(());
    }
    let Some(_hash) = password_hash_hex(&settings) else {
        log_remote(&settings, "remoteApi", "remote access could not start: no password is set");
        return Ok(());
    };
    let (bind_ip, tls) = bind_plan(&settings);
    // With the network switch on the receiver speaks only TLS. The identity is made (and stored)
    // before anything is bound; if that fails nothing starts.
    let acceptor = if tls {
        match prepare_acceptor(&app) {
            Ok(a) => Some(a),
            Err(e) => {
                log_remote(&settings, "remoteApi", &format!("remote access could not start: {e}"));
                return Err(e);
            }
        }
    } else {
        None
    };
    let state: State<RemoteApiState> = app.state();
    if state.running.swap(true, Ordering::SeqCst) {
        return Ok(()); // already running
    }
    let port = remote_port(&settings);
    // The bind address is fixed for a run; the peer allow-list is re-read on every connection
    // (see handle_connection).
    let addr = SocketAddr::from((bind_ip, port));
    let guard = state.guard.clone();
    let runs = state.runs.clone();
    let runs_for_run = runs.clone();
    runs.start_run(|ticket| {
        let generation = ticket.generation;
        let runs_conn = runs_for_run.clone();
        let (app_bound, app_failed, app_conn, app_done) = (app.clone(), app.clone(), app.clone(), app.clone());
        tauri::async_runtime::spawn(serve_run(
            ticket,
            addr,
            runs_for_run,
            move |bound_port| {
                let state: State<RemoteApiState> = app_bound.state();
                state.port.store(bound_port, Ordering::SeqCst);
                let _ = app_bound.emit("remote-api-status", serde_json::json!({"running": true, "port": bound_port}));
            },
            move |e, current| {
                let state: State<RemoteApiState> = app_failed.state();
                if current {
                    state.running.store(false, Ordering::SeqCst);
                    state.port.store(0, Ordering::SeqCst);
                    let _ = app_failed.emit("remote-api-status", serde_json::json!({"running": false, "port": 0}));
                }
                log_remote_app(&app_failed, "remoteApi", &format!("remote access could not bind its port: {e}"));
            },
            move |stream, peer| {
                let app3 = app_conn.clone();
                let guard2 = guard.clone();
                let acceptor2 = acceptor.clone();
                let runs2 = runs_conn.clone();
                tauri::async_runtime::spawn(async move { handle_connection(stream, peer, app3, guard2, acceptor2, runs2, generation).await });
            },
            move |current| {
                // A run that is no longer current must not touch the shared status: its stop() already
                // published "not running", and a newer run owns `running` and `port`.
                if current {
                    let state: State<RemoteApiState> = app_done.state();
                    state.running.store(false, Ordering::SeqCst);
                    state.port.store(0, Ordering::SeqCst);
                    let _ = app_done.emit("remote-api-status", serde_json::json!({"running": false, "port": 0}));
                }
            },
        ))
    });
    Ok(())
}

// The stored identity (made and saved first when missing or unreadable) turned into a
// handshake acceptor.
fn prepare_acceptor(app: &AppHandle) -> Result<TlsAcceptor, String> {
    let names = crate::remote_tls::host_names();
    let (id, source) = {
        let db: State<Db> = app.state();
        let mut store = lock(&db);
        crate::remote_tls::ensure_tls_identity(&mut store.settings, &names, |s| save_settings(app, s))?
    };
    if source == crate::remote_tls::IdentitySource::KeyUnreadable {
        log_remote_app(app, "remoteApi", "the receiver certificate key could not be read here, a new certificate was made");
    }
    if source == crate::remote_tls::IdentitySource::Expired {
        log_remote_app(app, "remoteApi", "the receiver certificate had expired, a new certificate was made");
        announce_tls_renewed(app);
    }
    Ok(TlsAcceptor::from(crate::remote_tls::build_server_config(&id)?))
}

// Tells the main window the certificate was replaced on its own, so it shows the same notice
// as after a manual renewal.
fn announce_tls_renewed(app: &AppHandle) {
    let _ = app.emit("remote-tls-renewed", ());
}

pub(crate) fn stop(app: &AppHandle) {
    let state: State<RemoteApiState> = app.state();
    state.runs.stop();
    state.running.store(false, Ordering::SeqCst);
    state.port.store(0, Ordering::SeqCst);
    let _ = app.emit("remote-api-status", serde_json::json!({"running": false, "port": 0}));
}

// ---------- The wire protocol ----------

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoteRequest {
    argv: Vec<String>,
}

struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

// Deliberately not a general HTTP/1.1 implementation: one request per connection,
// Content-Length only (no chunked transfer, no keep-alive). That is all a local companion
// client needs, with a much smaller surface than a real HTTP server.
async fn read_request<S: AsyncRead + Unpin>(stream: &mut S) -> Result<HttpRequest, u16> {
    let deadline = tokio::time::sleep(READ_TIMEOUT);
    tokio::pin!(deadline);
    let mut head = Vec::new();
    let mut chunk = [0u8; 512];
    loop {
        if let Some(pos) = find_subslice(&head, b"\r\n\r\n") {
            let rest = head.split_off(pos + 4);
            return finish_request(stream, head, rest, &mut deadline).await;
        }
        if head.len() > MAX_HEAD_BYTES {
            return Err(431);
        }
        let n = tokio::select! {
            r = stream.read(&mut chunk) => r.map_err(|_| 400u16)?,
            () = &mut deadline => return Err(408),
        };
        if n == 0 {
            return Err(400);
        }
        head.extend_from_slice(&chunk[..n]);
    }
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

async fn finish_request<S: AsyncRead + Unpin>(
    stream: &mut S,
    head: Vec<u8>,
    mut body: Vec<u8>,
    deadline: &mut (impl std::future::Future<Output = ()> + Unpin),
) -> Result<HttpRequest, u16> {
    let head_str = std::str::from_utf8(&head).map_err(|_| 400u16)?;
    let mut lines = head_str.split("\r\n");
    let request_line = lines.next().ok_or(400u16)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or(400u16)?.to_string();
    let path = parts.next().ok_or(400u16)?.to_string();
    let mut headers = HashMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line.split_once(':').ok_or(400u16)?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    let content_length: usize = match headers.get("content-length") {
        Some(v) => v.parse().map_err(|_| 400u16)?,
        None => 0,
    };
    if content_length > MAX_BODY_BYTES {
        return Err(413);
    }
    if body.len() > content_length {
        body.truncate(content_length); // no pipelining: anything past this request is never read
    }
    while body.len() < content_length {
        let mut buf = vec![0u8; (content_length - body.len()).min(65536)];
        let n = tokio::select! {
            r = stream.read(&mut buf) => r.map_err(|_| 400u16)?,
            () = &mut *deadline => return Err(408),
        };
        if n == 0 {
            return Err(400);
        }
        body.extend_from_slice(&buf[..n]);
    }
    Ok(HttpRequest { method, path, headers, body })
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

// No Access-Control-* header is ever sent, so a browser page that ignores the Origin/Referer
// refusal below still cannot read the response.
async fn write_response<S: AsyncWrite + Unpin>(stream: &mut S, status: u16, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status_text(status),
        body.len()
    );
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(body).await;
    let _ = stream.shutdown().await;
}

fn json_err(msg: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"error": msg})).unwrap_or_default()
}

// The request-shape checks that need no app state or password: method, path, and the
// cross-site headers a browser adds on its own and a genuine caller never sends. Pulled out
// of handle_connection so each has a direct test.
enum ShapeRejection {
    Method,
    Path,
    CrossSite,
}

fn reject_request_shape(req: &HttpRequest) -> Option<ShapeRejection> {
    if req.method != "POST" {
        return Some(ShapeRejection::Method);
    }
    if req.path != "/api" {
        return Some(ShapeRejection::Path);
    }
    // The other half of refusing a web page (no CORS header is sent, see write_response): a
    // browser attaches Origin or Referer itself while a same-machine or same-LAN caller never
    // does, so either header is refused before the password is checked.
    if req.headers.contains_key("origin") || req.headers.contains_key("referer") {
        return Some(ShapeRejection::CrossSite);
    }
    None
}

// The request-shape and password checks that need no app state: a verdict as the HTTP status
// to answer with. A missing stored hash is a 401, like a wrong password.
fn authorize(req: &HttpRequest, stored_hash: Option<&str>) -> Result<(), u16> {
    if let Some(rejection) = reject_request_shape(req) {
        return Err(match rejection {
            ShapeRejection::Method => 405,
            ShapeRejection::Path => 404,
            ShapeRejection::CrossSite => 403,
        });
    }
    let Some(stored) = stored_hash else {
        return Err(401);
    };
    let supplied = req.headers.get("x-promptsaver-password").map(String::as_str).unwrap_or("");
    if supplied.is_empty() || !verify_password(supplied, stored) {
        return Err(401);
    }
    Ok(())
}

// A refused peer is answered in plain HTTP. In TLS mode that would be garbage to a TLS
// client (and TLS mode never sends plaintext), so the connection is just closed.
async fn refuse_before_request<S: AsyncWrite + Unpin>(stream: &mut S, tls: bool, status: u16, message: &str) {
    if !tls {
        write_response(stream, status, &json_err(message)).await;
    }
}

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

// The TLS handshake, bounded: a plaintext client, a client that does not trust us, or one
// that stalls all end as Err before any request byte is read.
async fn tls_accept_within(
    acceptor: &TlsAcceptor,
    stream: TcpStream,
    limit: Duration,
) -> Result<tokio_rustls::server::TlsStream<TcpStream>, ()> {
    match tokio::time::timeout(limit, acceptor.accept(stream)).await {
        Ok(Ok(tls)) => Ok(tls),
        _ => Err(()),
    }
}

async fn tls_accept(acceptor: &TlsAcceptor, stream: TcpStream) -> Result<tokio_rustls::server::TlsStream<TcpStream>, ()> {
    tls_accept_within(acceptor, stream, HANDSHAKE_TIMEOUT).await
}

// Switch off: loopback, plain HTTP (traffic never leaves the machine). Switch on: every
// interface, TLS only.
fn bind_plan(settings: &Settings) -> (Ipv4Addr, bool) {
    if network_filter_enabled(settings) {
        (Ipv4Addr::UNSPECIFIED, true)
    } else {
        (Ipv4Addr::LOCALHOST, false)
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    peer: SocketAddr,
    app: AppHandle,
    guard: Arc<RemoteGuard>,
    acceptor: Option<TlsAcceptor>,
    runs: Arc<RunController>,
    generation: u64,
) {
    let ip = peer.ip();
    let access = ReceiverAccess::read(&app);
    let tls = acceptor.is_some();
    // Checked before any request byte is read: an address outside loopback (or outside the
    // configured ranges with the network switch on) never gets that far.
    if !peer_allowed(ip, access.network_filter, &access.ranges) {
        log_remote_if(access.log, "remoteApi", "remote access: refused a connection from a disallowed address");
        return;
    }
    if guard.locked(ip) {
        log_remote_if(access.log, "remoteApi", "remote access: refused a locked-out address");
        refuse_before_request(&mut stream, tls, 403, "locked out").await;
        return;
    }
    if !guard.check_rate(ip, access.rate_limit) {
        log_remote_if(access.log, "remoteApi", "remote access: rate limit exceeded");
        refuse_before_request(&mut stream, tls, 429, "rate limit exceeded").await;
        return;
    }
    match acceptor {
        Some(acceptor) => match tls_accept(&acceptor, stream).await {
            Ok(tls_stream) => serve_request(tls_stream, ip, access, app, guard, runs, generation).await,
            Err(()) => log_remote_if(access.log, "remoteApi", "remote access: a connection failed the secure handshake"),
        },
        None => serve_request(stream, ip, access, app, guard, runs, generation).await,
    }
}

// What the live access fields say about this peer right before a request is authorized:
// None when its address is no longer allowed, otherwise the stored password hash as it is
// now (itself None when no password is configured).
pub(crate) fn live_access(access: &ReceiverAccess, ip: IpAddr) -> Option<Option<String>> {
    if !peer_allowed(ip, access.network_filter, &access.ranges) {
        return None;
    }
    Some(access.password_hash.clone())
}

async fn serve_request<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    ip: IpAddr,
    access: ReceiverAccess,
    app: AppHandle,
    guard: Arc<RemoteGuard>,
    runs: Arc<RunController>,
    generation: u64,
) {
    let req = match read_request(&mut stream).await {
        Ok(r) => r,
        Err(status) => {
            log_remote_if(access.log, "remoteApi", "remote access: malformed or oversized request");
            write_response(&mut stream, status, &json_err("bad request")).await;
            return;
        }
    };
    // The settings were read before the handshake and request read, up to twenty seconds ago. A
    // regenerated password or narrowed range (neither restarts the receiver) must apply to this
    // request, so both are checked again against the live settings before authorizing.
    let access = ReceiverAccess::read(&app);
    let Some(stored_hash) = live_access(&access, ip) else {
        log_remote_if(access.log, "remoteApi", "remote access: refused a connection from a disallowed address");
        return;
    };
    if let Err(status) = authorize(&req, stored_hash.as_deref()) {
        let (log_msg, body_msg): (&str, &str) = match status {
            405 => ("remote access: method not allowed", "method not allowed"),
            404 => ("remote access: unknown path", "not found"),
            403 => ("remote access: refused a browser-originated request", "cross-site requests are refused"),
            _ if stored_hash.is_none() => ("remote access: no password is configured", "unauthorized"),
            _ => {
                let just_locked = guard.record_auth_failure(ip);
                (
                    if just_locked { "remote access: wrong password, address now locked out" } else { "remote access: wrong password" },
                    "unauthorized",
                )
            }
        };
        log_remote_if(access.log, "remoteApi", log_msg);
        write_response(&mut stream, status, &json_err(body_msg)).await;
        return;
    }
    guard.record_auth_success(ip);
    let argv = match serde_json::from_slice::<RemoteRequest>(&req.body) {
        Ok(r) if !r.argv.is_empty() && r.argv.len() <= MAX_ARGV_LEN => r.argv,
        _ => {
            log_remote_if(access.log, "remoteApi", "remote access: malformed command body");
            write_response(&mut stream, 400, &json_err("malformed command body")).await;
            return;
        }
    };
    let cmd_name = argv[0].clone();
    // A connection accepted before a stop, restart or renewed certificate belongs to a run that
    // no longer exists and must not execute anything.
    if !runs.current(generation) {
        log_remote_if(access.log, "remoteApi", "remote access: a request from a stopped receiver was not run");
        write_response(&mut stream, 503, &json_err("receiver restarted")).await;
        return;
    }
    let dir = data_dir(&app);
    if let Err(msg) = crate::cli::confine_path_arg(&argv, &dir) {
        log_remote_if(access.log, "remoteApi", &format!("remote access: {cmd_name} -> refused (path escapes the data folder)"));
        let body = serde_json::to_vec(&serde_json::json!({"code": 4, "stdout": "", "stderr": msg})).unwrap_or_default();
        write_response(&mut stream, 200, &body).await;
        return;
    }
    let (code, stdout, stderr, restart) = crate::cli::run_in_process_deferred(&app, argv, dir).await;
    log_remote_if(access.log, "remoteApi", &format!("remote access: {cmd_name} -> exit {code}"));
    let body = serde_json::to_vec(&serde_json::json!({"code": code, "stdout": stdout, "stderr": stderr})).unwrap_or_default();
    // A restore relaunches the app: the answer goes out first, then the restart.
    crate::cli::answer_then_restart(write_response(&mut stream, 200, &body), restart, || app.restart()).await;
}

// ---------- promptsaver:// links ----------

enum SchemeAction {
    Open(String),
    Copy(String),
}

// `promptsaver://open/<id>` or `promptsaver://copy/<id>` only: no other verb, query string or
// extra path segment, and the id must pass the pattern the CLI and receiver enforce.
fn parse_scheme_url(raw: &str) -> Option<SchemeAction> {
    let rest = raw.strip_prefix("promptsaver://")?;
    let (kind, id) = rest.split_once('/')?;
    if id.is_empty() || id.contains('/') || id.contains('?') || id.contains('#') {
        return None;
    }
    if !crate::cli::valid_id(id) {
        return None;
    }
    match kind {
        "open" => Some(SchemeAction::Open(id.to_string())),
        "copy" => Some(SchemeAction::Copy(id.to_string())),
        _ => None,
    }
}

// A registered protocol's target arrives as its own whole argv entry (Windows does not glue
// it to anything), so this only finds one and never assembles it.
pub(crate) fn find_scheme_arg(args: &[String]) -> Option<&str> {
    args.iter().map(String::as_str).find(|a| a.starts_with("promptsaver://"))
}

pub(crate) fn is_copy_link(url: &str) -> bool {
    matches!(parse_scheme_url(url), Some(SchemeAction::Copy(_)))
}

// A promptsaver:// link that started this process has nowhere to go until the main window's
// script has its listeners in place: emitting from .setup() lands before ui/main.js calls
// listen("edit-prompt", ...) and the event is lost. The link waits here, and the script asks
// for it once ready.
pub(crate) struct PendingLaunchUrl(pub Mutex<Option<String>>);

#[tauri::command]
pub(crate) fn run_pending_launch_url(app: AppHandle, pending: State<'_, PendingLaunchUrl>) {
    let url = pending.0.lock().ok().and_then(|mut g| g.take());
    if let Some(url) = url {
        handle_launch_url(&app, &url);
    }
}

pub(crate) fn handle_launch_url(app: &AppHandle, url: &str) {
    let settings = {
        let state: State<Db> = app.state();
        let guard = lock(&state);
        guard.settings.clone()
    };
    let Some(action) = parse_scheme_url(url) else {
        log_remote(&settings, "urlScheme", "a promptsaver:// link had an unrecognized shape");
        return;
    };
    if !url_scheme_enabled(&settings) {
        return; // off by default -- refused quietly, never a toast/dialog either
    }
    match action {
        SchemeAction::Open(id) => handle_open(app, &settings, id),
        SchemeAction::Copy(id) => handle_copy(app, &settings, id),
    }
}

// A missing flag means "never asked yet": ask once (a native dialog, so it works before any
// window shows), remember the answer either way, and never ask again for that kind of link.
fn consent(app: &AppHandle, settings: &Settings, flag_key: &str, kind: &str) -> bool {
    if let Some(v) = settings.ui_flags.get(flag_key) {
        return *v;
    }
    let lang = crate::resolve_lang(&settings.language);
    let allowed = confirm_native(lang, kind);
    let state: State<Db> = app.state();
    let mut store = lock(&state);
    store.settings.ui_flags.insert(flag_key.to_string(), allowed);
    save_settings(app, &store.settings);
    allowed
}

fn handle_open(app: &AppHandle, settings: &Settings, id: String) {
    let exists = {
        let state: State<Db> = app.state();
        let guard = lock(&state);
        guard.prompts.iter().any(|p| p.id == id)
    };
    if !exists {
        // Fails quietly: no dialog, no window, and no new entry for an id that does not exist.
        log_remote(settings, "urlScheme", "a promptsaver://open link named an id that does not exist");
        return;
    }
    if !consent(app, settings, "urlSchemeOpenAllowed", "open") {
        return;
    }
    // Reuses the path a normal in-app "edit" click takes (see edit_prompt_request), so a locked
    // entry's unlock screen is reached the same way and this link cannot bypass it.
    show_main(app);
    let _ = app.emit("edit-prompt", id);
}

fn handle_copy(app: &AppHandle, settings: &Settings, id: String) {
    let exists = {
        let state: State<Db> = app.state();
        let guard = lock(&state);
        guard.prompts.iter().any(|p| p.id == id)
    };
    if !exists {
        // Same as open: no consent question for an id that does not exist.
        log_remote(settings, "urlScheme", "a promptsaver://copy link named an id that does not exist");
        return;
    }
    if !consent(app, settings, "urlSchemeCopyAllowed", "copy") {
        return;
    }
    let app2 = app.clone();
    let dir = data_dir(app);
    let settings2 = settings.clone();
    tauri::async_runtime::spawn(async move {
        let (code, _stdout, _stderr) = crate::cli::run_in_process(&app2, vec!["copy".to_string(), id], dir).await;
        log_remote(&settings2, "urlScheme", &format!("promptsaver://copy -> exit {code}"));
    });
}

// A plain Yes/No native dialog (rfd is already a dependency); unlike a webview confirm() it
// works when no window exists yet.
fn confirm_native(lang: &str, kind: &str) -> bool {
    let (title, msg) = scheme_consent_texts(lang, kind);
    rfd::MessageDialog::new().set_level(rfd::MessageLevel::Info).set_title(&title).set_description(&msg).set_buttons(rfd::MessageButtons::YesNo).show()
        == rfd::MessageDialogResult::Yes
}

// Native-dialog strings live here, like tray_labels()/webview2_dialog(), not in ui/i18n.js,
// since this dialog can fire before any webview page has loaded.
fn scheme_consent_texts(lang: &str, kind: &str) -> (String, String) {
    let app = crate::brand::DISPLAY_NAME;
    let (title, open_msg, copy_msg): (&str, &str, &str) = match lang {
        "de" => (
            "Verknüpfung zulassen?",
            "Ein Link außerhalb von {app} möchte einen Eintrag im Fenster öffnen. Zulassen? Diese Frage erscheint nur beim ersten Mal.",
            "Ein Link außerhalb von {app} möchte einen Eintrag in die Zwischenablage kopieren. Zulassen? Diese Frage erscheint nur beim ersten Mal.",
        ),
        "es" => (
            "¿Permitir el enlace?",
            "Un enlace externo quiere abrir una entrada en la ventana de {app}. ¿Permitirlo? Esto solo se pregunta la primera vez.",
            "Un enlace externo quiere copiar una entrada al portapapeles de {app}. ¿Permitirlo? Esto solo se pregunta la primera vez.",
        ),
        "it" => (
            "Consentire il link?",
            "Un link esterno vuole aprire una voce nella finestra di {app}. Consentirlo? Viene chiesto solo la prima volta.",
            "Un link esterno vuole copiare una voce negli appunti da {app}. Consentirlo? Viene chiesto solo la prima volta.",
        ),
        "pt" => (
            "Permitir a ligação?",
            "Uma ligação externa quer abrir uma entrada na janela do {app}. Permitir? Isto só é perguntado da primeira vez.",
            "Uma ligação externa quer copiar uma entrada para a área de transferência do {app}. Permitir? Isto só é perguntado da primeira vez.",
        ),
        "pl" => (
            "Zezwolić na odnośnik?",
            "Zewnętrzny odnośnik chce otworzyć wpis w oknie {app}. Zezwolić? To pytanie pojawia się tylko raz.",
            "Zewnętrzny odnośnik chce skopiować wpis do schowka z {app}. Zezwolić? To pytanie pojawia się tylko raz.",
        ),
        "zh" => (
            "允许此链接吗？",
            "一个外部链接想要在 {app} 窗口中打开一个条目。是否允许？此询问仅出现一次。",
            "一个外部链接想要将一个条目复制到剪贴板（{app}）。是否允许？此询问仅出现一次。",
        ),
        "ja" => (
            "リンクを許可しますか?",
            "外部のリンクが {app} のウィンドウで項目を開こうとしています。許可しますか? この確認は最初の一回だけです。",
            "外部のリンクが {app} から項目をクリップボードにコピーしようとしています。許可しますか? この確認は最初の一回だけです。",
        ),
        "nl" => (
            "Koppeling toestaan?",
            "Een externe koppeling wil een item openen in het venster van {app}. Toestaan? Dit wordt maar één keer gevraagd.",
            "Een externe koppeling wil een item kopiëren naar het klembord vanuit {app}. Toestaan? Dit wordt maar één keer gevraagd.",
        ),
        "tr" => (
            "Bağlantıya izin verilsin mi?",
            "Harici bir bağlantı {app} penceresinde bir girdiyi açmak istiyor. İzin verilsin mi? Bu yalnızca ilk seferde sorulur.",
            "Harici bir bağlantı {app} panosuna bir girdiyi kopyalamak istiyor. İzin verilsin mi? Bu yalnızca ilk seferde sorulur.",
        ),
        "ro" => (
            "Permiți linkul?",
            "Un link extern dorește să deschidă o intrare în fereastra {app}. Permiți? Această întrebare apare o singură dată.",
            "Un link extern dorește să copieze o intrare în clipboard din {app}. Permiți? Această întrebare apare o singură dată.",
        ),
        _ => (
            "Allow this link?",
            "A link outside {app} wants to open an entry in the window. Allow it? This is asked only the first time.",
            "A link outside {app} wants to copy an entry to the clipboard from {app}. Allow it? This is asked only the first time.",
        ),
    };
    let msg = if kind == "open" { open_msg } else { copy_msg };
    (title.replace("{app}", app), msg.replace("{app}", app))
}

// ---------- promptsaver:// registry registration ----------
//
// HKCU\Software\Classes needs no elevation and takes precedence over a machine-wide entry
// for the signed-in user, so it covers the portable build (no installer) and the installed
// one alike.

#[cfg(windows)]
fn classes_key() -> Result<winreg::RegKey, String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_ALL_ACCESS};
    winreg::RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags("Software\\Classes", KEY_ALL_ACCESS).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn write_scheme(command: &str) -> Result<(), String> {
    let classes = classes_key()?;
    let (proto, _) = classes.create_subkey(SCHEME_NAME).map_err(|e| e.to_string())?;
    proto.set_value("", &format!("URL:{} Protocol", crate::brand::DISPLAY_NAME)).map_err(|e| e.to_string())?;
    proto.set_value("URL Protocol", &"").map_err(|e| e.to_string())?;
    let (open, _) = proto.create_subkey("shell\\open\\command").map_err(|e| e.to_string())?;
    open.set_value("", &command).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn delete_scheme() -> Result<(), String> {
    match classes_key()?.delete_subkey_all(SCHEME_NAME) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(windows)]
pub(crate) fn apply_url_scheme_registration(enabled: bool) -> Result<(), String> {
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        write_scheme(&format!("\"{}\" \"%1\"", exe.display()))
    } else {
        delete_scheme()
    }
}
#[cfg(not(windows))]
pub(crate) fn apply_url_scheme_registration(_enabled: bool) -> Result<(), String> {
    Err("the address scheme is only supported on Windows".to_string())
}

// The registered open command as it stands (None = not registered), so a failed change can
// put back exactly what was there.
#[cfg(windows)]
pub(crate) fn url_scheme_command() -> Result<Option<String>, String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    let path = format!("Software\\Classes\\{SCHEME_NAME}\\shell\\open\\command");
    match winreg::RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(path, KEY_READ) {
        Ok(key) => match key.get_value::<String, _>("") {
            Ok(v) => Ok(Some(v)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
#[cfg(not(windows))]
pub(crate) fn url_scheme_command() -> Result<Option<String>, String> {
    Ok(None)
}

#[cfg(windows)]
pub(crate) fn url_scheme_restore(prior: Option<String>) -> Result<(), String> {
    match prior {
        Some(cmd) => write_scheme(&cmd),
        None => delete_scheme(),
    }
}
#[cfg(not(windows))]
pub(crate) fn url_scheme_restore(_prior: Option<String>) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
