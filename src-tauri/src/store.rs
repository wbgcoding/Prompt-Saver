use super::*;
use std::time::Instant;

// ---------- SQLite store ----------
// Prompts live one-per-row (ordered); settings are a single JSON row. The DB
// is the source of truth; the legacy JSON files are imported once on upgrade
// and otherwise only used as a fallback if the DB cannot be opened.

// At-rest encryption for the local store. DPAPI (Windows, per-user, no password): the
// same Windows account always decrypts; copied elsewhere it can't be read. Falls back to
// plaintext if DPAPI is unavailable so data is never lost; legacy plaintext rows are
// re-encrypted on the next save (see load_store).
pub(crate) const ENC_PREFIX: &str = "enc:v1:";
// A NUL byte can never occur in text a user actually typed or pasted, so it is
// safe as an escape marker: the plaintext-fallback path below prepends it only
// when the plaintext itself happens to start with ENC_PREFIX (or, recursively,
// with this marker), so dec_str can tell "real ciphertext" from "plaintext that
// coincidentally looks like the marker" instead of misreading the latter as
// corrupt ciphertext and erasing it to "".
pub(crate) const PLAINTEXT_ESCAPE: &str = "\u{0}";

// Whether every write so far this run really went out sealed. Starts true and is
// flipped exactly once, by the plaintext fallback in enc_str below. The window reads
// it at startup through the `store_sealed` command and listens for `store-unsealed`
// for the rest of the session, so a failure that happens while the app is already
// open still reaches the user instead of only the log file.
//
// Deliberately never flips back: a row written in plaintext during the outage stays
// exactly that way on disk until something rewrites ITS content (write_rows only
// re-seals a row whose content actually changed, see "Diff writes"), so "the profile
// is repaired now" does not mean every row is sealed again -- only the ones touched
// from that point on. Clearing the flag on the first later success would tell the
// user the problem is fully gone while old plaintext rows could still be sitting on
// disk untouched; leaving it sticky for the rest of the session is the honest answer.
pub(crate) static STORE_SEALED: AtomicBool = AtomicBool::new(true);
// Set once the app handle exists; None while the store is still being loaded, which
// is fine: no window exists yet to receive an event, and the flag above already
// carries the state for the startup question.
//
// A closure rather than the AppHandle itself, deliberately: `enc_str` is linked into
// the unit-test binary, and a direct `handle.emit(...)` in its body pulls the whole
// webview event path (and the WebView2 import that comes with it) into a binary
// that has no webview beside it, which then fails to LOAD at all (every test
// "fails" before one of them runs). Behind this indirection the emit lives in the
// closure built in `run()`, which no test links.
pub(crate) type SealNotifier = Arc<dyn Fn() + Send + Sync>;
pub(crate) static SEAL_NOTIFY: Mutex<Option<SealNotifier>> = Mutex::new(None);

// Encrypt a JSON string for storage (prefix + base64 ciphertext).
//
// A DPAPI failure falls back to plaintext rather than refusing the write: hard-failing
// here would mean the user cannot save at all, trading a confidentiality problem for
// guaranteed data loss. It must never be SILENT though: a broken Windows profile
// would otherwise downgrade the whole store from sealed to readable with nothing to
// show for it. Logged and announced once per run (this runs per row; a full save
// would otherwise flood both the log and the window).
// Test-only escape hatch: forces the plaintext fallback below without needing a
// real DPAPI failure, which nothing in a unit test can induce. Thread-local, not a
// shared global: cargo test runs each test on its own thread, and a process-wide
// flag here would force EVERY other concurrently running test's enc_str calls into
// the fallback branch too, not just the one deliberately overriding it -- stealing
// STORE_SEALED's one-shot true-to-false transition out from under it. Compiled only
// into the test binary, so release behaviour is exactly what it was before this
// existed.
#[cfg(test)]
thread_local! {
    pub(crate) static FORCE_PLAINTEXT_FALLBACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn enc_str(plain: &str) -> String {
    #[cfg(windows)]
    {
        #[cfg(test)]
        let dpapi_ok = !FORCE_PLAINTEXT_FALLBACK.with(|f| f.get());
        #[cfg(not(test))]
        let dpapi_ok = true;
        if dpapi_ok {
            if let Some(ct) = store_crypt::protect(plain.as_bytes()) {
                return format!("{}{}", ENC_PREFIX, base64_encode(&ct));
            }
        }
        if STORE_SEALED.swap(false, Ordering::Relaxed) {
            append_crash_log(
                "DPAPI encryption unavailable - stored data is being written UNENCRYPTED. \
                 Check the Windows user profile; a repaired profile re-seals on the next save.",
            );
            // Cloned out of the lock first: notifying under it would hold the mutex
            // across a webview round trip for no reason.
            let notify = SEAL_NOTIFY.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(notify) = notify {
                notify();
            }
        }
    }
    if plain.starts_with(ENC_PREFIX) || plain.starts_with(PLAINTEXT_ESCAPE) {
        return format!("{PLAINTEXT_ESCAPE}{plain}");
    }
    plain.to_string()
}

// The state behind the startup warning: true while everything on disk is sealed,
// false once a write really had to fall back to plaintext this run.
#[tauri::command]
pub(crate) fn store_sealed() -> bool {
    STORE_SEALED.load(Ordering::Relaxed)
}

// Copy of `settings.minimize_to_tray` for the window-close handler, which runs
// on the event-loop thread and must never wait for the store lock (a backup
// holds it for a while). Refreshed whenever settings are loaded or written.
pub(crate) static MINIMIZE_TO_TRAY: AtomicBool = AtomicBool::new(true);

pub(crate) fn mirror_minimize_to_tray(settings: &Settings) {
    MINIMIZE_TO_TRAY.store(settings.minimize_to_tray, Ordering::Relaxed);
}

// Set at load_store() when the settings row was PRESENT but could not be read back
// or parsed (query error, decrypt failure, corrupt JSON) -- as opposed to genuinely
// absent, which is just a fresh install and never sets this. While true,
// write_settings_now refuses to rewrite the row: the in-memory settings are already
// Settings::default() at that point, and writing them would permanently overwrite
// whatever is actually sitting in the corrupt row with defaults. Mirrors
// Store.prompts_locked, but process-global rather than a Store field because
// save_settings/write_settings_now are the one choke point every settings mutation
// already funnels through, without a &Store to read a field from.
pub(crate) static SETTINGS_LOCKED: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub(crate) fn settings_locked() -> bool {
    SETTINGS_LOCKED.load(Ordering::Relaxed)
}

// Whether the LAST settings write actually reached disk. Flips true the moment
// write_settings_now exhausts both the database and the JSON fallback, and back
// to false the moment a write succeeds again -- so it always reflects "is the
// in-memory state ahead of what is on disk right now", not a sticky one-shot
// like STORE_SEALED. The window reads it at startup (nothing to report from a
// clean run) and gets the live flips through the existing "persist-failed"
// event, which write_settings_now already emits with the "settings" payload --
// no separate notifier plumbing needed here, unlike enc_str's SEAL_NOTIFY: this
// function already takes an AppHandle directly, so it can just emit.
pub(crate) static SETTINGS_WRITE_FAILED: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub(crate) fn settings_write_failed() -> bool {
    SETTINGS_WRITE_FAILED.load(Ordering::Relaxed)
}

// ---- Data folder lock ------------------------------------------------------
// The data folder can be pointed at a synchronised folder (OneDrive, Dropbox,
// Google Drive, iCloud Drive, Nextcloud), and the worst case for a SQLite file
// under one of those is a corrupted database once two machines write to it at
// close to the same time. `tauri_plugin_single_instance` already rules out a
// second process on the SAME machine; this only has to rule out a second one on
// a DIFFERENT machine sharing the same synced file.
//
// A small lock file (host name, process id, last-seen timestamp) sits beside
// the database, refreshed every DATA_DIR_LOCK_REFRESH_SECS by the scheduler job
// registered in run(). A machine that finds another host's lock fresh opens the
// store read-only for the rest of the session and says so in the window (a
// dedicated persistent line, not the shared expert-menu notice list, since a
// user who never opens the expert menu still has to know their edits are not
// being saved). A lock older than three missed refreshes is taken over without
// asking: the alternative is a crash on one machine locking every other one out
// permanently. This mirrors the settings write-lock (`SETTINGS_LOCKED`) as its
// model -- a process-global flag rather than a `Store` field, because the save
// functions below are the one choke point every prompt/settings write already
// funnels through, the same way the prompts_locked valve already gates every
// diff write path.
pub(crate) const DATA_DIR_LOCK_FILE: &str = "datadir.lock.json";
pub(crate) const DATA_DIR_LOCK_REFRESH_SECS: u64 = 30;
// Three missed refreshes: long enough that one slow tick under load is never
// mistaken for a crash, short enough that a real crash does not lock the other
// machine out of its own data for long.
pub(crate) const DATA_DIR_LOCK_STALE_SECS: u64 = DATA_DIR_LOCK_REFRESH_SECS * 3;

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct DataDirLockInfo {
    pub(crate) host: String,
    pub(crate) pid: u32,
    // Wall clock of the writer. Never trusted for staleness (a machine with a
    // skewed clock would take over a live lock); it only makes the content change
    // with every refresh.
    pub(crate) seen: u64,
    // Names one claim of the folder, so a process can tell its own lock from one
    // that someone else wrote over it. Empty in a lock from an older version.
    #[serde(default)]
    pub(crate) id: String,
    // Counts the refreshes of this claim.
    #[serde(default)]
    pub(crate) beat: u64,
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// Windows always sets COMPUTERNAME for the logged-in session; a missing value
// (never observed in practice) falls back to a label that still reads sensibly
// in the read-only banner rather than an empty string.
pub(crate) fn local_host_name() -> String {
    std::env::var("COMPUTERNAME").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "this computer".to_string())
}

pub(crate) fn data_dir_lock_path(dir: &Path) -> PathBuf {
    dir.join(DATA_DIR_LOCK_FILE)
}

pub(crate) fn read_data_dir_lock(path: &Path) -> Option<DataDirLockInfo> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

// Never written in place: another process reads this file while it is refreshed.
// A refused swap leaves the old lock and the next refresh tick tries again.
pub(crate) fn write_data_dir_lock(path: &Path, info: &DataDirLockInfo) -> bool {
    serde_json::to_string(info).is_ok_and(|json| write_bytes_atomic_strict(path, json.as_bytes()).is_ok())
}

pub(crate) static DATA_DIR_READONLY: AtomicBool = AtomicBool::new(false);
pub(crate) static DATA_DIR_LOCK_HOST: Mutex<Option<String>> = Mutex::new(None);

// Set by enable_demo_mode()/disable_demo_mode() once each has flushed
// whatever it needs to flush and is about to flip the "datapath" redirect
// (to the demo folder, or back to the real one) and call app.restart().
// Between that flip and the process actually exiting, the exit's own
// RunEvent::Exit flush (or a 250ms settings_flush_tick landing in the same
// window) -- or, on the enable side, any ordinary command still in flight --
// must not be allowed to write anything: at that point db_conn()/data_dir()
// already resolve to the OTHER store than the one just flushed, and a write
// there would land real data in the demo store (or demo data in the real
// one). Reset to false on a failed flip (see both commands' own Err paths) --
// left on, every save in the process would silently fail forever even though
// the switch never actually completed. On success it is never cleared
// explicitly: the process it belongs to is about to be replaced by
// app.restart(), and the fresh process starts with the default `false` again.
pub(crate) static DEMO_TRANSITION_LOCKED: AtomicBool = AtomicBool::new(false);

pub(crate) enum LockOutcome {
    Owned,
    ReadOnly(String),
}

// Who is asking: this machine, this process, and the id of this claim.
pub(crate) struct LockMe<'a> {
    pub(crate) host: &'a str,
    pub(crate) pid: u32,
    pub(crate) id: &'a str,
}

// Judges a foreign lock dead from what THIS process saw on its own monotonic
// clock, never from the timestamp the other machine wrote: the lock is dead only
// when its content has not changed for DATA_DIR_LOCK_STALE_SECS of our own
// observation. A live owner rewrites it (counter and time) every refresh, a
// sleeping or crashed one does not.
pub(crate) struct LockObserver {
    key: Option<String>,
    since: Instant,
}

impl LockObserver {
    pub(crate) fn new(now: Instant) -> Self {
        Self { key: None, since: now }
    }

    pub(crate) fn is_dead(&mut self, key: &str, now: Instant) -> bool {
        if self.key.as_deref() != Some(key) {
            self.key = Some(key.to_string());
            self.since = now;
            return false;
        }
        now.saturating_duration_since(self.since) >= Duration::from_secs(DATA_DIR_LOCK_STALE_SECS)
    }
}

fn lock_content_key(e: &DataDirLockInfo) -> String {
    format!("{}|{}|{}|{}|{}", e.host, e.pid, e.id, e.beat, e.seen)
}

// The claim itself, free of statics so tests can race it on one folder. With no
// lock file the claim is `create_new`, which only one of any number of
// simultaneous starters can win. Any other failure of that call is NOT ownership:
// the lock is read and judged like a foreign one, and a folder that cannot even
// be read counts as busy until the next try. A foreign lock is taken over only
// once the observer has seen it unchanged for the stale limit; after a takeover
// the file is read back and the claim holds only if it still carries our id.
pub(crate) fn claim_data_dir_lock(path: &Path, me: &LockMe, obs: &mut LockObserver, now: Instant, settle: Duration) -> LockOutcome {
    use std::io::Write;
    let unknown = || LockOutcome::ReadOnly("unknown".to_string());
    let info = DataDirLockInfo { host: me.host.to_string(), pid: me.pid, seen: unix_now(), id: me.id.to_string(), beat: 0 };
    let Ok(json) = serde_json::to_string(&info) else { return unknown() };
    if let Ok(mut f) = fs::OpenOptions::new().write(true).create_new(true).open(path) {
        if f.write_all(json.as_bytes()).and_then(|_| f.sync_all()).is_err() {
            drop(f);
            let _ = fs::remove_file(path);
            return unknown();
        }
        return LockOutcome::Owned;
    }
    let mut text = fs::read_to_string(path);
    if text.as_ref().ok().is_some_and(|t| serde_json::from_str::<DataDirLockInfo>(t).is_err()) {
        // Most likely a write that is still landing.
        std::thread::sleep(settle);
        text = fs::read_to_string(path);
    }
    let Ok(text) = text else { return unknown() };
    match serde_json::from_str::<DataDirLockInfo>(&text) {
        Ok(e) => {
            if e.id == me.id {
                return LockOutcome::Owned;
            }
            // A lock naming THIS machine is never foreign, whatever its age: a
            // crash-and-restart on the same computer is not the two-machine case
            // this guards against.
            if e.host != me.host && !obs.is_dead(&lock_content_key(&e), now) {
                return LockOutcome::ReadOnly(e.host);
            }
        }
        Err(_) => {
            if !obs.is_dead(&format!("raw|{text}"), now) {
                return unknown();
            }
        }
    }
    if write_bytes_atomic_strict(path, json.as_bytes()).is_err() {
        return unknown();
    }
    std::thread::sleep(settle);
    match read_data_dir_lock(path) {
        Some(e) if e.id == me.id => LockOutcome::Owned,
        Some(e) => LockOutcome::ReadOnly(e.host),
        None => unknown(),
    }
}

pub(crate) enum RefreshOutcome {
    Kept,
    // The file names another claim: ownership is gone.
    Lost(String),
    // Unreadable right now; nothing was written.
    Skipped,
}

// One refresh of a lock this process owns: read first, rewrite only our own
// claim, never overwrite someone else's.
pub(crate) fn refresh_owned_lock(path: &Path, me: &LockMe, beat: u64) -> RefreshOutcome {
    let fresh = DataDirLockInfo { host: me.host.to_string(), pid: me.pid, seen: unix_now(), id: me.id.to_string(), beat };
    match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<DataDirLockInfo>(&text) {
            Ok(e) if e.id == me.id => {
                write_data_dir_lock(path, &fresh);
                RefreshOutcome::Kept
            }
            Ok(e) => RefreshOutcome::Lost(e.host),
            Err(_) => RefreshOutcome::Skipped,
        },
        // A vanished lock is put back the way a first claim is made, so a claim
        // another machine made in that gap is never overwritten: it simply wins,
        // and the next refresh reads it and reports the loss.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            use std::io::Write;
            let Ok(json) = serde_json::to_string(&fresh) else { return RefreshOutcome::Skipped };
            match fs::OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(mut f) => {
                    if f.write_all(json.as_bytes()).and_then(|_| f.sync_all()).is_err() {
                        drop(f);
                        let _ = fs::remove_file(path);
                        return RefreshOutcome::Skipped;
                    }
                    RefreshOutcome::Kept
                }
                Err(_) => RefreshOutcome::Skipped,
            }
        }
        Err(_) => RefreshOutcome::Skipped,
    }
}

// This process's identity for the lock: the claim id is minted once.
pub(crate) fn lock_claim_id() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(gen_id)
}

pub(crate) static DATA_DIR_LOCK_OWNED: AtomicBool = AtomicBool::new(false);
static DATA_DIR_LOCK_BEAT: AtomicU64 = AtomicU64::new(0);
static DATA_DIR_LOCK_WATCH: Mutex<Option<LockObserver>> = Mutex::new(None);
const LOCK_SETTLE: Duration = Duration::from_millis(300);

fn apply_lock_outcome(outcome: LockOutcome) {
    match outcome {
        LockOutcome::ReadOnly(host) => {
            DATA_DIR_LOCK_OWNED.store(false, Ordering::Relaxed);
            DATA_DIR_READONLY.store(true, Ordering::Relaxed);
            *DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()) = Some(host);
        }
        LockOutcome::Owned => {
            DATA_DIR_LOCK_OWNED.store(true, Ordering::Relaxed);
            DATA_DIR_READONLY.store(false, Ordering::Relaxed);
            *DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }
}

fn claim_with_process_state(dir: &Path) -> LockOutcome {
    let host = local_host_name();
    let me = LockMe { host: &host, pid: std::process::id(), id: lock_claim_id() };
    let now = Instant::now();
    let mut guard = DATA_DIR_LOCK_WATCH.lock().unwrap_or_else(|e| e.into_inner());
    let obs = guard.get_or_insert_with(|| LockObserver::new(now));
    claim_data_dir_lock(&data_dir_lock_path(dir), &me, obs, now, LOCK_SETTLE)
}

// Decides read-only vs. owned for `dir` and, if owned, claims it. Called once at
// load_store() and again after set_data_dir() points the app at a different
// folder. The `dataDirLock` toggle (default on) skips all of this when off,
// restoring today's behaviour with no lock file and no read-only state. A
// foreign lock the observer has not yet watched for the whole stale limit keeps
// the store read-only; the refresh tick keeps watching it.
pub(crate) fn acquire_data_dir_lock(dir: &Path, enabled: bool) {
    *DATA_DIR_LOCK_WATCH.lock().unwrap_or_else(|e| e.into_inner()) = None;
    DATA_DIR_LOCK_OWNED.store(false, Ordering::Relaxed);
    if !enabled {
        DATA_DIR_READONLY.store(false, Ordering::Relaxed);
        *DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()) = None;
        return;
    }
    apply_lock_outcome(claim_with_process_state(dir));
}

// The scheduler tick (see "data-dir-lock" in run()). An owner reads the lock
// first and gives the folder up when it names another claim, so it never erases
// the lock of the process that took over (a sleep longer than the stale limit, a
// skewed clock). A read-only process never writes while the owner is alive; it
// keeps watching, and takes the folder over once the lock has stood still for the
// whole stale limit.
pub(crate) fn refresh_data_dir_lock(dir: &Path) {
    let path = data_dir_lock_path(dir);
    if DATA_DIR_LOCK_OWNED.load(Ordering::Relaxed) {
        let host = local_host_name();
        let me = LockMe { host: &host, pid: std::process::id(), id: lock_claim_id() };
        let beat = DATA_DIR_LOCK_BEAT.fetch_add(1, Ordering::Relaxed) + 1;
        if let RefreshOutcome::Lost(other) = refresh_owned_lock(&path, &me, beat) {
            apply_lock_outcome(LockOutcome::ReadOnly(other));
        }
    } else if DATA_DIR_READONLY.load(Ordering::Relaxed) && DATA_DIR_LOCK_WATCH.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
        apply_lock_outcome(claim_with_process_state(dir));
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DataDirLockStatus {
    pub(crate) read_only: bool,
    pub(crate) other_host: Option<String>,
}

// Polled once at startup (init(), the same shape as store_sealed/settings_locked)
// to fill in the persistent read-only banner before the first render.
#[tauri::command]
pub(crate) fn data_dir_lock_status() -> DataDirLockStatus {
    DataDirLockStatus {
        read_only: DATA_DIR_READONLY.load(Ordering::Relaxed),
        other_host: DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    }
}

// The single choke point save_prompts/save_prompts_and_trash/save_trash/
// write_settings_now all call first: real refusal, not a silent no-op -- every
// blocked write emits the same event with the same host name, so the window
// can show the same clear message no matter which action triggered it.
pub(crate) fn data_dir_write_blocked(app: &AppHandle) -> bool {
    // No event emitted here (unlike the readonly case below): the window is
    // already on its way down for app.restart(), and this is not a condition
    // the user needs a banner about.
    if DEMO_TRANSITION_LOCKED.load(Ordering::Relaxed) {
        return true;
    }
    if !DATA_DIR_READONLY.load(Ordering::Relaxed) {
        return false;
    }
    let host = DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let _ = app.emit("data-dir-readonly-write-blocked", host);
    true
}

// Known synchronised-folder conventions: the environment variable a client
// publishes when it has one (more reliable than a folder name a business tenant
// can rename in Explorer without moving the actual sync root), then the folder
// name itself. Returns the provider's display name, or None for an ordinary
// local folder.
pub(crate) fn sync_folder_hint(path: &Path) -> Option<&'static str> {
    let lower = path.to_string_lossy().to_lowercase();
    for var in ["OneDriveCommercial", "OneDriveConsumer", "OneDrive"] {
        if let Some(root) = std::env::var_os(var) {
            let root_lower = root.to_string_lossy().to_lowercase();
            if !root_lower.is_empty() && lower.starts_with(&root_lower) {
                return Some("OneDrive");
            }
        }
    }
    const MARKERS: &[(&str, &str)] = &[
        ("onedrive", "OneDrive"),
        ("dropbox", "Dropbox"),
        ("google drive", "Google Drive"),
        ("googledrive", "Google Drive"),
        ("icloud drive", "iCloud Drive"),
        ("icloud", "iCloud Drive"),
        ("nextcloud", "Nextcloud"),
    ];
    MARKERS.iter().find(|(marker, _)| lower.contains(marker)).map(|(_, name)| *name)
}

// Used both when picking a new data folder (before set_data_dir commits to it)
// and at startup against the folder already in use, so the same warning applies
// whichever way a sync folder ends up configured.
#[tauri::command]
pub(crate) fn path_sync_folder_hint(path: String) -> Option<String> {
    sync_folder_hint(Path::new(&path)).map(str::to_string)
}

#[tauri::command]
pub(crate) fn current_data_dir_sync_hint(app: AppHandle) -> Option<String> {
    sync_folder_hint(&data_dir(&app)).map(str::to_string)
}

// Decrypt a stored value. Legacy (un-prefixed) values pass through for migration; an
// undecryptable prefixed value (e.g. another Windows account) yields "" so the row is
// skipped rather than fed garbage to the JSON parser.
pub(crate) fn dec_str(stored: &str) -> String {
    if let Some(rest) = stored.strip_prefix(PLAINTEXT_ESCAPE) {
        return rest.to_string();
    }
    #[cfg(windows)]
    if let Some(b64) = stored.strip_prefix(ENC_PREFIX) {
        let ct = base64_decode(b64);
        if !ct.is_empty() {
            if let Some(pt) = store_crypt::unprotect(&ct) {
                if let Ok(s) = String::from_utf8(pt) {
                    return s;
                }
            }
        }
        return String::new();
    }
    stored.to_string()
}

// Schema + migration probe for a data.db connection, run once per data folder and
// process (see db_conn_at below). Returns false on any failure; db_conn_at then
// fails closed for that call and tries again on the next one.
pub(crate) fn db_conn_init_schema(conn: &Connection) -> bool {
    if conn
        .execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS idx_copy_log_ts ON copy_log(ts);
             CREATE INDEX IF NOT EXISTS idx_copy_log_id ON copy_log(id);
             CREATE TABLE IF NOT EXISTS prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');
             CREATE INDEX IF NOT EXISTS idx_pv_prompt ON prompt_versions(prompt_id);
             CREATE TABLE IF NOT EXISTS clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS edit_state(id TEXT PRIMARY KEY, kind TEXT NOT NULL, target TEXT NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL, ops TEXT NOT NULL, updated_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS editor_drafts(id TEXT PRIMARY KEY, prompt_id TEXT NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '', updated_at INTEGER NOT NULL);",
        )
        .is_err()
    {
        return false;
    }
    // First-ever schema migration in this app: `prompt_versions` predates
    // `variants`. `CREATE TABLE IF NOT EXISTS` above already gives a brand-new
    // install the column; an EXISTING data.db still has the old 5-column table.
    // Checked via PRAGMA table_info rather than blindly firing ALTER TABLE and
    // swallowing whatever comes back: that would also swallow a REAL failure
    // (locked/full/corrupt disk), silently leaving the column missing with no way
    // to tell the difference from "already migrated".
    let has_variants_column = conn
        .prepare("SELECT 1 FROM pragma_table_info('prompt_versions') WHERE name = 'variants'")
        .and_then(|mut s| s.exists([]))
        .unwrap_or(true); // a PRAGMA query failing here means something is already
                           // badly wrong with the connection: do not also spam an
                           // ALTER TABLE into that same broken state.
    if !has_variants_column {
        // The whole point of checking has_variants_column first (rather than
        // firing the ALTER unconditionally and swallowing whatever comes back) was
        // to tell "already migrated" apart from a REAL failure -- discarding the
        // ALTER's own result here threw that away again. A real failure now fails
        // db_conn itself, which every caller already treats as "DB unavailable".
        if conn.execute("ALTER TABLE prompt_versions ADD COLUMN variants TEXT NOT NULL DEFAULT ''", []).is_err() {
            return false;
        }
    }
    true
}

pub(crate) fn db_conn(app: &AppHandle) -> Option<Connection> {
    // The single choke point EVERY writer goes through, not just the four that
    // also call data_dir_write_blocked() (save_prompts/save_trash/
    // save_prompts_and_trash/write_settings_now): db_conn(app) is also called
    // directly by save_editor_draft, save_edit_state, record_copy's copy_log
    // append, version snapshots and more, none of which check that flag
    // themselves. Checking it here instead of (or in addition to) each call
    // site means enable_demo_mode's/disable_demo_mode's DEMO_TRANSITION_LOCKED
    // window blocks every one of them, not only the ones already funneled through
    // data_dir_write_blocked. None here reads exactly like "DB unavailable" to
    // every existing caller, which already handles it (an error result, or a
    // JSON-fallback path that goes through data_dir_write_blocked itself).
    if DEMO_TRANSITION_LOCKED.load(Ordering::Relaxed) {
        return None;
    }
    db_conn_at(&data_dir(app))
}

// Split out of db_conn() so the command line's headless path (no AppHandle to
// resolve a folder from) can open the exact same connection, with the exact
// same pragma and one-time schema init, as every other caller.
pub(crate) fn db_conn_at(dir: &Path) -> Option<Connection> {
    let conn = Connection::open(dir.join("data.db")).ok()?;
    // busy_timeout is connection-scoped (NOT persisted in the DB file, unlike
    // journal_mode below) so it must be set on every connection, but the schema
    // batch + migration probe are idempotent and only ever need to run once per
    // process: every call used to pay for a 10-statement execute_batch plus a
    // pragma_table_info probe just to open a connection for one write.
    conn.execute_batch("PRAGMA busy_timeout=3000;").ok()?;
    // Keyed by folder, and only a SUCCESS is remembered: a store adopted from another
    // folder mid-session still gets its schema run, and one transient failure at the
    // first open (WAL switch, busy) does not disable the database for the process.
    static SCHEMA_READY: Mutex<Option<std::collections::HashSet<PathBuf>>> = Mutex::new(None);
    let already = SCHEMA_READY
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|set| set.contains(dir));
    if !already {
        if !db_conn_init_schema(&conn) {
            return None;
        }
        SCHEMA_READY
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert_with(std::collections::HashSet::new)
            .insert(dir.to_path_buf());
    }
    Some(conn)
}

// What the prompts/trash tables are known to hold: "<table>\u{1}<id>" → a hash of
// that row's PLAINTEXT json. Every row is sealed before it is stored and sealing
// the same text twice gives different bytes, so a stored blob cannot be compared
// against a new one directly: and decrypting the whole table to compare would
// cost exactly what this map exists to save. An entry means "the row with this id
// currently in that table decodes to content hashing to this value", which is what
// lets a save skip re-sealing every untouched row.
//
// Anything that changes those tables from OUTSIDE write_rows: a restored or
// swapped data.db, a raw DELETE, a move to a different data folder: must clear
// the map (Store::forget_written_rows), or the next save trusts an entry that no
// longer describes the disk and skips a row that really did need writing.
pub(crate) type WrittenRows = HashMap<String, u64>;

pub(crate) fn row_key(table: &str, id: &str) -> String {
    // \u{1} cannot occur in a table name, so the two halves can never run together.
    format!("{table}\u{1}{id}")
}

pub(crate) fn row_hash(json: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    json.hash(&mut h);
    h.finish()
}

// Two prompts sharing one id can only ever be a bug in the caller's list: writing
// it would silently collapse them into a single row (and, with the old replace-all
// write, hit the PRIMARY KEY instead). Reject the whole write, exactly as before.
pub(crate) fn duplicate_row_id_error(table: &str) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY),
        Some(format!("duplicate id in {table}")),
    )
}

// Bring one table in line with `rows`, touching as little as possible: rows that
// disappeared are deleted, rows whose content changed are re-sealed and rewritten,
// and rows that merely moved get their `ord` corrected: reordering costs no
// encryption at all. Always inside the caller's own transaction. `table` is always
// one of our own two hardcoded table names below, never user input, so
// interpolating it into the SQL text is safe.
//
// Returns the map entries describing the rows as they now stand. The caller adopts
// them only after its COMMIT actually succeeded, so a rolled-back or failed write
// never leaves the process believing a row reached the disk.
pub(crate) fn write_rows(
    tx: &Transaction,
    table: &str,
    rows: &[Prompt],
    written: &WrittenRows,
) -> rusqlite::Result<WrittenRows> {
    let mut keep: std::collections::HashSet<&str> = std::collections::HashSet::with_capacity(rows.len());
    for p in rows {
        if !keep.insert(p.id.as_str()) {
            return Err(duplicate_row_id_error(table));
        }
    }
    // What the table holds right now: ids and positions only, nothing decrypted.
    // Row PRESENCE is taken from here rather than from the map, so a row that is
    // missing from the table is re-inserted even if the map still lists it.
    let mut present: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = tx.prepare(&format!("SELECT id, ord FROM {table}"))?;
        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            present.insert(r.get(0)?, r.get(1)?);
        }
    }
    {
        // A row missing from `keep` is only deleted if THIS process already knew
        // about it (present in `written`, its own last-synced view of the table).
        // With `allowMultipleInstances`, another process can commit a brand-new
        // row between this process's own load and this save; that row is present
        // in the DB but never made it into this process's in-memory list or its
        // `written` map. Deleting on absence-from-`keep` alone would treat that
        // foreign row as "removed by the user" and destroy it, even though this
        // process never showed it to anyone. Requiring prior knowledge keeps a
        // real, user-initiated deletion working (the row was loaded, so it is in
        // `written`) while no longer erasing another instance's unseen work.
        let mut del = tx.prepare(&format!("DELETE FROM {table} WHERE id = ?1"))?;
        for id in present.keys() {
            if !keep.contains(id.as_str()) && written.contains_key(&row_key(table, id)) {
                del.execute(params![id])?;
            }
        }
    }
    let mut fresh = WrittenRows::with_capacity(rows.len());
    let mut ins = tx.prepare(&format!("INSERT INTO {table}(id, ord, data) VALUES(?1, ?2, ?3)"))?;
    let mut upd = tx.prepare(&format!("UPDATE {table} SET ord = ?2, data = ?3 WHERE id = ?1"))?;
    let mut moved = tx.prepare(&format!("UPDATE {table} SET ord = ?2 WHERE id = ?1"))?;
    for (i, p) in rows.iter().enumerate() {
        let json = serde_json::to_string(p).unwrap_or_default();
        let hash = row_hash(&json);
        let key = row_key(table, &p.id);
        let ord = i as i64;
        match present.get(&p.id) {
            Some(&cur_ord) if written.get(&key) == Some(&hash) => {
                if cur_ord != ord {
                    moved.execute(params![p.id, ord])?;
                }
            }
            Some(_) => {
                upd.execute(params![p.id, ord, enc_str(&json)])?;
            }
            None => {
                ins.execute(params![p.id, ord, enc_str(&json)])?;
            }
        }
        fresh.insert(key, hash);
    }
    Ok(fresh)
}

// Swap in one table's half of the map, after that table's write committed.
pub(crate) fn adopt_written_rows(written: &mut WrittenRows, table: &str, fresh: WrittenRows) {
    let prefix = row_key(table, "");
    written.retain(|k, _| !k.starts_with(&prefix));
    written.extend(fresh);
}

pub(crate) fn db_write_prompts(conn: &mut Connection, prompts: &[Prompt], written: &mut WrittenRows) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    let fresh = write_rows(&tx, "prompts", prompts, written)?;
    tx.commit()?;
    adopt_written_rows(written, "prompts", fresh);
    Ok(())
}

// Same schema and shape as db_write_prompts/db_load's prompt half, but its own
// table. A prompt "in the trash" simply isn't in `prompts` any more, so every
// existing read of store.prompts (grid, library, tray, hotkeys, export, stats,
// duplicate finder...) stays correct with zero changes: the alternative, a
// deleted_at flag on Prompt filtered everywhere, would have needed every one of
// those call sites found and fixed, with real risk of missing one.
pub(crate) fn db_write_trash(conn: &mut Connection, trash: &[Prompt], written: &mut WrittenRows) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    let fresh = write_rows(&tx, "trash", trash, written)?;
    tx.commit()?;
    adopt_written_rows(written, "trash", fresh);
    Ok(())
}

// delete_prompt/restore_prompt move one row between these two tables. Writing
// them as two independent transactions (the old db_write_prompts + db_write_trash
// pair) left a window where the first commit could durably land while the second
// failed, stranding the prompt in neither table. One transaction for both DELETE+
// INSERT pairs means either the whole move lands or neither half does.
pub(crate) fn db_write_prompts_and_trash(
    conn: &mut Connection,
    prompts: &[Prompt],
    trash: &[Prompt],
    written: &mut WrittenRows,
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    let fresh_prompts = write_rows(&tx, "prompts", prompts, written)?;
    let fresh_trash = write_rows(&tx, "trash", trash, written)?;
    tx.commit()?;
    adopt_written_rows(written, "prompts", fresh_prompts);
    adopt_written_rows(written, "trash", fresh_trash);
    Ok(())
}

// An import that replaces both halves: the prompt rows and the settings row
// land in one transaction, so either both are on disk or neither is. Two
// separate commits could leave one half imported while the command reported a
// failure.
pub(crate) fn db_write_prompts_and_settings(
    conn: &mut Connection,
    prompts: &[Prompt],
    settings: &Settings,
    written: &mut WrittenRows,
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    let fresh = write_rows(&tx, "prompts", prompts, written)?;
    db_write_settings(&tx, settings)?;
    tx.commit()?;
    adopt_written_rows(written, "prompts", fresh);
    Ok(())
}

// `written` collects what the table was found to hold (see WrittenRows) so the
// first save of the session already only writes what actually changed. Recorded
// straight from the rows as they were read: never from a later, migrated or
// replaced list, which would claim content for the disk that never reached it.
//
// Second return value is a failure count, same convention as db_load: a row
// that could not be read back, or an encrypted row that would not decrypt or
// parse, is present-but-unaccounted-for, not simply absent: the caller must
// fold it into the same write-lock decision as db_load's own failures, or a
// trash mutation could permanently erase whatever this omitted.
pub(crate) fn db_load_trash(conn: &Connection, written: &mut WrittenRows) -> (Vec<Prompt>, usize) {
    let mut trash = Vec::new();
    let mut failures = 0usize;
    match conn.prepare("SELECT data FROM trash ORDER BY ord") {
        Ok(mut stmt) => match stmt.query_map([], |r| r.get::<_, String>(0)) {
            Ok(rows) => {
                for row in rows {
                    match row {
                        Ok(data) => {
                            let encrypted = data.starts_with(ENC_PREFIX);
                            match serde_json::from_str::<Prompt>(&dec_str(&data)) {
                                Ok(p) => {
                                    // Same rule as db_load: an unsealed row stays out of
                                    // the map so the next write seals it instead of
                                    // skipping it as unchanged.
                                    if encrypted {
                                        written.insert(
                                            row_key("trash", &p.id),
                                            row_hash(&serde_json::to_string(&p).unwrap_or_default()),
                                        );
                                    }
                                    trash.push(p);
                                }
                                Err(_) if encrypted => failures += 1,
                                Err(_) => {}
                            }
                        }
                        Err(_) => failures += 1,
                    }
                }
            }
            Err(_) => failures += 1,
        },
        Err(_) => failures += 1,
    }
    (trash, failures)
}

pub(crate) fn db_write_settings(conn: &Connection, settings: &Settings) -> rusqlite::Result<()> {
    let json = enc_str(&serde_json::to_string(settings).unwrap_or_default());
    conn.execute(
        "INSERT INTO meta(key, value) VALUES('settings', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        params![json],
    )?;
    Ok(())
}

// Returns (prompts, settings, any_plaintext, load_failures). `load_failures`
// counts rows that are present-but-unaccounted-for: an encrypted row that
// would not decrypt/parse, a row the SQLite driver could not read back at all,
// or the SELECT itself failing to prepare/run (corrupt table, locked file):
// every one of them means the true prompt count is NOT known to be what this
// function returned, so the caller must avoid any destructive rewrite of the
// prompts table rather than treat a partial result as the whole truth.
pub(crate) fn db_load(conn: &Connection, written: &mut WrittenRows) -> (Vec<Prompt>, Option<Settings>, bool, usize, bool) {
    let mut prompts = Vec::new();
    let mut plaintext = false; // any legacy unencrypted row → re-save once to encrypt
    let mut failures = 0usize;
    match conn.prepare("SELECT data FROM prompts ORDER BY ord") {
        Ok(mut stmt) => match stmt.query_map([], |r| r.get::<_, String>(0)) {
            Ok(rows) => {
                for row in rows {
                    match row {
                        Ok(data) => {
                            let encrypted = data.starts_with(ENC_PREFIX);
                            if !encrypted {
                                plaintext = true;
                            }
                            match serde_json::from_str::<Prompt>(&dec_str(&data)) {
                                Ok(p) => {
                                    // A legacy plaintext row must be re-saved (sealed) even
                                    // though its content did not change, so it is deliberately
                                    // left out of the map and thus counts as changed.
                                    if encrypted {
                                        written.insert(
                                            row_key("prompts", &p.id),
                                            row_hash(&serde_json::to_string(&p).unwrap_or_default()),
                                        );
                                    }
                                    prompts.push(p);
                                }
                                // An encrypted row that won't decrypt/parse is present-but-unreadable;
                                // a plaintext row that won't parse is genuinely corrupt (not a key issue).
                                Err(_) if encrypted => failures += 1,
                                Err(_) => {}
                            }
                        }
                        Err(_) => failures += 1,
                    }
                }
            }
            Err(_) => failures += 1,
        },
        Err(_) => failures += 1,
    }
    // A genuinely absent row (fresh install, nothing saved yet) must never count as
    // a failure -- only a row that IS there but could not be read back or parsed is
    // "unreadable" in the sense that locks writes (see settings_locked below).
    let mut settings_unreadable = false;
    let raw = match conn.query_row("SELECT value FROM meta WHERE key='settings'", [], |r| {
        r.get::<_, String>(0)
    }) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(_) => {
            settings_unreadable = true;
            None
        }
    };
    if raw.as_deref().is_some_and(|v| !v.starts_with(ENC_PREFIX)) {
        plaintext = true;
    }
    let settings = raw.and_then(|s| match serde_json::from_str::<Settings>(&dec_str(&s)) {
        Ok(s) => Some(s),
        Err(_) => {
            settings_unreadable = true;
            None
        }
    });
    (prompts, settings, plaintext, failures, settings_unreadable)
}

// Peeks the "allowMultipleInstances" flag before the Tauri builder exists. The
// single-instance plugin has to be attached (or not) at builder time, before any
// AppHandle exists to resolve settings the normal way: so this mirrors
// default_data_dir()'s portable/APPDATA resolution and opens the DB directly. Any
// failure (first run, missing DB, undecryptable row) yields false: single instance,
// the safe default that matches every install before this option existed.
pub(crate) fn early_allow_multiple_instances() -> bool {
    let dir = if is_portable() {
        portable_data_dir()
    } else {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join(DATA_FOLDER))
    };
    let Some(base) = dir else { return false };
    let dir = fs::read_to_string(base.join("datapath"))
        .ok()
        .map(|s| PathBuf::from(s.trim()))
        .filter(|p| !p.as_os_str().is_empty() && p.is_dir())
        .unwrap_or(base);
    let Ok(conn) = Connection::open(dir.join("data.db")) else { return false };
    let raw: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key='settings'", [], |r| r.get(0))
        .ok();
    let Some(raw) = raw else { return false };
    serde_json::from_str::<Settings>(&dec_str(&raw))
        .map(|s| s.ui_flags.get("allowMultipleInstances").copied().unwrap_or(false))
        .unwrap_or(false)
}

// Whether any copy-history row exists; an unreadable table counts as yes, so an
// unsure case is treated as an existing install.
pub(crate) fn copy_log_has_rows(conn: &Connection) -> bool {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM copy_log)", [], |r| r.get::<_, i64>(0))
        .map(|v| v != 0)
        .unwrap_or(true)
}

// Append one copy event, then trim to the cap and the retention window. The cap
// trim uses the rowid (monotonic insertion order): "keep rows whose rowid is
// within `cap` of the newest" deletes at most one row per call in steady state
// and touches nothing during growth: O(log n), so a million-entry history is
// still cheap on every copy.
// Trim the history to the cap (newest `cap` rows) and the retention window. Rows
// kept are exactly those with rowid in (MAX-cap, MAX], i.e. at most `cap`, and
// age-prune only ever deletes from the old (low-rowid) end, so the kept block
// stays a contiguous newest-N: the cap can never be exceeded.
pub(crate) fn db_trim_copy(conn: &Connection, cap: usize, cutoff: Option<u64>) -> rusqlite::Result<()> {
    if cap == 0 {
        conn.execute("DELETE FROM copy_log", [])?;
        return Ok(());
    }
    conn.execute(
        "DELETE FROM copy_log WHERE rowid <= (SELECT MAX(rowid) FROM copy_log) - ?1",
        params![cap as i64],
    )?;
    if let Some(c) = cutoff {
        conn.execute("DELETE FROM copy_log WHERE ts > 0 AND ts < ?1", params![c as i64])?;
    }
    Ok(())
}

pub(crate) fn db_append_copy(conn: &Connection, entry: &CopyEntry, cap: usize, cutoff: Option<u64>) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO copy_log(ts, id) VALUES(?1, ?2)", params![entry.ts as i64, entry.id])?;
    db_trim_copy(conn, cap, cutoff)
}

// One journal row as shown/deletable in the UI: unlike CopyEntry (insert/trim/
// legacy-migration only), this carries the rowid a delete needs to target the exact
// row, even in grouped mode where several rows share the same prompt id.
#[derive(Serialize)]
pub(crate) struct JournalEntry {
    pub(crate) rowid: i64,
    pub(crate) id: String,
    pub(crate) ts: u64,
}

// Copy counts for the last `days` UTC days, oldest first, zero-filled: a
// single GROUP BY query bucketed by day-since-epoch, merged against a dense day
// range in Rust (cheaper than one query per day, and never skips an empty day).
pub(crate) fn db_daily_copies(conn: &Connection, days: u32) -> Vec<DayCount> {
    const DAY: u64 = 86400;
    let today_bucket = now_secs() / DAY;
    let start_bucket = today_bucket.saturating_sub(days as u64 - 1);
    let mut counts: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT ts / 86400, COUNT(*) FROM copy_log WHERE ts >= ?1 GROUP BY ts / 86400") {
        if let Ok(rows) = stmt.query_map(params![(start_bucket * DAY) as i64], |r| {
            Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as usize))
        }) {
            counts.extend(rows.flatten());
        }
    }
    (0..days as u64)
        .map(|i| {
            let bucket = start_bucket + i;
            DayCount { ts: bucket * DAY, count: counts.get(&bucket).copied().unwrap_or(0) }
        })
        .collect()
}

// Newest copy events for the journal. grouped = one row per prompt (its most
// recent copy); otherwise every event. Capped so the UI never renders millions.
pub(crate) fn db_recent_copies(conn: &Connection, limit: usize, grouped: bool) -> Vec<JournalEntry> {
    let sql = if grouped {
        // The whole newest row per prompt, not MAX(rowid) beside an independently
        // computed MAX(ts): two aggregates are evaluated separately, so the row id
        // the delete button targets could belong to a different copy than the
        // timestamp shown next to it.
        "SELECT rowid, id, ts FROM copy_log
         WHERE rowid IN (SELECT MAX(rowid) FROM copy_log GROUP BY id)
         ORDER BY rowid DESC LIMIT ?1"
    } else {
        "SELECT rowid, id, ts FROM copy_log ORDER BY rowid DESC LIMIT ?1"
    };
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(sql) {
        if let Ok(rows) = stmt.query_map(params![limit as i64], |r| {
            Ok(JournalEntry { rowid: r.get(0)?, id: r.get(1)?, ts: r.get::<_, i64>(2)? as u64 })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}

// ---- Prompt version history ----
// Snapshots of a prompt's previous name/text, encrypted like the prompt rows. The
// table lives in data.db (not the settings blob) so history never bloats settings.
#[derive(Serialize)]
pub(crate) struct PromptVersion {
    pub(crate) id: i64,
    pub(crate) ts: u64,
    pub(crate) name: String,
    pub(crate) text: String,
    // Entries ARE content: a version snapshot without them would let a
    // restore silently drop entries 2..n. JSON-encoded in its own DB column
    // (unlike the export formats, nothing here needs to stay human-editable).
    pub(crate) variants: Vec<String>,
}

pub(crate) fn encode_variants(variants: &[String]) -> String {
    serde_json::to_string(variants).unwrap_or_default()
}
pub(crate) fn decode_variants(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

pub(crate) fn history_enabled(settings: &Settings) -> bool {
    settings.ui_flags.get("promptHistory") != Some(&false) // on by default
}
pub(crate) fn history_cap(settings: &Settings) -> usize {
    settings.ui_values.get("historyPerPrompt").copied().unwrap_or(20.0).clamp(5.0, 100.0) as usize
}

pub(crate) fn db_add_version(conn: &Connection, prompt_id: &str, ts: u64, name: &str, text: &str, variants: &[String], cap: usize) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO prompt_versions(prompt_id, ts, name, text, variants) VALUES(?1, ?2, ?3, ?4, ?5)",
        params![prompt_id, ts as i64, enc_str(name), enc_str(text), enc_str(&encode_variants(variants))],
    )?;
    // Keep only the newest `cap` snapshots for this prompt.
    conn.execute(
        "DELETE FROM prompt_versions WHERE prompt_id = ?1 AND id NOT IN
            (SELECT id FROM prompt_versions WHERE prompt_id = ?1 ORDER BY id DESC LIMIT ?2)",
        params![prompt_id, cap as i64],
    )?;
    Ok(())
}
pub(crate) fn db_list_versions(conn: &Connection, prompt_id: &str) -> Vec<PromptVersion> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, ts, name, text, variants FROM prompt_versions WHERE prompt_id = ?1 ORDER BY id DESC") {
        if let Ok(rows) = stmt.query_map(params![prompt_id], |r| {
            Ok(PromptVersion {
                id: r.get::<_, i64>(0)?,
                ts: r.get::<_, i64>(1)? as u64,
                name: dec_str(&r.get::<_, String>(2)?),
                // Display only: an old snapshot reads in the new spelling without a write.
                text: upgrade_placeholders(&dec_str(&r.get::<_, String>(3)?)),
                variants: decode_variants(&dec_str(&r.get::<_, String>(4)?)).iter().map(|v| upgrade_placeholders(v)).collect(),
            })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}
// One prompt's last real content edit (name or text change: the only kind
// that gets a version row, see update_prompt) is the newest `ts` it has in
// prompt_versions. Bundled for every prompt at once (one GROUP BY, not an N+1
// query per prompt): a prompt absent from the result has never had a version
// row: history was off for all its edits, or it was never edited since creation.
pub(crate) fn db_last_edited(conn: &Connection) -> std::collections::HashMap<String, u64> {
    let mut out = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT prompt_id, MAX(ts) FROM prompt_versions GROUP BY prompt_id") {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))) {
            out.extend(rows.flatten());
        }
    }
    out
}
pub(crate) fn db_get_version(conn: &Connection, id: i64) -> Option<(String, String, String, Vec<String>)> {
    conn.query_row(
        "SELECT prompt_id, name, text, variants FROM prompt_versions WHERE id = ?1",
        params![id],
        |r| Ok((
            r.get::<_, String>(0)?,
            dec_str(&r.get::<_, String>(1)?),
            upgrade_placeholders(&dec_str(&r.get::<_, String>(2)?)),
            decode_variants(&dec_str(&r.get::<_, String>(3)?)).iter().map(|v| upgrade_placeholders(v)).collect(),
        )),
    )
    .ok()
}
// Removes version history whose prompt is in neither the library nor the trash.
// Works on the plaintext id column alone, so it is safe even when the rows
// themselves could not be decrypted. Returns the number of rows removed.
// The orphan sweep deletes rows: never in a read-only instance (the store belongs
// to another machine) and never while a JSON fallback still holds prompts.
pub(crate) fn version_sweep_allowed(dir: &Path, read_only: bool) -> bool {
    !read_only && !dir.join("prompts.json").exists()
}

pub(crate) fn db_sweep_orphan_versions(conn: &Connection) -> usize {
    match conn.execute(
        "DELETE FROM prompt_versions WHERE prompt_id NOT IN (SELECT id FROM prompts UNION SELECT id FROM trash)",
        [],
    ) {
        Ok(n) => n,
        Err(e) => {
            append_crash_log(&format!("could not sweep orphaned version history: {e}"));
            0
        }
    }
}
pub(crate) fn db_delete_versions(conn: &Connection, prompt_id: &str) {
    // Called from a PERMANENT purge -- the prompt is meant to be entirely gone,
    // so a failure here silently leaving its text behind in prompt_versions is
    // a privacy gap, not just clutter.
    if let Err(e) = conn.execute("DELETE FROM prompt_versions WHERE prompt_id = ?1", params![prompt_id]) {
        append_crash_log(&format!("could not delete version history for purged prompt {prompt_id}: {e}"));
    }
}

// ---- Clipboard watcher ----
#[derive(Serialize)]
pub(crate) struct ClipItem {
    pub(crate) id: i64,
    pub(crate) ts: u64,
    pub(crate) text: String,
}
pub(crate) fn text_hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

// Fingerprint of a seeded example prompt's editable content, so "remove examples"
// can tell whether the user has since made one their own. Mirrors SlimPrompt's own
// image_rev (0 for no image, otherwise a hash of the image bytes) rather than a
// separate stored field.
pub(crate) fn example_seed_fingerprint(name: &str, text: &str, image: &str) -> u64 {
    let image_rev = if image.is_empty() { 0 } else { text_hash(image) };
    text_hash(&format!("{name}\u{0}{text}\u{0}{image_rev}"))
}
pub(crate) fn db_add_clip(conn: &Connection, ts: u64, text: &str, cap: usize) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO clip_inbox(ts, text) VALUES(?1, ?2)", params![ts as i64, enc_str(text)])?;
    conn.execute(
        "DELETE FROM clip_inbox WHERE id <= (SELECT MAX(id) FROM clip_inbox) - ?1",
        params![cap as i64],
    )?;
    Ok(())
}
pub(crate) fn db_list_clips(conn: &Connection) -> Vec<ClipItem> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, ts, text FROM clip_inbox ORDER BY id DESC") {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok(ClipItem { id: r.get::<_, i64>(0)?, ts: r.get::<_, i64>(1)? as u64, text: dec_str(&r.get::<_, String>(2)?) })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}

// load_store()'s half of the relative <-> absolute round trip (the other
// half is strip_data_dir_prefix right below, its own comment has the full
// story). Pulled out as its own function, run over both `prompts` and
// `trash` (a third review finding: only `prompts` was resolved, so a demo
// attachment that got deleted during the session restored from trash with a
// bare, unresolvable filename), so it can be unit tested directly without an
// AppHandle, the same way strip_data_dir_prefix already is.
pub(crate) fn resolve_relative_file_paths(dir: &Path, rows: &mut [Prompt]) {
    for p in rows.iter_mut() {
        if !p.file_path.is_empty() && Path::new(&p.file_path).is_relative() {
            p.file_path = dir.join(&p.file_path).to_string_lossy().to_string();
        }
    }
}

// The other half of load_store()'s relative -> absolute resolution for
// `file_path` (see its own comment): reversed right before anything is
// persisted or exported, so a demo prompt's attachment is never baked into a
// saved row, a promptSync .md file or an export as an absolute path
// containing the Windows account name. Only ever touches a path whose PARENT
// is exactly the live data dir -- the one load_store() itself would have
// joined on -- so a real attachment (never placed directly inside the data
// dir by the file picker) is never touched. Every caller of db_write_prompts/
// db_write_prompts_and_trash, sync_write_all_impl and export_prompts must run
// its prompts through this first.
//
// Gated to actually being in demo mode right now (a third, then a fourth
// review finding): in portable mode the data dir IS the exe folder, and a
// custom data dir is common too, so a real user's file picked from directly
// inside that folder has an absolute path whose parent legitimately equals
// `dir`: stripping it unconditionally turned a real attachment into a bare
// filename that broke after any later data-dir move or portable/installed
// takeover. The first attempt at this gate compared `dir`'s folder NAME
// against `DEMO_SUBFOLDER`, which still fires for a real (if unlikely) data
// dir a user happens to have named `prompt-saver-demo` -- `is_demo` is an
// exact-path comparison instead (`data_dir(app) == default_data_dir(app).
// join(DEMO_SUBFOLDER)`), computed once by the caller, which already has
// `app` in scope. Demo mode is the only caller this needs to touch, so it
// only ever runs there.
// The exact-path check strip_data_dir_prefix's own comment explains: never a
// folder-name guess, always the live data dir compared against exactly where
// demo mode's own redirect would have pointed it.
pub(crate) fn data_dir_is_demo(app: &AppHandle, dir: &Path) -> bool {
    dir == default_data_dir(app).join(DEMO_SUBFOLDER)
}

pub(crate) fn strip_data_dir_prefix<'a>(dir: &Path, is_demo: bool, prompts: &'a [Prompt]) -> std::borrow::Cow<'a, [Prompt]> {
    if !is_demo {
        return std::borrow::Cow::Borrowed(prompts);
    }
    if !prompts.iter().any(|p| !p.file_path.is_empty() && Path::new(&p.file_path).parent() == Some(dir)) {
        return std::borrow::Cow::Borrowed(prompts);
    }
    let mut out = prompts.to_vec();
    for p in out.iter_mut() {
        if Path::new(&p.file_path).parent() == Some(dir) {
            if let Some(name) = Path::new(&p.file_path).file_name() {
                p.file_path = name.to_string_lossy().to_string();
            }
        }
    }
    std::borrow::Cow::Owned(out)
}

// Returns whether the prompts made it to durable storage (DB or JSON fallback):
// callers that follow a save with a destructive step (deleting an orphaned
// attachment file) must skip that step when this is false, or the deletion can
// outlive the very change that made the file orphaned.
pub(crate) fn save_prompts(app: &AppHandle, store: &Store) -> bool {
    if data_dir_write_blocked(app) {
        return false;
    }
    // Safety valve: if some rows failed to decrypt at load, the in-memory list is a
    // SUBSET of what's on disk. Rewriting the table (DELETE + re-INSERT) would delete
    // the unreadable rows permanently: so we refuse to persist prompt changes at all
    // until the situation is resolved. Prompt edits this session simply won't stick.
    if store.prompts_locked {
        return false;
    }
    let dir = data_dir(app);
    let prompts = strip_data_dir_prefix(&dir, data_dir_is_demo(app, &dir), &store.prompts);
    if let Some(mut conn) = db_conn(app) {
        // The map is only advanced by a committed write, so a failure here leaves it
        // describing exactly what the (rolled-back) table still holds: the JSON
        // fallback below must not touch it either, or the row would count as saved.
        let ok = {
            let mut written = store.written.borrow_mut();
            db_write_prompts(&mut conn, &prompts, &mut written).is_ok()
        };
        if ok {
            drop_stale_fallback(&dir, "prompts.json");
            spawn_sync_write_all(app, store); // Prompt-folders-as-Markdown: no-op unless the feature is on
            return true;
        }
    }
    // Last resort: JSON fallback. load_store() reads this back and retries the
    // DB write on next launch, so a fallback here is not itself data loss. If even
    // this fails (disk full / permission), the change really is lost: leave a
    // diagnosable trace instead of failing silently.
    if !write_sealed_json(&dir.join("prompts.json"), &prompts) {
        append_crash_log("persist failed: prompts could not be written to the database or the JSON fallback");
        let _ = app.emit("persist-failed", "prompts");
        return false;
    }
    spawn_sync_write_all(app, store);
    true
}

pub(crate) fn save_trash(app: &AppHandle, store: &Store) -> bool {
    if data_dir_write_blocked(app) {
        return false;
    }
    // Same lock as save_prompts: an unreadable-row situation must never trigger a
    // destructive rewrite of a second table either.
    if store.prompts_locked {
        return false;
    }
    let dir = data_dir(app);
    let trash = strip_data_dir_prefix(&dir, data_dir_is_demo(app, &dir), &store.trash);
    if let Some(mut conn) = db_conn(app) {
        let mut written = store.written.borrow_mut();
        if db_write_trash(&mut conn, &trash, &mut written).is_ok() {
            return true;
        }
    }
    append_crash_log("persist failed: trash could not be written to the database");
    let _ = app.emit("persist-failed", "trash");
    false
}

// The atomic counterpart to save_prompts+save_trash for delete_prompt/
// restore_prompt - see db_write_prompts_and_trash for why the two tables can no
// longer be saved as two separate commits for a single move.
pub(crate) fn save_prompts_and_trash(app: &AppHandle, store: &Store) -> bool {
    if data_dir_write_blocked(app) {
        return false;
    }
    if store.prompts_locked {
        return false;
    }
    let dir = data_dir(app);
    let demo = data_dir_is_demo(app, &dir);
    let prompts = strip_data_dir_prefix(&dir, demo, &store.prompts);
    let trash = strip_data_dir_prefix(&dir, demo, &store.trash);
    if let Some(mut conn) = db_conn(app) {
        let ok = {
            let mut written = store.written.borrow_mut();
            db_write_prompts_and_trash(&mut conn, &prompts, &trash, &mut written).is_ok()
        };
        if ok {
            drop_stale_fallback(&dir, "prompts.json");
            spawn_sync_write_all(app, store); // Prompt-folders-as-Markdown mirrors prompts, same as save_prompts
            return true;
        }
    }
    append_crash_log("persist failed: trash move could not be written to the database");
    let _ = app.emit("persist-failed", "trash");
    false
}

// Mirrors save_prompts' shape below - same DB-then-JSON-fallback order, same
// choke point every settings mutation already funnels through. Returning bool
// (instead of nothing) plus the same persist-failed event lets the existing
// frontend listener cover all of this function's callers for free instead of
// the setting silently reverting, unexplained, on the next restart.
pub(crate) fn save_settings(app: &AppHandle, settings: &Settings) -> bool {
    let _writing = settings_write_lock();
    // A full write covers whatever was collected: the counters live in this same
    // block, so once it is on disk nothing is owed any more.
    SETTINGS_DELAY.discard();
    write_settings_now(app, settings)
}

// The write itself, without the "cancel a collected write" step: the collecting
// path below calls this one so it does not clear its own mark twice.
pub(crate) fn write_settings_now(app: &AppHandle, settings: &Settings) -> bool {
    mirror_minimize_to_tray(settings);
    if data_dir_write_blocked(app) {
        return false;
    }
    // Safety valve: the row on disk is present but unreadable, so the in-memory
    // Settings is a default, not what is actually stored. Rewriting now would
    // permanently destroy the real (corrupt but recoverable-by-hand) row.
    if SETTINGS_LOCKED.load(Ordering::Relaxed) {
        return false;
    }
    if let Some(conn) = db_conn(app) {
        if db_write_settings(&conn, settings).is_ok() {
            drop_stale_fallback(&data_dir(app), "settings.json");
            // Only announced on the actual true -> false transition (swap, not a
            // plain store) -- every ordinary successful write also passes through
            // here and must not spam the window with a recovery event it never
            // regressed from.
            if SETTINGS_WRITE_FAILED.swap(false, Ordering::Relaxed) {
                let _ = app.emit("settings-write-recovered", ());
            }
            return true;
        }
    }
    if !write_sealed_json(&data_dir(app).join("settings.json"), settings) {
        append_crash_log("persist failed: settings could not be written to the database or the JSON fallback");
        SETTINGS_WRITE_FAILED.store(true, Ordering::Relaxed);
        let _ = app.emit("persist-failed", "settings");
        return false;
    }
    if SETTINGS_WRITE_FAILED.swap(false, Ordering::Relaxed) {
        let _ = app.emit("settings-write-recovered", ());
    }
    true
}

// ---- Counter writes are collected instead of going to disk one at a time ----
//
// save_settings re-encrypts and rewrites the WHOLE settings block (themes,
// layouts, window geometry, every toggle) each time it is called. Rotation
// pointers, usage counters and "last used" stamps move on every copy: and on a
// rotating prompt with auto-advance, on every single paste: so they used to buy
// one full settings write per Ctrl+V. They mark the block dirty instead, and the
// shared timer thread writes it once the marking has been quiet for a moment.
//
// Only the DISK write waits. Every marking call site has already updated
// store.settings under the store lock before it marks, so the next copy of that
// prompt still serves the right entry no matter when the write lands.
//
// What this costs in the worst case: if the process is killed outright; task
// manager, a power cut, a crash: up to one second of counter state is lost: a
// rotation pointer that stays where it was, a use count one short, a "last used"
// stamp from the previous copy. It can never cost a prompt, any text, or a
// setting the user just changed. Everything the user edits still writes through
// immediately, and every orderly exit, Windows session end, backup, export,
// import, reset and data-folder move flushes what is owed before it goes on.
pub(crate) const SETTINGS_QUIET_MS: u64 = 1000;
// How often the timer thread looks at the mark. Finer than the quiet window so
// the write lands just behind it rather than a whole window late; a round with
// nothing owed is one uncontended mutex and no disk access at all.
pub(crate) const SETTINGS_FLUSH_TICK_MS: u64 = 250;

// Marked = a settings write is owed, at the instant the mark was set. Every
// later mark within the quiet window pushes the write further out, so a burst of
// copies collapses into one write rather than one per copy.
pub(crate) struct SettingsDelay {
    pub(crate) marked: Mutex<Option<Instant>>,
    pub(crate) quiet: Mutex<Duration>,
}

impl SettingsDelay {
    pub(crate) const fn new(quiet_ms: u64) -> Self {
        SettingsDelay {
            marked: Mutex::new(None),
            quiet: Mutex::new(Duration::from_millis(quiet_ms)),
        }
    }

    pub(crate) fn mark(&self) {
        *self.marked.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    }

    // Is anything owed at all? Cheap enough for the idle tick to ask every round.
    pub(crate) fn owed(&self) -> bool {
        self.marked.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    // The ONE place that decides whether a collected write happens, so the number
    // of `true` answers is exactly the number of disk writes on this path.
    // `force` skips the quiet window: an exit or a backup cannot wait it out.
    pub(crate) fn take(&self, force: bool) -> bool {
        let quiet = *self.quiet.lock().unwrap_or_else(|e| e.into_inner());
        let mut marked = self.marked.lock().unwrap_or_else(|e| e.into_inner());
        match *marked {
            Some(at) if force || at.elapsed() >= quiet => {
                *marked = None;
                true
            }
            _ => false,
        }
    }

    // An immediate write of the same block has just happened: nothing left owed.
    pub(crate) fn discard(&self) {
        *self.marked.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

pub(crate) static SETTINGS_DELAY: SettingsDelay = SettingsDelay::new(SETTINGS_QUIET_MS);

// Serializes the writes themselves, so a collected write started on the timer
// thread can never land on top of a newer immediate one. Lock order is always
// the store lock first, this one second: nothing under this lock ever takes it.
pub(crate) static SETTINGS_WRITE: Mutex<()> = Mutex::new(());

pub(crate) fn settings_write_lock() -> std::sync::MutexGuard<'static, ()> {
    SETTINGS_WRITE.lock().unwrap_or_else(|e| e.into_inner())
}

// A counter/pointer change: already correct in memory, worth one disk write per
// quiet second instead of one per copy. The caller must have updated
// store.settings before calling this.
pub(crate) fn save_settings_soon() {
    SETTINGS_DELAY.mark();
}

// Write a collected change out now. `settings` is the live block, which the
// caller already holds under the store lock.
pub(crate) fn flush_settings(app: &AppHandle, settings: &Settings) {
    let _writing = settings_write_lock();
    if SETTINGS_DELAY.take(true) {
        write_settings_now(app, settings);
    }
}

// Same, for a caller that does not hold the store lock itself.
pub(crate) fn flush_settings_now(app: &AppHandle) {
    if !SETTINGS_DELAY.owed() {
        return;
    }
    if let Some(state) = app.try_state::<Db>() {
        let store = state.lock().unwrap_or_else(|e| e.into_inner());
        flush_settings(app, &store.settings);
    }
}

// One round of the timer thread's flush job.
pub(crate) fn settings_flush_tick(app: &AppHandle) {
    if !SETTINGS_DELAY.owed() {
        return;
    }
    let Some(state) = app.try_state::<Db>() else {
        return;
    };
    let store = state.lock().unwrap_or_else(|e| e.into_inner());
    let _writing = settings_write_lock();
    if SETTINGS_DELAY.take(false) {
        write_settings_now(app, &store.settings);
    }
}

// Pre-1.9 builds saved image prompts without copy_image but copied on click.
// The self-filling placeholder names, kept in step with DYN_VARS in ui/promptVars.js.
pub(crate) const DYN_NAMES: [&str; 16] = [
    "date", "time", "datetime", "weekday", "yesterday", "tomorrow", "week", "month",
    "year", "timestamp", "epoch", "clipboard", "username", "uuid", "random", "counter",
];

// Placeholders are written {?Name}, {?Name=default}, {?Name|A, B} and {?@date}. The
// retired spelling was {#{Name}#}, with {#{/date\}#} (and, before that, the bare
// {#{date}#}) for the self-filling names. Rewrites every old token of `text`, in
// source order; text without one comes back untouched. Idempotent: the result holds
// no complete old token. A token whose body is empty, or would grow past the body
// limit of the new spelling, stays exactly as typed.
pub(crate) fn upgrade_placeholders(text: &str) -> String {
    if !text.contains("{#{") {
        return text.to_string();
    }
    let tokens = find_old_var_tokens(text);
    if tokens.is_empty() {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for (start, end, body) in tokens {
        out.extend(chars[cursor..start].iter().copied());
        cursor = end;
        let fenced_dyn = body
            .strip_prefix('/')
            .and_then(|b| b.strip_suffix('\\'))
            .filter(|b| DYN_NAMES.contains(b));
        let rewritten = if body.is_empty() {
            None
        } else if let Some(name) = fenced_dyn {
            Some(format!("{{?@{name}}}"))
        } else if DYN_NAMES.contains(&body.as_str()) {
            Some(format!("{{?@{body}}}"))
        } else if body.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '@') {
            Some(format!("{{?{body}}}"))
        } else {
            Some(format!("{{?_{body}}}"))
        };
        match rewritten {
            Some(token) if token.chars().count() <= 203 => out.push_str(&token),
            _ => out.extend(chars[start..end].iter().copied()),
        }
    }
    out.extend(chars[cursor..].iter().copied());
    out
}

// The same rewrite for one prompt's text and every rotation entry.
pub(crate) fn upgrade_prompt_placeholders(p: &mut Prompt) {
    p.text = upgrade_placeholders(&p.text);
    for v in &mut p.variants {
        *v = upgrade_placeholders(v);
    }
}

pub(crate) fn migrate_prompts(prompts: &mut [Prompt]) {
    // `id` is the prompts table's PRIMARY KEY, so a duplicate makes the whole
    // DELETE+INSERT rewrite fail: and save_prompts then quietly writes the JSON
    // fallback that the database path ignores, so the import looks like it worked and
    // the old prompts are back after a restart. An import (or a hand-edited backup) is
    // the only way two prompts can share an id; the second one gets a fresh one.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for p in prompts.iter_mut() {
        if p.id.is_empty() || !seen.insert(p.id.clone()) {
            p.id = gen_id();
            seen.insert(p.id.clone());
        }
    }
    for p in prompts {
        if !p.image.is_empty() && !p.copy_image && (p.text.is_empty() || p.text == p.name) {
            p.copy_image = true;
        }
        upgrade_prompt_placeholders(p);
    }
}

// The one-time data upgrade for the stored history: every version row and every
// trash entry still holding the old placeholder spelling is rewritten, so an old
// snapshot restored later already reads right. Returns whether it all landed;
// the caller only records the new data version then, and a failed run simply
// repeats at the next start (the rewrite is idempotent).
pub(crate) fn upgrade_stored_placeholders(app: &AppHandle, store: &mut Store) -> bool {
    let Some(mut conn) = db_conn(app) else {
        return false;
    };
    if upgrade_history_placeholders(&mut conn, &mut store.trash).is_err() {
        return false;
    }
    // The prompts themselves were upgraded in memory by migrate_prompts; they are
    // written here together with the trash, so the database stops holding the old
    // spelling. A locked store (rows that failed to decrypt) is never written.
    if store.prompts_locked {
        return false;
    }
    save_prompts_and_trash(app, store)
}

// The version rows (in one transaction) and the in-memory trash entries through
// upgrade_placeholders; returns how many of them changed, so a second run is 0.
pub(crate) fn upgrade_history_placeholders(conn: &mut Connection, trash: &mut [Prompt]) -> rusqlite::Result<usize> {
    let mut changed = db_upgrade_version_rows(conn)?;
    for p in trash {
        let before = (p.text.clone(), p.variants.clone());
        upgrade_prompt_placeholders(p);
        if before != (p.text.clone(), p.variants.clone()) {
            changed += 1;
        }
    }
    Ok(changed)
}

// Every prompt_versions row (text and rotation entries) through upgrade_placeholders,
// in one transaction; a row that needs no change is not written.
pub(crate) fn db_upgrade_version_rows(conn: &mut Connection) -> rusqlite::Result<usize> {
    let tx = conn.transaction()?;
    let mut rewritten = 0usize;
    {
        let rows: Vec<(i64, String, String)> = {
            let mut stmt = tx.prepare("SELECT id, text, variants FROM prompt_versions")?;
            let mapped = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
            mapped.flatten().collect()
        };
        for (id, text_enc, variants_enc) in rows {
            let text = dec_str(&text_enc);
            let variants_raw = dec_str(&variants_enc);
            // dec_str answers "" for what it cannot read; a stored value that is not
            // empty but reads back empty is left alone, never overwritten with "".
            if (text.is_empty() && !text_enc.is_empty()) || (variants_raw.is_empty() && !variants_enc.is_empty()) {
                continue;
            }
            let variants = decode_variants(&variants_raw);
            let new_text = upgrade_placeholders(&text);
            let new_variants: Vec<String> = variants.iter().map(|v| upgrade_placeholders(v)).collect();
            if new_text == text && new_variants == variants {
                continue;
            }
            tx.execute(
                "UPDATE prompt_versions SET text = ?1, variants = ?2 WHERE id = ?3",
                params![enc_str(&new_text), enc_str(&encode_variants(&new_variants)), id],
            )?;
            rewritten += 1;
        }
    }
    tx.commit()?;
    Ok(rewritten)
}

pub(crate) fn load_store(app: &AppHandle) -> Store {
    let dir = data_dir(app);
    if let Some(mut conn) = db_conn(app) {
        // Filled from the rows as they are read, so the first save of the session
        // already only writes what changed (see WrittenRows).
        let mut written = WrittenRows::new();
        let (mut prompts, settings_opt, needs_mig, prompt_fails, settings_unreadable) =
            db_load(&conn, &mut written);
        let (mut trash, trash_fails) = db_load_trash(&conn, &mut written);
        let decrypt_fails = prompt_fails + trash_fails;
        // Some row (prompts, trash, or settings) could not be read back or decrypted →
        // preserve the raw DB and lock writes to whatever was affected (see
        // prompts_locked/SETTINGS_LOCKED below, which gate their respective writes
        // the same way).
        if decrypt_fails > 0 || settings_unreadable {
            let db_path = dir.join("data.db");
            let bak = dir.join("data.db.locked.bak");
            if !bak.exists() {
                let _ = fs::copy(&db_path, &bak);
            }
            append_crash_log(&format!(
                "{} row(s) failed to load reliably (query error, unreadable row, or decrypt failure){}; \
                 writes locked, raw DB copied to data.db.locked.bak",
                decrypt_fails,
                if settings_unreadable { " (settings row included)" } else { "" }
            ));
        }
        SETTINGS_LOCKED.store(settings_unreadable, Ordering::Relaxed);
        let mut settings = settings_opt.clone().unwrap_or_default();
        // True while `settings` is a blank default rather than something read from disk.
        let mut settings_defaulted = settings_opt.is_none();
        // Empty DB but legacy JSON present → import it once, then own the data.
        // A corrupt-but-present settings row must never be mistaken for "nothing
        // here yet" -- that would silently paper over the SETTINGS_LOCKED case set
        // above with whatever an unrelated legacy JSON file happens to contain.
        if prompts.is_empty() && settings_opt.is_none() && !settings_unreadable {
            let j_settings_r: SealedRead<Settings> = read_sealed_json(&dir.join("settings.json"));
            let j_prompts_r: SealedRead<Vec<Prompt>> = read_sealed_json(&dir.join("prompts.json"));
            // An unreadable legacy file (wrong Windows account, corruption) must never be
            // treated as "nothing to import": that would both skip real data AND, below,
            // still be eligible for the empty-DB path to run again forever, or worse, look
            // imported. Skip the whole import this launch and leave the file for inspection.
            if matches!(j_settings_r, SealedRead::Unreadable) || matches!(j_prompts_r, SealedRead::Unreadable) {
                append_crash_log(
                    "legacy prompts.json/settings.json present but could not be decrypted/parsed; \
                     import skipped this launch, files kept",
                );
            } else {
                let j_prompts = match j_prompts_r {
                    SealedRead::Loaded(v) => v,
                    _ => Vec::new(),
                };
                let j_settings = match j_settings_r {
                    SealedRead::Loaded(v) => {
                        settings_defaulted = false;
                        v
                    }
                    _ => Settings::default(),
                };
                if !j_prompts.is_empty() || dir.join("settings.json").exists() {
                    let prompts_ok = db_conn(app)
                        .map(|mut c2| db_write_prompts(&mut c2, &j_prompts, &mut written).is_ok())
                        .unwrap_or(false);
                    let settings_ok = db_write_settings(&conn, &j_settings).is_ok();
                    // Once the data is safely in the DB, drop the legacy JSON files so
                    // the import never runs again and stale copies cannot diverge.
                    if prompts_ok && settings_ok {
                        let _ = fs::remove_file(dir.join("prompts.json"));
                        let _ = fs::remove_file(dir.join("settings.json"));
                    }
                }
                prompts = j_prompts;
                settings = j_settings;
            }
        } else {
            // prompts.json/settings.json otherwise only exist as a save_prompts/
            // save_settings fallback from a failed DB write earlier this session (they
            // are deleted the moment the DB accepts the data again): their presence
            // here means the DB never received that change, so the file, not the DB,
            // is the truth. Retry the write; keep the file (and the loaded data) if the
            // DB still refuses, so this keeps retrying every launch instead of quietly
            // losing the edit.
            // Rows that failed to decrypt already lock the store below (prompts_locked);
            // do not also retry the fallback write here, or a partially-successful DB
            // write would fight with that lock over what the DB is supposed to contain.
            if decrypt_fails == 0 {
                let pj = dir.join("prompts.json");
                if pj.exists() {
                    match read_sealed_json::<Vec<Prompt>>(&pj) {
                        SealedRead::Loaded(fallback) => {
                            prompts = fallback;
                            if db_write_prompts(&mut conn, &prompts, &mut written).is_ok() {
                                let _ = fs::remove_file(&pj);
                            }
                        }
                        SealedRead::Unreadable => append_crash_log(
                            "prompts.json fallback present but could not be decrypted/parsed; \
                             DB left untouched, file kept for inspection",
                        ),
                        SealedRead::Missing => {}
                    }
                }
                let sj = dir.join("settings.json");
                if sj.exists() {
                    match read_sealed_json::<Settings>(&sj) {
                        SealedRead::Loaded(s) => {
                            settings = s;
                            settings_defaulted = false;
                            if db_write_settings(&conn, &settings).is_ok() {
                                let _ = fs::remove_file(&sj);
                            }
                        }
                        SealedRead::Unreadable => append_crash_log(
                            "settings.json fallback present but could not be decrypted/parsed; \
                             DB left untouched, file kept for inspection",
                        ),
                        SealedRead::Missing => {}
                    }
                }
            }
        }
        // Force a resave when the "ai" theme is about to be rewritten to
        // "neon": otherwise the fix only lives in memory until some unrelated
        // setting change happens to persist it.
        let theme_ai_before =
            settings.theme == "ai" || settings.custom_themes.iter().any(|c| c.base == "ai");
        let pruned_removed_values = settings.has_removed_ui_values();
        // An unreadable settings row, or a missing one next to existing data, belongs
        // to an existing install: it must never start pruning under the new defaults.
        if settings_unreadable
            || (settings_defaulted && (!prompts.is_empty() || copy_log_has_rows(&conn)))
        {
            settings.retention_defaults_v2 = false;
        }
        let pin_retention = !settings.retention_defaults_v2;
        settings.migrate();
        load_pick_history(&settings); // restore the colour-pick history mirror
        // One-time: move any legacy in-blob copy history into the copy_log table.
        // Gated on a persistent `meta` marker (written in the same transaction as
        // the rows) so a failed post-migration resave can never re-import and
        // duplicate the history on the next launch.
        let already_migrated = conn
            .query_row("SELECT 1 FROM meta WHERE key='copy_log_migrated'", [], |_| Ok(()))
            .is_ok();
        let mut migrated_log = false;
        if !already_migrated {
            if !settings.copy_log.is_empty() {
                if let Ok(tx) = conn.transaction() {
                    {
                        // copy_log is newest-first: insert oldest-first so rowid order
                        // stays chronological; backfill last_used with each id's newest ts.
                        if let Ok(mut stmt) = tx.prepare("INSERT INTO copy_log(ts, id) VALUES(?1, ?2)") {
                            for e in settings.copy_log.iter().rev() {
                                let _ = stmt.execute(params![e.ts as i64, e.id]);
                                if e.ts > 0 {
                                    settings.last_used.insert(e.id.clone(), e.ts);
                                }
                            }
                        }
                        let _ = tx.execute(
                            "INSERT OR REPLACE INTO meta(key, value) VALUES('copy_log_migrated', '1')",
                            [],
                        );
                    }
                    if tx.commit().is_ok() {
                        migrated_log = true;
                    }
                }
            } else {
                // No legacy history to move: mark done so we never re-check.
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO meta(key, value) VALUES('copy_log_migrated', '1')",
                    [],
                );
            }
            settings.copy_log.clear();
        }
        // Enforce the cap + retention window on the table at startup (e.g. after
        // the cap was lowered, or right after a large legacy import).
        let (cap, cutoff) = history_cap_and_cutoff(&settings);
        let _ = db_trim_copy(&conn, cap, cutoff);
        migrate_prompts(&mut prompts);
        // A relative file_path (only ever produced by demo_prompts(), never by the
        // real attachment picker, which always yields an absolute path) is resolved
        // against the live data dir here instead of being stored absolute on disk --
        // see the comment on demo_prompts()'s attachment row and on
        // resolve_relative_file_paths itself.
        resolve_relative_file_paths(&dir, &mut prompts);
        resolve_relative_file_paths(&dir, &mut trash);
        let mut store = Store {
            prompts,
            trash,
            settings,
            prompts_locked: decrypt_fails > 0,
            written: std::cell::RefCell::new(written),
        };
        acquire_data_dir_lock(&dir, data_dir_lock_enabled(&store.settings));
        // Never let an existing install see the first-run tour -- a store
        // this is discovered for is, by definition, not the one that will ever
        // write the flag itself via the tour's own Finish/Skip.
        if tour_should_be_marked_seen(!store.prompts.is_empty(), store.settings.examples_seeded) {
            store.settings.ui_flags.entry("tourSeen".to_string()).or_insert(true);
        }
        // A purge whose version-history DELETE failed is never retried by the purge
        // itself, so the text would stay in prompt_versions for good. Only when no
        // JSON fallback is waiting: then the tables really are the whole library.
        if version_sweep_allowed(&dir, DATA_DIR_READONLY.load(Ordering::Relaxed)) {
            db_sweep_orphan_versions(&conn);
        }
        // Seal every plain edit_state row an earlier version left behind, off the
        // main thread; idempotent, so running it on each load costs one cheap scan.
        {
            let app = app.clone();
            std::thread::spawn(move || {
                if let Some(conn) = db_conn(&app) {
                    edit_state_seal_all(&conn);
                }
            });
        }
        store.normalize_folders(); // Drop members whose prompt is gone
        sweep_trash(app, &mut store); // drop anything past the retention window
        // Data upgrades that rewrite stored content, once per step. Each later step
        // adds its own `data_version < N` block under this one.
        let mut data_upgraded = false;
        let may_upgrade = !DATA_DIR_READONLY.load(Ordering::Relaxed);
        if may_upgrade && store.settings.data_version < 1 && upgrade_stored_placeholders(app, &mut store) {
            store.settings.data_version = DATA_VERSION;
            data_upgraded = true;
        }
        if needs_mig || migrated_log {
            // One-time: re-encrypt legacy plaintext rows / drop the migrated log from the blob.
            save_prompts(app, &store);
            save_settings(app, &store.settings);
        } else if theme_ai_before || pruned_removed_values || pin_retention || data_upgraded {
            save_settings(app, &store.settings);
        }
        return store;
    }
    // Fallback: DB unavailable → read the JSON files directly.
    let mut store = load_store_from_json(&dir);
    acquire_data_dir_lock(&dir, data_dir_lock_enabled(&store.settings));
    if tour_should_be_marked_seen(!store.prompts.is_empty(), store.settings.examples_seeded) {
        store.settings.ui_flags.entry("tourSeen".to_string()).or_insert(true);
    }
    store.normalize_folders();
    sweep_trash(app, &mut store); // no-op here (fallback path never loads trash), kept for symmetry
    store
}

// The `dataDirLock` toggle (default on) -- same "absent/true = on, explicit
// false = off" convention as colorPicker and the other default-on flags.
pub(crate) fn data_dir_lock_enabled(settings: &Settings) -> bool {
    settings.ui_flags.get("dataDirLock") != Some(&false)
}

// What a launch can still recover when the database cannot be opened at all.
// Split out of load_store so the write-lock decision below can be exercised
// without an AppHandle around it: everything here depends only on the folder.
// There is no second source to fall back to on this path, so an unreadable file
// must lock writes (like load_store's decrypt_fails case) rather than silently
// present (and then save) an empty store.
pub(crate) fn load_store_from_json(dir: &Path) -> Store {
    let mut unreadable = false;
    let mut settings_missing = false;
    let mut settings = match read_sealed_json::<Settings>(&dir.join("settings.json")) {
        SealedRead::Loaded(s) => s,
        SealedRead::Unreadable => {
            unreadable = true;
            Settings { retention_defaults_v2: false, ..Settings::default() }
        }
        SealedRead::Missing => {
            settings_missing = true;
            Settings::default()
        }
    };
    settings.migrate();
    load_pick_history(&settings); // restore the colour-pick history mirror
    prune_history(&mut settings);
    let mut prompts_missing = false;
    let mut prompts = match read_sealed_json::<Vec<Prompt>>(&dir.join("prompts.json")) {
        SealedRead::Loaded(p) => p,
        SealedRead::Unreadable => {
            unreadable = true;
            Vec::new()
        }
        SealedRead::Missing => {
            prompts_missing = true;
            Vec::new()
        }
    };
    migrate_prompts(&mut prompts);
    // Missing settings next to existing prompts is an existing install, not a new one.
    if settings_missing && !prompts.is_empty() {
        settings.retention_defaults_v2 = false;
        settings.migrate();
    }
    // A previous session already wrote data.db, so its rows still exist on disk:
    // db_conn just could not open it this time (e.g. a transient ALTER failure).
    // Without prompts.json either, this launch has literally no way to see the real
    // prompts; presenting an empty-but-unlocked store would let the very next save
    // (or a stray auto-save) write that emptiness over the table: and to
    // prompts.json, which load_store's own "101" rule then trusts over the DB on the
    // next launch, wiping it for good the moment data.db is reachable again.
    // What settings.json happens to be doing is irrelevant here: it says nothing
    // about the prompts, and a session whose settings write once fell back to the
    // file leaves exactly that combination behind: settings.json present,
    // prompts.json not.
    if !unreadable && prompts_missing && dir.join("data.db").exists() {
        unreadable = true;
    }
    if unreadable {
        append_crash_log(
            "DB unavailable and prompts.json/settings.json fallback could not be decrypted/parsed; \
             writes locked to avoid saving an empty store over real data",
        );
    }
    // No usable database was read here, so nothing is known to be on disk: the map
    // starts empty and the next successful save writes every row.
    Store {
        prompts,
        trash: Vec::new(),
        settings,
        prompts_locked: unreadable,
        written: std::cell::RefCell::new(WrittenRows::new()),
    }
}

pub(crate) fn gen_id() -> String {
    // nanos give cross-restart uniqueness; the per-process counter prevents
    // collisions when many ids are minted in the same tick (e.g. bulk import).
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("p{}_{}", nanos, seq)
}

pub(crate) fn lock<'a>(state: &'a State<'a, Db>) -> std::sync::MutexGuard<'a, Store> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}
