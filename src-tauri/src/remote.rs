// promptsaver:// links and the local remote-access receiver.
//
// Both give another program, or a clicked link, a way to reach this app
// without a real command line. Neither implements its own copy of the
// command set: a link's "copy" and every remote-receiver request run
// through cli::run_in_process, the exact function an already-running
// instance's own command-line pipe uses (see cli.rs). A store-side rule
// added there later -- a locked entry, a disabled command, a new validation
// -- protects the URL scheme and the receiver automatically, with nothing in
// this file aware of it. "open" is the one new action, because putting the
// real window in front of the user was never something the headless CLI
// needed.
//
// Both are off by default and stay off until a setting turns them on: the
// receiver additionally refuses to start at all without a password already
// set (see `start`), and a link's first "open" and first "copy" each ask for
// a one-time, native confirmation before ever acting.

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
// What a reset that turned the stored flags off still owes the outside world:
// the receiver keeps its socket until stopped and the address scheme stays in
// the registry until removed.
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
    if error_log_enabled(settings) {
        record_error_log(text, origin, None, ERROR_LOG_MAX);
    }
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

// 24 random bytes -- long enough that guessing it is not the weak point of
// anything else here, short enough to type by hand if the copy ever fails.
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

// Both sides are hashed to a fixed 32 bytes first specifically so a length
// mismatch (which WOULD be a fast, non-constant check) can never happen on
// real input -- only on a corrupt stored hash, never on anything an attacker
// controls.
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

// A freshly generated (or regenerated) password is shown to the user exactly
// once, through this one-shot slot -- never logged, never returned from a
// settings getter, never put in a URL.
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
// Every change to the receiver's own settings goes through one of these
// cores, which take the save as a closure so the failure paths can be proven
// without an AppHandle. The rule is asymmetric on purpose: anything that
// opens the receiver up (switching it on, a new password, wider ranges) is
// rolled back and refused when the write does not land, so a restart can never
// bring back something the user was not told about. Anything that closes it
// (switching it off) still takes effect at once and only reports that the
// choice was not saved.

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

// The `remoteApi` switch. On: flag plus (the very first time) a password hash
// are saved together; a failed write restores both and starts nothing. Off:
// the flag goes off in memory and the receiver is told to stop whatever the
// write does; a failed write is reported as ERR_OFF_NOT_SAVED.
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

// A new password replaces the stored hash. When the write fails the old hash
// is put back and the caller must stop the receiver: the user was never shown
// the new password, and the disk still holds the old one.
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
            if let Err(e) = start(app.clone()) {
                // The receiver did not come up: the switch must not stay on, and
                // a password nobody was shown must not stay on record.
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

// The `remoteNetworkFilter` switch while the receiver may be running. Widening
// it is asymmetric on purpose, like the receiver's own switch: the identity is
// made first, then the flag is set and saved once, and any failure leaves both
// memory and disk as they were and restarts nothing. Narrowing it back to
// loopback takes effect whatever the write does (the receiver is restarted bound
// to loopback) and a failed write is reported as not saved.
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
            // The new mode could not start: go back to the old one.
            {
                let mut store = lock(state);
                restore_flag(&mut store.settings, "remoteNetworkFilter", old);
                let _ = save_settings(app, &store.settings);
            }
            let _ = start(app.clone());
            return Err(e);
        }
    }
    match result.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

// The port is fixed for the life of one run, so a new value only takes effect
// through a restart: needed when the receiver is on and the value really moved.
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

// Stores a new `remotePort`. When the running receiver would have to move, the
// new port is probed first and a port that cannot be bound leaves the stored
// value untouched; a failed write puts the old value back. Ok(true) means the
// receiver has to be restarted.
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

// The `remotePort` branch of set_ui_value, run after the new value was saved.
// A receiver that cannot come up on the new port goes back to the old value
// and the old run; the error reaches the UI.
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
    let ca = {
        let mut store = lock(&state);
        let (id, _) = crate::remote_tls::ensure_tls_identity(&mut store.settings, &names, |s| save_settings(&app, s))?;
        id.ca_pem
    };
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

// Loopback is always allowed (that is the receiver's default, and its
// fallback the instant the network switch goes back off). Anything else
// needs the switch on AND a matching range -- IPv6 ranges are not supported
// by the filter, so an IPv6 peer that is not loopback never matches, fail
// closed rather than guess.
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

// Past this many tracked peers, entries that are neither locked out nor inside
// their rate window are dropped. A peer is tracked before it has authenticated, so
// without a bound a LAN peer cycling addresses could grow the table without limit.
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

    // true = still within the per-minute allowance for this peer (and it was
    // just counted); false = the request is over the limit. A limit of 0
    // means the setting's unlimited stop: no throttle at all, and the peer's
    // own window never even needs tracking.
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

    // Returns true exactly when this failure is the one that just triggered
    // the lockout (so the caller can log the distinction).
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

// One receiver run: its own shutdown signal and the task that owns the
// listener. Nothing is shared between runs, so a stop can never leave a
// pending wake-up behind for the next start.
#[derive(Default)]
struct RunSlot {
    cancel: Option<oneshot::Sender<()>>,
    join: Option<tauri::async_runtime::JoinHandle<()>>,
}

// Hands out runs one at a time. A new run cancels the previous one and gets
// its task handle, so it can wait for the port to be free; the generation
// only tells a late-finishing run that it is no longer the current one.
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

// The body of one run: wait for the previous run to be gone, bind, serve until
// this run's own signal, release the port. Returns without binding when the
// run was cancelled while it waited.
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
                // Cancelled while waiting: still wait for the older run, so a
                // chain of starts and stops never lets two tasks near the port.
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

// The banner's off switch: the receiver stops whether or not "off" could be
// written; a failed write comes back as an error so the user knows a restart
// may bring it back.
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

// Starts the receiver if the setting is on and a password already exists;
// otherwise leaves it not running. This is the ONE place that rule is
// enforced, so every caller (boot, the expert-menu switch, a future one)
// gets it for free just by calling start() instead of binding a socket
// itself.
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
    // With the network switch on the receiver speaks only TLS. The identity is
    // made (and stored) before anything is bound; when that fails nothing starts.
    let acceptor = if tls {
        match prepare_acceptor(&app) {
            Ok(a) => Some(a),
            Err(e) => {
                log_remote(&settings, "remoteApi", &format!("remote access could not start: {e}"));
                record_error_log(&format!("remote access could not start: {e}"), "remoteApi", None, 100);
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
    // The bind address is fixed for the lifetime of a run; the peer allow-list
    // itself is re-read fresh on every connection (see handle_connection).
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
                }
                record_error_log(&format!("remote access could not bind its port: {e}"), "remoteApi", None, 100);
            },
            move |stream, peer| {
                let app3 = app_conn.clone();
                let guard2 = guard.clone();
                let acceptor2 = acceptor.clone();
                let runs2 = runs_conn.clone();
                tauri::async_runtime::spawn(async move { handle_connection(stream, peer, app3, guard2, acceptor2, runs2, generation).await });
            },
            move |current| {
                // A run that is no longer the current one must not touch the
                // shared status: its stop() already published "not running",
                // and a newer run owns `running` and `port` now.
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

// The stored identity (made and saved first when it is missing or unreadable)
// turned into a handshake acceptor.
fn prepare_acceptor(app: &AppHandle) -> Result<TlsAcceptor, String> {
    let names = crate::remote_tls::host_names();
    let (id, source) = {
        let db: State<Db> = app.state();
        let mut store = lock(&db);
        crate::remote_tls::ensure_tls_identity(&mut store.settings, &names, |s| save_settings(app, s))?
    };
    if source == crate::remote_tls::IdentitySource::KeyUnreadable {
        record_error_log("the receiver certificate key could not be read here, a new certificate was made", "remoteApi", None, 100);
    }
    Ok(TlsAcceptor::from(crate::remote_tls::build_server_config(&id)?))
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

// Deliberately not a general HTTP/1.1 implementation: one request per
// connection, Content-Length only (no chunked transfer, no keep-alive). That
// is everything a local companion client needs, and it is a much smaller
// surface to get right than a real HTTP server would be.
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

// No Access-Control-* header is ever sent -- a browser page that ignores the
// Origin/Referer refusal below and tries anyway still cannot read the
// response, because nothing here ever opts it in.
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

// The request-shape checks that need no app state and no password: method,
// path, and the cross-site headers a browser adds on its own and a genuine
// caller never sends. Pulled out of handle_connection so each one has a
// direct test instead of only being reachable through a live connection.
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
    // No permissive CORS header is ever sent (see write_response), and this
    // is the other half of refusing a web page outright: a browser attaches
    // Origin or Referer on its own, a same-machine or same-LAN caller never
    // does, so either header's mere presence is refused before the password
    // is even checked.
    if req.headers.contains_key("origin") || req.headers.contains_key("referer") {
        return Some(ShapeRejection::CrossSite);
    }
    None
}

// The request-shape and password checks that need no app state: a verdict as
// the HTTP status to answer with. A missing stored hash is a 401 too, like a
// wrong password.
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

// A refused peer is answered in plain HTTP. In TLS mode that would be garbage
// to a TLS client (and a plaintext answer is exactly what TLS mode never
// sends), so the connection is just closed.
async fn refuse_before_request<S: AsyncWrite + Unpin>(stream: &mut S, tls: bool, status: u16, message: &str) {
    if !tls {
        write_response(stream, status, &json_err(message)).await;
    }
}

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

// The TLS handshake, bounded. A plaintext client, a client that does not trust
// us, or one that stalls all end up as Err before a single request byte is read.
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

// Switch off: loopback, plain HTTP (traffic never leaves the machine). Switch
// on: every interface, TLS only, for local and remote clients alike.
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
    let settings = {
        let state: State<Db> = app.state();
        let guard = lock(&state);
        guard.settings.clone()
    };
    let tls = acceptor.is_some();
    // Checked before a single byte of the actual request is read -- an
    // address outside the loopback default (or outside the configured
    // ranges, with the network switch on) never gets that far.
    if !peer_allowed(ip, network_filter_enabled(&settings), &allowed_ranges(&settings)) {
        log_remote(&settings, "remoteApi", "remote access: refused a connection from a disallowed address");
        return;
    }
    if guard.locked(ip) {
        log_remote(&settings, "remoteApi", "remote access: refused a locked-out address");
        refuse_before_request(&mut stream, tls, 403, "locked out").await;
        return;
    }
    if !guard.check_rate(ip, remote_rate_limit(&settings)) {
        log_remote(&settings, "remoteApi", "remote access: rate limit exceeded");
        refuse_before_request(&mut stream, tls, 429, "rate limit exceeded").await;
        return;
    }
    match acceptor {
        Some(acceptor) => match tls_accept(&acceptor, stream).await {
            Ok(tls_stream) => serve_request(tls_stream, ip, settings, app, guard, runs, generation).await,
            Err(()) => log_remote(&settings, "remoteApi", "remote access: a connection failed the secure handshake"),
        },
        None => serve_request(stream, ip, settings, app, guard, runs, generation).await,
    }
}

async fn serve_request<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    ip: IpAddr,
    settings: Settings,
    app: AppHandle,
    guard: Arc<RemoteGuard>,
    runs: Arc<RunController>,
    generation: u64,
) {
    let req = match read_request(&mut stream).await {
        Ok(r) => r,
        Err(status) => {
            log_remote(&settings, "remoteApi", "remote access: malformed or oversized request");
            write_response(&mut stream, status, &json_err("bad request")).await;
            return;
        }
    };
    let stored_hash = password_hash_hex(&settings);
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
        log_remote(&settings, "remoteApi", log_msg);
        write_response(&mut stream, status, &json_err(body_msg)).await;
        return;
    }
    guard.record_auth_success(ip);
    let argv = match serde_json::from_slice::<RemoteRequest>(&req.body) {
        Ok(r) if !r.argv.is_empty() && r.argv.len() <= MAX_ARGV_LEN => r.argv,
        _ => {
            log_remote(&settings, "remoteApi", "remote access: malformed command body");
            write_response(&mut stream, 400, &json_err("malformed command body")).await;
            return;
        }
    };
    let cmd_name = argv[0].clone();
    // A connection accepted before a stop, a restart or a renewed certificate
    // belongs to a run that no longer exists: it must not execute anything.
    if !runs.current(generation) {
        log_remote(&settings, "remoteApi", "remote access: a request from a stopped receiver was not run");
        write_response(&mut stream, 503, &json_err("receiver restarted")).await;
        return;
    }
    let dir = data_dir(&app);
    if let Err(msg) = crate::cli::confine_path_arg(&argv, &dir) {
        log_remote(&settings, "remoteApi", &format!("remote access: {cmd_name} -> refused (path escapes the data folder)"));
        let body = serde_json::to_vec(&serde_json::json!({"code": 4, "stdout": "", "stderr": msg})).unwrap_or_default();
        write_response(&mut stream, 200, &body).await;
        return;
    }
    let (code, stdout, stderr, restart) = crate::cli::run_in_process_deferred(&app, argv, dir).await;
    log_remote(&settings, "remoteApi", &format!("remote access: {cmd_name} -> exit {code}"));
    let body = serde_json::to_vec(&serde_json::json!({"code": code, "stdout": stdout, "stderr": stderr})).unwrap_or_default();
    // A restore relaunches the app: the answer goes out first, then the restart.
    crate::cli::answer_then_restart(write_response(&mut stream, 200, &body), restart, || app.restart()).await;
}

// ---------- promptsaver:// links ----------

enum SchemeAction {
    Open(String),
    Copy(String),
}

// `promptsaver://open/<id>` or `promptsaver://copy/<id>` only -- no other
// verb, no query string, no extra path segment. Anything else, including an
// id that fails the same pattern the CLI and the receiver already enforce,
// is not a link this app recognizes.
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

// A registered protocol's target always arrives as its own whole argv entry
// (Windows does not glue it to anything else), so this only ever has to find
// one, never assemble it from parts.
pub(crate) fn find_scheme_arg(args: &[String]) -> Option<&str> {
    args.iter().map(String::as_str).find(|a| a.starts_with("promptsaver://"))
}

pub(crate) fn is_copy_link(url: &str) -> bool {
    matches!(parse_scheme_url(url), Some(SchemeAction::Copy(_)))
}

// A promptsaver:// link that started this process itself (nobody was running
// yet) has nowhere to go until the main window's script has its listeners in
// place -- emitting straight into .setup() lands before ui/main.js calls
// listen("edit-prompt", ...) and the event is simply gone. The link waits
// here instead, and the script asks for it once it is ready.
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

// A flag that is missing means "never asked yet" -- ask once (a native
// dialog, so this works even before any window is showing), remember the
// answer either way, and never ask again for that kind of link.
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
        // Fails quietly: no dialog, no window, and above all no new entry --
        // a link naming an id that does not exist must never create one.
        log_remote(settings, "urlScheme", "a promptsaver://open link named an id that does not exist");
        return;
    }
    if !consent(app, settings, "urlSchemeOpenAllowed", "open") {
        return;
    }
    // Reuses the exact path a normal in-app "edit" click takes (see
    // edit_prompt_request) -- a locked entry's own unlock screen, once that
    // exists, is already reached this same way, so this link cannot bypass
    // it by construction.
    show_main(app);
    let _ = app.emit("edit-prompt", id);
}

fn handle_copy(app: &AppHandle, settings: &Settings, id: String) {
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

// A plain Yes/No native dialog -- rfd is already a dependency (file/folder
// pickers, the WebView2 offer) and this works even when no window exists
// yet, which a webview confirm() cannot.
fn confirm_native(lang: &str, kind: &str) -> bool {
    let (title, msg) = scheme_consent_texts(lang, kind);
    rfd::MessageDialog::new().set_level(rfd::MessageLevel::Info).set_title(&title).set_description(&msg).set_buttons(rfd::MessageButtons::YesNo).show()
        == rfd::MessageDialogResult::Yes
}

// Native-dialog strings live here, the same way tray_labels()/webview2_dialog()
// already do for other pre-webview text, rather than in ui/i18n.js -- this
// dialog can fire before any webview page has loaded.
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
// HKCU\Software\Classes needs no elevation and takes precedence over any
// machine-wide entry for the signed-in user, so this covers the portable
// build (no installer to do it at setup time) and the installed one alike.

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

// The registered open command as it stands now (None = not registered), so a
// failed change can put back exactly what was there.
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
mod tests {
    #[test]
    fn a_port_change_restarts_the_receiver_only_while_it_runs() {
        let mut on = Settings::default();
        on.ui_flags.insert("remoteApi".into(), true);
        let off = Settings::default();
        assert!(needs_receiver_restart("remotePort", Some(51823.0), 51900.0, &on));
        assert!(needs_receiver_restart("remotePort", None, 51900.0, &on));
        assert!(!needs_receiver_restart("remotePort", Some(51900.0), 51900.0, &on), "same value");
        assert!(!needs_receiver_restart("remotePort", Some(51823.0), 51900.0, &off), "receiver off");
        assert!(!needs_receiver_restart("remoteRateLimit", Some(1.0), 2.0, &on), "other key");
        assert!(!needs_receiver_restart("remotePort", None, 51823.0, &on), "default spelled out");
        assert!(!needs_receiver_restart("remotePort", Some(70000.0), 65535.0, &on), "clamped to the same port");
    }

    #[test]
    fn a_port_held_by_another_socket_is_refused_and_the_old_one_stays() {
        let held = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let busy = held.local_addr().unwrap().port();
        let mut s = Settings::default();
        s.ui_flags.insert("remoteApi".into(), true);
        s.ui_values.insert("remotePort".into(), 51900.0);
        let mut saved = 0;
        let r = commit_port_value(&mut s, busy as f64, probe_port, |_| {
            saved += 1;
            true
        });
        assert_eq!(r, Err(ERR_PORT_UNAVAILABLE.to_string()));
        assert_eq!(s.ui_values.get("remotePort"), Some(&51900.0));
        assert_eq!(saved, 0, "nothing written");
        drop(held);
        let free = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let p = free.local_addr().unwrap().port();
        drop(free);
        assert_eq!(commit_port_value(&mut s, p as f64, probe_port, |_| true), Ok(true));
        assert_eq!(s.ui_values.get("remotePort"), Some(&(p as f64)));
    }

    #[test]
    fn a_failed_write_puts_the_old_port_back() {
        let mut s = Settings::default();
        s.ui_flags.insert("remoteApi".into(), true);
        s.ui_values.insert("remotePort".into(), 51900.0);
        let r = commit_port_value(&mut s, 51901.0, |_, _| Ok(()), |_| false);
        assert_eq!(r, Err(ERR_SAVE_FAILED.to_string()));
        assert_eq!(s.ui_values.get("remotePort"), Some(&51900.0));
        let mut t = Settings::default();
        assert_eq!(commit_port_value(&mut t, 51901.0, |_, _| Err("x".into()), |_| false), Err(ERR_SAVE_FAILED.to_string()));
        assert!(!t.ui_values.contains_key("remotePort"));
    }

    #[test]
    fn a_reset_that_turns_the_receiver_and_the_scheme_off_owes_both_side_effects() {
        let mut old = Settings::default();
        old.ui_flags.insert("remoteApi".into(), true);
        old.ui_flags.insert("urlScheme".into(), true);
        let new = Settings::default();
        assert_eq!(reset_side_effects(&old, &new), ResetSideEffects { stop_receiver: true, unregister_scheme: true });
        assert_eq!(reset_side_effects(&new, &new), ResetSideEffects { stop_receiver: false, unregister_scheme: false });
        // A flag that stays on after the reset is not turned off by it.
        assert_eq!(reset_side_effects(&old, &old), ResetSideEffects { stop_receiver: false, unregister_scheme: false });
    }

    use super::*;
    use std::net::Ipv4Addr;

    // ---- password ----

    #[test]
    fn password_hash_round_trips_and_verifies() {
        let pw = generate_password();
        assert_eq!(pw.len(), 48); // 24 bytes, hex-encoded
        let hash = hash_password(&pw);
        assert!(verify_password(&pw, &hash));
    }

    #[test]
    fn wrong_password_is_rejected() {
        let hash = hash_password("correct horse battery staple");
        assert!(!verify_password("wrong password entirely", &hash));
        // Same length as the real one -- proves the check is not merely a
        // length shortcut.
        assert!(!verify_password("correct horse battery staplf", &hash));
    }

    #[test]
    fn nothing_verifies_against_an_empty_stored_hash() {
        // A receiver with no password configured has nothing to compare
        // against; the caller (handle_connection) refuses before ever
        // calling verify_password, but the function itself must also never
        // treat an empty stored value as a match for anything.
        assert!(!verify_password("anything", ""));
        assert!(!verify_password("", ""));
    }

    #[test]
    fn constant_time_eq_rejects_different_lengths_without_panic() {
        assert!(!constant_time_eq(b"short", b"a bit longer"));
    }

    // ---- CIDR / network filter ----

    #[test]
    fn loopback_is_always_allowed() {
        assert!(peer_allowed(IpAddr::V4(Ipv4Addr::LOCALHOST), false, &[]));
        assert!(peer_allowed(IpAddr::V4(Ipv4Addr::LOCALHOST), true, &[]));
        assert!(peer_allowed("::1".parse().unwrap(), false, &[]));
    }

    #[test]
    fn outside_address_is_refused_when_filter_is_off() {
        // TEST-NET-3 (RFC 5737) -- a real, non-loopback address standing in
        // for "somewhere outside this machine", never dialled for real.
        let outside = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5));
        assert!(!peer_allowed(outside, false, &["203.0.113.0/24".to_string()]));
    }

    #[test]
    fn outside_address_in_range_is_allowed_only_with_filter_on() {
        let peer = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42));
        let ranges = vec!["192.168.1.0/24".to_string()];
        assert!(!peer_allowed(peer, false, &ranges));
        assert!(peer_allowed(peer, true, &ranges));
    }

    #[test]
    fn address_outside_every_configured_range_is_refused() {
        let peer = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5));
        let ranges = vec!["192.168.1.0/24".to_string()];
        assert!(!peer_allowed(peer, true, &ranges));
    }

    #[test]
    fn ipv6_peer_never_matches_the_ipv4_only_filter() {
        let peer: IpAddr = "2001:db8::1".parse().unwrap();
        assert!(!peer_allowed(peer, true, &["0.0.0.0/0".to_string()]));
    }

    #[test]
    fn cidr_prefix_zero_matches_everything_v4() {
        let peer = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
        assert!(peer_allowed(peer, true, &["0.0.0.0/0".to_string()]));
    }

    #[test]
    fn range_syntax_validation_rejects_garbage() {
        assert!(validate_range_syntax("192.168.1.0/24, 10.0.0.0/8").is_ok());
        assert!(validate_range_syntax("not-an-address").is_err());
        assert!(validate_range_syntax("192.168.1.0/99").is_err());
        assert!(validate_range_syntax("300.1.1.1/24").is_err());
    }

    // ---- rate limit + lockout ----

    #[test]
    fn rate_limit_admits_up_to_the_cap_then_rejects() {
        let guard = RemoteGuard::new();
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        for _ in 0..5 {
            assert!(guard.check_rate(ip, 5));
        }
        assert!(!guard.check_rate(ip, 5));
    }

    // The slider's rightmost "unlimited" stop is stored as 0, and that must
    // skip the throttle entirely -- old code always applied a cap here.
    #[test]
    fn a_zero_limit_never_throttles() {
        let guard = RemoteGuard::new();
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        for _ in 0..1000 {
            assert!(guard.check_rate(ip, 0));
        }
    }

    #[test]
    fn a_real_limit_still_blocks_the_request_past_it() {
        let guard = RemoteGuard::new();
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        for _ in 0..60 {
            assert!(guard.check_rate(ip, 60));
        }
        assert!(!guard.check_rate(ip, 60), "the 61st request must be blocked");
    }

    #[test]
    fn ten_failures_lock_the_address_out() {
        let guard = RemoteGuard::new();
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        assert!(!guard.locked(ip));
        let mut triggered = false;
        for _ in 0..10 {
            triggered = guard.record_auth_failure(ip);
        }
        assert!(triggered, "the tenth failure must be the one that reports the lockout");
        assert!(guard.locked(ip));
    }

    #[test]
    fn a_success_clears_the_failure_count() {
        let guard = RemoteGuard::new();
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        for _ in 0..9 {
            guard.record_auth_failure(ip);
        }
        guard.record_auth_success(ip);
        // Nine more failures after a success must not reach the lockout --
        // the counter really was reset, not merely paused.
        let mut triggered = false;
        for _ in 0..9 {
            triggered = guard.record_auth_failure(ip);
        }
        assert!(!triggered);
        assert!(!guard.locked(ip));
    }

    #[test]
    fn concurrent_rate_checks_never_lose_a_count() {
        // Proves the shared counter is not racy under real concurrency: N
        // tasks each register exactly one request, and the guard's own
        // bookkeeping must show exactly N counted, no more, no fewer.
        let guard = Arc::new(RemoteGuard::new());
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let n = 200u32;
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(4).build().unwrap();
        let admitted = rt.block_on(async {
            let mut handles = Vec::new();
            for _ in 0..n {
                let g = guard.clone();
                handles.push(tokio::spawn(async move { g.check_rate(ip, n) }));
            }
            let mut admitted = 0u32;
            for h in handles {
                if h.await.unwrap() {
                    admitted += 1;
                }
            }
            admitted
        });
        assert_eq!(admitted, n, "every one of the N concurrent requests should have been counted exactly once");
    }

    // ---- promptsaver:// link parsing ----

    #[test]
    fn parses_open_and_copy_links() {
        assert!(matches!(parse_scheme_url("promptsaver://open/abc123"), Some(SchemeAction::Open(id)) if id == "abc123"));
        assert!(matches!(parse_scheme_url("promptsaver://copy/abc123"), Some(SchemeAction::Copy(id)) if id == "abc123"));
    }

    #[test]
    fn rejects_unknown_verb_and_write_style_links() {
        assert!(parse_scheme_url("promptsaver://delete/abc123").is_none());
        assert!(parse_scheme_url("promptsaver://edit/abc123").is_none());
        assert!(parse_scheme_url("promptsaver://open/").is_none());
        assert!(parse_scheme_url("promptsaver://open").is_none());
    }

    #[test]
    fn rejects_a_path_smuggled_into_the_id() {
        assert!(parse_scheme_url("promptsaver://open/../../etc/passwd").is_none());
        assert!(parse_scheme_url("promptsaver://open/abc/def").is_none());
        assert!(parse_scheme_url("promptsaver://open/abc?x=1").is_none());
    }

    #[test]
    fn rejects_ids_outside_the_shared_id_pattern() {
        assert!(parse_scheme_url("promptsaver://open/abc def").is_none());
        assert!(parse_scheme_url("promptsaver://open/abc;drop").is_none());
    }

    #[test]
    fn is_copy_link_only_true_for_copy() {
        assert!(is_copy_link("promptsaver://copy/abc123"));
        assert!(!is_copy_link("promptsaver://open/abc123"));
        assert!(!is_copy_link("not-a-link"));
    }

    #[test]
    fn find_scheme_arg_locates_the_url_among_other_args() {
        let args = vec!["--minimized".to_string(), "promptsaver://open/abc123".to_string()];
        assert_eq!(find_scheme_arg(&args), Some("promptsaver://open/abc123"));
        assert_eq!(find_scheme_arg(&["--minimized".to_string()]), None);
    }

    // ---- request-shape rejection (method, path, cross-site headers) ----

    fn shape_req(method: &str, path: &str, headers: &[(&str, &str)]) -> HttpRequest {
        HttpRequest {
            method: method.to_string(),
            path: path.to_string(),
            headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            body: Vec::new(),
        }
    }

    #[test]
    fn a_well_formed_request_passes_the_shape_check() {
        assert!(reject_request_shape(&shape_req("POST", "/api", &[])).is_none());
    }

    #[test]
    fn a_non_post_method_is_rejected() {
        assert!(matches!(reject_request_shape(&shape_req("GET", "/api", &[])), Some(ShapeRejection::Method)));
    }

    #[test]
    fn a_path_other_than_api_is_rejected() {
        assert!(matches!(reject_request_shape(&shape_req("POST", "/", &[])), Some(ShapeRejection::Path)));
    }

    #[test]
    fn a_request_carrying_an_origin_header_is_refused() {
        let req = shape_req("POST", "/api", &[("origin", "https://evil.example")]);
        assert!(matches!(reject_request_shape(&req), Some(ShapeRejection::CrossSite)));
    }

    #[test]
    fn a_request_carrying_a_referer_header_is_refused() {
        let req = shape_req("POST", "/api", &[("referer", "https://evil.example/page")]);
        assert!(matches!(reject_request_shape(&req), Some(ShapeRejection::CrossSite)));
    }

    // ---- refuses to start without a password ----

    #[test]
    fn no_stored_password_hash_means_password_hash_hex_returns_none() {
        // This is the exact condition start() checks before it ever binds a
        // socket -- proving it here proves the receiver cannot come up
        // without a password on record.
        let settings = Settings::default();
        assert!(password_hash_hex(&settings).is_none());
    }

    #[test]
    fn an_empty_stored_password_hash_also_means_no_password() {
        let mut settings = Settings::default();
        settings.ui_texts.insert("remotePasswordHash".to_string(), String::new());
        assert!(password_hash_hex(&settings).is_none());
    }

    #[test]
    fn a_real_stored_hash_is_found() {
        let mut settings = Settings::default();
        settings.ui_texts.insert("remotePasswordHash".to_string(), hash_password("correct horse battery staple"));
        assert!(password_hash_hex(&settings).is_some());
    }

    // ---- an id shaped like a SQL injection attempt never reaches the store ----

    #[test]
    fn a_sql_injection_shaped_id_fails_the_shared_id_pattern() {
        assert!(!crate::cli::valid_id("abc'; DROP TABLE prompts;--"));
        assert!(!crate::cli::valid_id("' OR '1'='1"));
        assert!(parse_scheme_url("promptsaver://open/' OR '1'='1").is_none());
        assert!(parse_scheme_url("promptsaver://copy/abc';DROP TABLE prompts;--").is_none());
    }

    // ---- HTTP framing ----

    #[tokio::test]
    async fn oversized_body_is_rejected_without_reading_it_all() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_request(&mut stream).await
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        let head = format!("POST /api HTTP/1.1\r\nContent-Length: {}\r\n\r\n", MAX_BODY_BYTES + 1);
        client.write_all(head.as_bytes()).await.unwrap();
        let result = server.await.unwrap();
        assert_eq!(result.err(), Some(413));
    }

    #[tokio::test]
    async fn well_formed_request_round_trips() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_request(&mut stream).await
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        let body = br#"{"argv":["list"]}"#;
        let head = format!(
            "POST /api HTTP/1.1\r\nContent-Length: {}\r\nX-Promptsaver-Password: x\r\n\r\n",
            body.len()
        );
        client.write_all(head.as_bytes()).await.unwrap();
        client.write_all(body).await.unwrap();
        let req = server.await.unwrap().unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/api");
        assert_eq!(req.headers.get("x-promptsaver-password").map(String::as_str), Some("x"));
        assert_eq!(req.body, body);
    }

    #[test]
    fn wire_body_rejects_unknown_fields() {
        let bad = br#"{"argv":["list"],"extra":true}"#;
        assert!(serde_json::from_slice::<RemoteRequest>(bad).is_err());
    }

    #[test]
    fn wire_body_requires_argv() {
        let bad = br#"{}"#;
        assert!(serde_json::from_slice::<RemoteRequest>(bad).is_err());
    }

    // ---- durable-or-reversible remote settings ----

    fn settings_with(on: bool, hash: Option<&str>) -> Settings {
        let mut s = Settings::default();
        if on {
            s.ui_flags.insert("remoteApi".into(), true);
        }
        if let Some(h) = hash {
            s.ui_texts.insert("remotePasswordHash".into(), h.into());
        }
        s
    }

    #[test]
    fn enabling_with_a_failed_write_restores_flag_and_first_password() {
        let mut s = settings_with(false, None);
        let mut calls = 0;
        let r = apply_remote_switch(&mut s, true, |_| {
            calls += 1;
            false
        });
        assert_eq!(calls, 1);
        assert_eq!(r.action, SwitchAction::None);
        assert!(r.password.is_none());
        assert_eq!(r.error.as_deref(), Some(ERR_SAVE_FAILED));
        assert!(!remote_api_enabled(&s));
        assert!(!s.ui_texts.contains_key("remotePasswordHash"));
    }

    #[test]
    fn enabling_with_a_failed_write_keeps_an_existing_hash() {
        let mut s = settings_with(false, Some("aa"));
        let r = apply_remote_switch(&mut s, true, |_| false);
        assert_eq!(r.action, SwitchAction::None);
        assert_eq!(s.ui_texts.get("remotePasswordHash").map(String::as_str), Some("aa"));
        assert!(!remote_api_enabled(&s));
    }

    #[test]
    fn enabling_with_a_good_write_starts_once_and_shows_the_new_password() {
        let mut s = settings_with(false, None);
        let mut calls = 0;
        let r = apply_remote_switch(&mut s, true, |_| {
            calls += 1;
            true
        });
        assert_eq!(calls, 1);
        assert_eq!(r.action, SwitchAction::Start);
        assert!(r.error.is_none());
        let pw = r.password.expect("first password");
        assert!(verify_password(&pw, &password_hash_hex(&s).unwrap()));
        // A second enable keeps the stored password and mints nothing.
        let r2 = apply_remote_switch(&mut s, true, |_| true);
        assert!(r2.password.is_none());
        assert!(verify_password(&pw, &password_hash_hex(&s).unwrap()));
    }

    #[test]
    fn disabling_stops_even_when_the_write_fails_and_says_so() {
        let mut s = settings_with(true, Some("aa"));
        let r = apply_remote_switch(&mut s, false, |_| false);
        assert_eq!(r.action, SwitchAction::Stop);
        assert_eq!(r.error.as_deref(), Some(ERR_OFF_NOT_SAVED));
        assert!(!remote_api_enabled(&s));
        let mut s2 = settings_with(true, Some("aa"));
        let ok = apply_remote_switch(&mut s2, false, |_| true);
        assert_eq!(ok.action, SwitchAction::Stop);
        assert!(ok.error.is_none());
    }

    #[test]
    fn regenerating_with_a_failed_write_restores_the_old_hash() {
        let mut s = settings_with(true, Some("old-hash"));
        let r = regenerate_password_core(&mut s, |_| false);
        assert_eq!(r.err().as_deref(), Some(ERR_SAVE_FAILED));
        assert_eq!(s.ui_texts.get("remotePasswordHash").map(String::as_str), Some("old-hash"));
    }

    #[test]
    fn regenerating_with_a_good_write_replaces_the_hash_and_returns_the_password() {
        let mut s = settings_with(true, Some("old-hash"));
        let pw = regenerate_password_core(&mut s, |_| true).unwrap();
        assert!(verify_password(&pw, &password_hash_hex(&s).unwrap()));
    }

    #[test]
    fn regenerating_while_off_is_refused_without_saving() {
        let mut s = settings_with(false, Some("old-hash"));
        let mut calls = 0;
        let r = regenerate_password_core(&mut s, |_| {
            calls += 1;
            true
        });
        assert!(r.is_err());
        assert_eq!(calls, 0);
    }

    #[test]
    fn a_failed_range_write_restores_the_old_ranges() {
        let mut s = settings_with(true, Some("aa"));
        s.ui_texts.insert("remoteAllowedRanges".into(), "10.0.0.0/8".into());
        let r = set_allowed_ranges_core(&mut s, "0.0.0.0/0".into(), |_| false);
        assert_eq!(r.err().as_deref(), Some(ERR_SAVE_FAILED));
        assert_eq!(s.ui_texts.get("remoteAllowedRanges").map(String::as_str), Some("10.0.0.0/8"));
        // Never set before: the failed write leaves it unset, not empty.
        let mut fresh = settings_with(true, Some("aa"));
        let _ = set_allowed_ranges_core(&mut fresh, "10.0.0.0/8".into(), |_| false);
        assert!(!fresh.ui_texts.contains_key("remoteAllowedRanges"));
    }

    #[test]
    fn a_good_range_write_lands_and_a_bad_range_never_reaches_the_save() {
        let mut s = settings_with(true, Some("aa"));
        assert!(set_allowed_ranges_core(&mut s, "192.168.1.0/24".into(), |_| true).is_ok());
        assert_eq!(s.ui_texts.get("remoteAllowedRanges").map(String::as_str), Some("192.168.1.0/24"));
        let mut calls = 0;
        let r = set_allowed_ranges_core(&mut s, "nonsense".into(), |_| {
            calls += 1;
            true
        });
        assert!(r.is_err());
        assert_eq!(calls, 0);
        assert_eq!(s.ui_texts.get("remoteAllowedRanges").map(String::as_str), Some("192.168.1.0/24"));
    }

    // ---- one shutdown per run ----

    #[derive(Default)]
    struct Probe {
        bound: std::sync::atomic::AtomicUsize,
        finished_current: std::sync::atomic::AtomicUsize,
        finished_stale: std::sync::atomic::AtomicUsize,
    }

    fn launch(controller: &Arc<RunController>, addr: SocketAddr, probe: &Arc<Probe>) {
        let c = controller.clone();
        let (p1, p2) = (probe.clone(), probe.clone());
        controller.start_run(|ticket| {
            tauri::async_runtime::spawn(serve_run(
                ticket,
                addr,
                c,
                move |_| {
                    p1.bound.fetch_add(1, Ordering::SeqCst);
                },
                |_, _| {},
                |_stream, _peer| {},
                move |current| {
                    let counter = if current { &p2.finished_current } else { &p2.finished_stale };
                    counter.fetch_add(1, Ordering::SeqCst);
                },
            ))
        });
    }

    async fn free_port() -> u16 {
        let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        l.local_addr().unwrap().port()
    }

    async fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
        for _ in 0..200 {
            if cond() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        cond()
    }

    #[tokio::test]
    async fn a_stop_while_idle_does_not_kill_the_next_start() {
        let controller = Arc::new(RunController::new());
        controller.stop();
        controller.stop();
        let probe = Arc::new(Probe::default());
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_port().await));
        launch(&controller, addr, &probe);
        assert!(wait_until(|| probe.bound.load(Ordering::SeqCst) == 1).await);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(probe.finished_current.load(Ordering::SeqCst) + probe.finished_stale.load(Ordering::SeqCst), 0);
        assert!(TcpStream::connect(addr).await.is_ok());
        controller.stop();
        assert!(wait_until(|| probe.finished_current.load(Ordering::SeqCst) == 0 && probe.finished_stale.load(Ordering::SeqCst) == 1).await);
    }

    #[tokio::test]
    async fn rapid_start_stop_start_leaves_one_live_listener_and_a_quiet_old_run() {
        let controller = Arc::new(RunController::new());
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_port().await));
        let first = Arc::new(Probe::default());
        let second = Arc::new(Probe::default());
        launch(&controller, addr, &first);
        controller.stop();
        launch(&controller, addr, &second);
        assert!(wait_until(|| second.bound.load(Ordering::SeqCst) == 1).await);
        // The old run never took the port after the stop, and never published.
        assert!(wait_until(|| first.finished_stale.load(Ordering::SeqCst) == 1).await);
        assert_eq!(first.finished_current.load(Ordering::SeqCst), 0);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(second.finished_current.load(Ordering::SeqCst) + second.finished_stale.load(Ordering::SeqCst), 0);
        assert!(TcpStream::connect(addr).await.is_ok());
        // One owner: a second bind on the same port is refused while it runs.
        assert!(TcpListener::bind(addr).await.is_err());
        // A restart waits for the running one to release the port, then owns it.
        let third = Arc::new(Probe::default());
        launch(&controller, addr, &third);
        assert!(wait_until(|| third.bound.load(Ordering::SeqCst) == 1).await);
        assert!(wait_until(|| second.finished_stale.load(Ordering::SeqCst) == 1).await);
        assert_eq!(second.finished_current.load(Ordering::SeqCst), 0);
        assert!(TcpStream::connect(addr).await.is_ok());
    }

    // ---- HTTPS transport ----

    use std::sync::atomic::AtomicUsize;
    use tokio_rustls::rustls::pki_types::{pem::PemObject, CertificateDer, ServerName};
    use tokio_rustls::rustls::{ClientConfig, RootCertStore};

    fn test_identity(settings: &mut Settings) -> crate::remote_tls::TlsIdentity {
        let names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
        crate::remote_tls::ensure_tls_identity(settings, &names, |_| true).unwrap().0
    }

    fn acceptor_for(id: &crate::remote_tls::TlsIdentity) -> TlsAcceptor {
        TlsAcceptor::from(crate::remote_tls::build_server_config(id).unwrap())
    }

    fn connector_trusting(ca_pem: &str) -> tokio_rustls::TlsConnector {
        let mut roots = RootCertStore::empty();
        for c in CertificateDer::pem_slice_iter(ca_pem.as_bytes()) {
            roots.add(c.unwrap()).unwrap();
        }
        let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
        let cfg = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        tokio_rustls::TlsConnector::from(Arc::new(cfg))
    }

    // A responder built from the same pieces the receiver uses: handshake,
    // read_request, authorize, write_response. `calls` counts requests that
    // were parsed, i.e. got past the handshake.
    async fn spawn_responder(acceptor: Option<TlsAcceptor>, stored: Option<String>, calls: Arc<AtomicUsize>) -> SocketAddr {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else { continue };
                let acceptor = acceptor.clone();
                let stored = stored.clone();
                let calls = calls.clone();
                tokio::spawn(async move {
                    async fn respond<S: AsyncRead + AsyncWrite + Unpin>(mut s: S, stored: Option<String>, calls: Arc<AtomicUsize>) {
                        let Ok(req) = read_request(&mut s).await else { return };
                        calls.fetch_add(1, Ordering::SeqCst);
                        match authorize(&req, stored.as_deref()) {
                            Ok(()) => write_response(&mut s, 200, b"{}").await,
                            Err(status) => write_response(&mut s, status, b"{}").await,
                        }
                    }
                    match acceptor {
                        Some(a) => {
                            if let Ok(tls) = tls_accept(&a, stream).await {
                                respond(tls, stored, calls).await;
                            }
                        }
                        None => respond(stream, stored, calls).await,
                    }
                });
            }
        });
        addr
    }

    fn post_with(password: &str) -> Vec<u8> {
        format!("POST /api HTTP/1.1\r\nHost: x\r\nX-Promptsaver-Password: {password}\r\nContent-Length: 2\r\n\r\n{{}}").into_bytes()
    }

    async fn https_status(connector: &tokio_rustls::TlsConnector, addr: SocketAddr, password: &str) -> Result<u16, String> {
        let tcp = TcpStream::connect(addr).await.map_err(|e| e.to_string())?;
        let mut tls = connector.connect(ServerName::try_from("localhost").unwrap(), tcp).await.map_err(|e| e.to_string())?;
        tls.write_all(&post_with(password)).await.map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        let _ = tls.read_to_end(&mut out).await;
        let text = String::from_utf8_lossy(&out).to_string();
        text.strip_prefix("HTTP/1.1 ").and_then(|r| r.get(..3)).and_then(|c| c.parse().ok()).ok_or_else(|| format!("no status in {text:?}"))
    }

    #[test]
    fn bind_plan_is_loopback_plain_or_everywhere_with_tls() {
        let mut s = Settings::default();
        assert_eq!(bind_plan(&s), (Ipv4Addr::LOCALHOST, false));
        s.ui_flags.insert("remoteNetworkFilter".into(), true);
        assert_eq!(bind_plan(&s), (Ipv4Addr::UNSPECIFIED, true));
    }

    #[test]
    fn authorize_checks_shape_then_password() {
        let hash = hash_password("secret");
        let req = |method: &str, path: &str, pw: Option<&str>, origin: bool| {
            let mut headers = HashMap::new();
            if let Some(p) = pw {
                headers.insert("x-promptsaver-password".to_string(), p.to_string());
            }
            if origin {
                headers.insert("origin".to_string(), "http://evil".to_string());
            }
            HttpRequest { method: method.into(), path: path.into(), headers, body: vec![] }
        };
        assert_eq!(authorize(&req("POST", "/api", Some("secret"), false), Some(&hash)), Ok(()));
        assert_eq!(authorize(&req("POST", "/api", Some("wrong"), false), Some(&hash)), Err(401));
        assert_eq!(authorize(&req("POST", "/api", None, false), Some(&hash)), Err(401));
        assert_eq!(authorize(&req("POST", "/api", Some("secret"), false), None), Err(401));
        assert_eq!(authorize(&req("GET", "/api", Some("secret"), false), Some(&hash)), Err(405));
        assert_eq!(authorize(&req("POST", "/x", Some("secret"), false), Some(&hash)), Err(404));
        assert_eq!(authorize(&req("POST", "/api", Some("secret"), true), Some(&hash)), Err(403));
    }

    #[tokio::test]
    async fn plaintext_to_a_tls_listener_gets_no_http_answer_and_reaches_no_request_code() {
        let mut settings = Settings::default();
        let id = test_identity(&mut settings);
        let calls = Arc::new(AtomicUsize::new(0));
        let addr = spawn_responder(Some(acceptor_for(&id)), Some(hash_password("pw")), calls.clone()).await;
        let mut plain = TcpStream::connect(addr).await.unwrap();
        plain.write_all(&post_with("pw")).await.unwrap();
        let mut out = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(5), plain.read_to_end(&mut out)).await;
        assert!(!out.starts_with(b"HTTP/"), "a plaintext client must never get an HTTP answer");
        assert!(!String::from_utf8_lossy(&out).contains("200"));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_client_trusting_the_saved_ca_gets_200_with_the_password_and_401_without() {
        let mut settings = Settings::default();
        let id = test_identity(&mut settings);
        let calls = Arc::new(AtomicUsize::new(0));
        let addr = spawn_responder(Some(acceptor_for(&id)), Some(hash_password("pw")), calls.clone()).await;
        let connector = connector_trusting(&id.ca_pem);
        assert_eq!(https_status(&connector, addr, "pw").await, Ok(200));
        assert_eq!(https_status(&connector, addr, "wrong").await, Ok(401));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_client_trusting_a_different_or_the_old_ca_fails_the_handshake() {
        let mut settings = Settings::default();
        let old = test_identity(&mut settings);
        let names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
        let new = crate::remote_tls::renew_tls_identity(&mut settings, &names, |_| true).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let addr = spawn_responder(Some(acceptor_for(&new)), Some(hash_password("pw")), calls.clone()).await;
        // The device that trusted the certificate from before the renewal.
        assert!(https_status(&connector_trusting(&old.ca_pem), addr, "pw").await.is_err());
        let mut other = Settings::default();
        let stranger = test_identity(&mut other);
        assert!(https_status(&connector_trusting(&stranger.ca_pem), addr, "pw").await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(https_status(&connector_trusting(&new.ca_pem), addr, "pw").await, Ok(200));
    }

    #[tokio::test]
    async fn a_stalled_handshake_is_dropped_within_the_limit() {
        let mut settings = Settings::default();
        let id = test_identity(&mut settings);
        let acceptor = acceptor_for(&id);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _idle = TcpStream::connect(addr).await.unwrap();
        let (server_side, _) = listener.accept().await.unwrap();
        let started = Instant::now();
        assert!(tls_accept_within(&acceptor, server_side, Duration::from_millis(200)).await.is_err());
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[tokio::test]
    async fn a_refused_peer_in_tls_mode_gets_zero_bytes_and_in_plain_mode_a_status() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        for (tls, expect_bytes) in [(true, false), (false, true)] {
            let client = tokio::spawn(async move {
                let mut c = TcpStream::connect(addr).await.unwrap();
                let mut out = Vec::new();
                let _ = tokio::time::timeout(Duration::from_secs(5), c.read_to_end(&mut out)).await;
                out
            });
            let (mut server_side, _) = listener.accept().await.unwrap();
            refuse_before_request(&mut server_side, tls, 403, "locked out").await;
            drop(server_side);
            let out = client.await.unwrap();
            assert_eq!(!out.is_empty(), expect_bytes, "tls={tls}");
            if expect_bytes {
                assert!(out.starts_with(b"HTTP/1.1 403"));
            }
        }
    }

    // ---- the network switch ----

    fn names_for_test() -> Vec<String> {
        vec!["localhost".to_string(), "127.0.0.1".to_string()]
    }

    #[test]
    fn turning_the_network_switch_on_makes_the_identity_then_saves_and_asks_for_a_restart() {
        let mut s = settings_with(true, Some("aa"));
        let r = apply_network_switch(&mut s, true, &names_for_test(), |_| true);
        assert!(r.error.is_none());
        assert!(r.restart);
        assert!(network_filter_enabled(&s));
        assert!(s.ui_texts.get(crate::remote_tls::REMOTE_TLS_KEY).unwrap().starts_with(crate::ENC_PREFIX));
    }

    #[test]
    fn a_failed_final_write_leaves_the_flag_off_in_memory_and_on_disk() {
        let mut s = settings_with(true, Some("aa"));
        // Every write the switch makes: what the disk would hold after each.
        let mut disk_flag_after_each_write = Vec::new();
        let r = apply_network_switch(&mut s, true, &names_for_test(), |st| {
            disk_flag_after_each_write.push(network_filter_enabled(st));
            // The identity write lands, the flag write fails.
            !network_filter_enabled(st)
        });
        assert_eq!(r.error.as_deref(), Some(ERR_SAVE_FAILED));
        assert!(!r.restart);
        assert!(!network_filter_enabled(&s));
        // The only write that succeeded never carried the flag.
        assert_eq!(disk_flag_after_each_write.first(), Some(&false));
    }

    #[test]
    fn a_failing_identity_leaves_the_flag_alone() {
        let mut s = settings_with(true, Some("aa"));
        let r = apply_network_switch(&mut s, true, &names_for_test(), |_| false);
        assert_eq!(r.error.as_deref(), Some(ERR_SAVE_FAILED));
        assert!(!network_filter_enabled(&s));
        assert!(!s.ui_texts.contains_key(crate::remote_tls::REMOTE_TLS_KEY));
    }

    #[test]
    fn narrowing_the_network_switch_always_applies_and_says_when_it_was_not_saved() {
        let mut s = settings_with(true, Some("aa"));
        s.ui_flags.insert("remoteNetworkFilter".into(), true);
        let r = apply_network_switch(&mut s, false, &names_for_test(), |_| false);
        assert!(r.restart, "the receiver must be restarted bound to loopback");
        assert_eq!(r.error.as_deref(), Some(ERR_NETWORK_OFF_NOT_SAVED));
        assert!(!network_filter_enabled(&s));
        let mut s2 = settings_with(true, Some("aa"));
        s2.ui_flags.insert("remoteNetworkFilter".into(), true);
        let ok = apply_network_switch(&mut s2, false, &names_for_test(), |_| true);
        assert!(ok.restart && ok.error.is_none());
    }

    #[test]
    fn the_network_switch_with_the_receiver_off_needs_no_identity_and_no_restart() {
        let mut s = settings_with(false, None);
        let r = apply_network_switch(&mut s, true, &names_for_test(), |_| true);
        assert!(!r.restart && r.error.is_none());
        assert!(!s.ui_texts.contains_key(crate::remote_tls::REMOTE_TLS_KEY));
    }

    #[test]
    fn secret_keys_are_the_password_hash_and_the_certificate_values() {
        assert!(is_remote_secret_key("remotePasswordHash"));
        for k in crate::remote_tls::REMOTE_TLS_KEYS {
            assert!(is_remote_secret_key(k));
        }
        assert!(!is_remote_secret_key("remoteAllowedRanges"));
        assert!(!is_remote_secret_key("remotePort"));
    }

    #[tokio::test]
    async fn a_chain_of_starts_and_stops_never_binds_twice() {
        let controller = Arc::new(RunController::new());
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_port().await));
        let probes: Vec<Arc<Probe>> = (0..4).map(|_| Arc::new(Probe::default())).collect();
        launch(&controller, addr, &probes[0]);
        controller.stop();
        launch(&controller, addr, &probes[1]);
        controller.stop();
        launch(&controller, addr, &probes[2]);
        controller.stop();
        launch(&controller, addr, &probes[3]);
        assert!(wait_until(|| probes[3].bound.load(Ordering::SeqCst) == 1).await);
        let bound_total: usize = probes.iter().map(|p| p.bound.load(Ordering::SeqCst)).sum();
        assert_eq!(bound_total, 1, "only the last run may ever have owned the port");
        assert!(TcpStream::connect(addr).await.is_ok());
        assert!(!controller.current(1) && controller.current(controller.generation.load(Ordering::SeqCst)));
    }

    // ---- peer table bound ----

    #[test]
    fn the_peer_table_is_pruned_to_its_bound_once_windows_have_expired() {
        let mut peers: HashMap<IpAddr, PeerState> = HashMap::new();
        for i in 0..5000u32 {
            peers.insert(IpAddr::V4(Ipv4Addr::from(0x0A00_0000 + i)), PeerState::default());
        }
        let later = Instant::now() + Duration::from_secs(RATE_WINDOW_SECS + 60);
        let locked_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 9, 9));
        peers.insert(
            locked_ip,
            PeerState { locked_until: Some(later + Duration::from_secs(600)), ..PeerState::default() },
        );
        prune_peers(&mut peers, later);
        assert!(peers.len() <= MAX_TRACKED_PEERS, "left {}", peers.len());
        assert!(peers.contains_key(&locked_ip), "a peer that is still locked out is kept");
    }

    #[test]
    fn a_small_peer_table_is_left_alone() {
        let mut peers: HashMap<IpAddr, PeerState> = HashMap::new();
        for i in 0..10u32 {
            peers.insert(IpAddr::V4(Ipv4Addr::from(0x0A00_0000 + i)), PeerState::default());
        }
        prune_peers(&mut peers, Instant::now() + Duration::from_secs(RATE_WINDOW_SECS + 60));
        assert_eq!(peers.len(), 10);
    }

    #[test]
    fn busy_peers_inside_their_window_are_capped_oldest_first() {
        let base = Instant::now();
        let mut peers: HashMap<IpAddr, PeerState> = HashMap::new();
        for i in 0..2000u32 {
            peers.insert(
                IpAddr::V4(Ipv4Addr::from(0x0A00_0000 + i)),
                PeerState { window_start: base + Duration::from_millis(u64::from(i)), ..PeerState::default() },
            );
        }
        let locked_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 9, 9));
        peers.insert(
            locked_ip,
            PeerState { window_start: base, locked_until: Some(base + Duration::from_secs(600)), ..PeerState::default() },
        );
        prune_peers(&mut peers, base + Duration::from_secs(1));
        assert_eq!(peers.len(), MAX_TRACKED_PEERS);
        assert!(peers.contains_key(&locked_ip), "a locked-out peer outlives older unlocked ones");
        assert!(!peers.contains_key(&IpAddr::V4(Ipv4Addr::from(0x0A00_0000))), "the oldest window goes first");
        assert!(peers.contains_key(&IpAddr::V4(Ipv4Addr::from(0x0A00_0000 + 1999))), "the newest window stays");
    }
}
