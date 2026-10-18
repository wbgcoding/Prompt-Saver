use super::*;
use std::time::Instant;

// SQLite store: one row per prompt (ordered), settings as a single JSON row. The legacy
// JSON files are imported once on upgrade and used only if the DB cannot be opened.

// At-rest encryption (DPAPI, per user): only the same Windows account can decrypt. Falls
// back to plaintext if DPAPI is unavailable so data is never lost; legacy plaintext rows
// are re-encrypted on the next save.
pub(crate) const ENC_PREFIX: &str = "enc:v1:";
// A NUL byte never occurs in typed or pasted text, so it escapes plaintext that itself
// starts with ENC_PREFIX (or this marker). dec_str can then tell it from real ciphertext
// instead of erasing it as corrupt.
pub(crate) const PLAINTEXT_ESCAPE: &str = "\u{0}";

// Whether every write this run went out sealed. Flipped once, by the plaintext fallback in
// enc_str, and never flipped back: a row written in plaintext stays that way until its
// content is rewritten, so clearing the flag on a later success would hide it. The window
// reads it through store_sealed and listens for store-unsealed.
pub(crate) static STORE_SEALED: AtomicBool = AtomicBool::new(true);
// Set once the app handle exists. A closure rather than the AppHandle: enc_str is linked
// into the unit-test binary, and a direct emit would pull the webview event path into it,
// which then fails to load. The emit lives in the closure built in run().
pub(crate) type SealNotifier = Arc<dyn Fn() + Send + Sync>;
pub(crate) static SEAL_NOTIFY: Mutex<Option<SealNotifier>> = Mutex::new(None);

// Encrypt a JSON string for storage (prefix + base64 ciphertext). A DPAPI failure falls
// back to plaintext rather than refusing the write, which would mean data loss. It is
// logged and announced once per run, so a broken profile never downgrades the store
// silently.
// Test-only: forces the plaintext fallback. Thread-local because tests run on separate
// threads and a global would push other tests into the fallback too.
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
            // Cloned out of the lock so the notification does not hold the mutex across a webview
            // round trip.
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

// True while everything on disk is sealed; false once a write fell back to plaintext this
// run.
#[tauri::command]
pub(crate) fn store_sealed() -> bool {
    STORE_SEALED.load(Ordering::Relaxed)
}

// Copy of settings.minimize_to_tray for the window-close handler, which runs on the
// event-loop thread and must not wait for the store lock. Refreshed on every settings load
// and write.
pub(crate) static MINIMIZE_TO_TRAY: AtomicBool = AtomicBool::new(true);

pub(crate) fn mirror_minimize_to_tray(settings: &Settings) {
    MINIMIZE_TO_TRAY.store(settings.minimize_to_tray, Ordering::Relaxed);
}

// Set at load_store() when the settings row was present but unreadable (query error,
// decrypt failure, corrupt JSON); a fresh install does not set it. While true,
// write_settings_outcome refuses to rewrite the row, since the in-memory defaults would
// overwrite it. Process-global because every settings write funnels through functions
// that have no &Store.
pub(crate) static SETTINGS_LOCKED: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub(crate) fn settings_locked() -> bool {
    SETTINGS_LOCKED.load(Ordering::Relaxed)
}

// Whether the last settings write reached disk: set when both the database and the JSON
// fallback fail, cleared when a write succeeds. Not sticky like STORE_SEALED. The window
// reads it at startup and gets live changes through the persist-failed event.
pub(crate) static SETTINGS_WRITE_FAILED: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub(crate) fn settings_write_failed() -> bool {
    SETTINGS_WRITE_FAILED.load(Ordering::Relaxed)
}

// Data folder lock. A data folder in a synced folder (OneDrive, Dropbox, ...) risks a
// corrupted SQLite file if two machines write at once. The single-instance plugin already
// covers a second process on this machine; this covers another machine.
//
// A small lock file (host, pid, last-seen time) beside the database is refreshed every
// DATA_DIR_LOCK_REFRESH_SECS by the scheduler. A machine that finds another host's lock
// fresh opens the store read-only for the session and says so in the window. A stale lock
// is taken over without asking, so a crash on one machine cannot lock out the others.
// Process-global like SETTINGS_LOCKED, since all writes funnel through the same save
// functions.
pub(crate) const DATA_DIR_LOCK_FILE: &str = "datadir.lock.json";
pub(crate) const DATA_DIR_LOCK_REFRESH_SECS: u64 = 30;
// Three missed refreshes: one slow tick is never mistaken for a crash, and a real crash
// does not lock the other machine out for long.
pub(crate) const DATA_DIR_LOCK_STALE_SECS: u64 = DATA_DIR_LOCK_REFRESH_SECS * 3;

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct DataDirLockInfo {
    pub(crate) host: String,
    pub(crate) pid: u32,
    // Writer's wall clock. Never trusted for staleness (clock skew); it only makes the content
    // change on each refresh.
    pub(crate) seen: u64,
    // Names one claim of the folder so a process can tell its own lock from one written over
    // it. Empty in locks from older versions.
    #[serde(default)]
    pub(crate) id: String,
    // Counts the refreshes of this claim.
    #[serde(default)]
    pub(crate) beat: u64,
}

// COMPUTERNAME is always set in a logged-in session; the fallback label just keeps the
// read-only banner readable.
pub(crate) fn local_host_name() -> String {
    std::env::var("COMPUTERNAME").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "this computer".to_string())
}

pub(crate) fn data_dir_lock_path(dir: &Path) -> PathBuf {
    dir.join(DATA_DIR_LOCK_FILE)
}

pub(crate) fn read_data_dir_lock(path: &Path) -> Option<DataDirLockInfo> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

// Never written in place, since another process reads the file while it is refreshed. A
// refused swap keeps the old lock and the next tick retries.
pub(crate) fn write_data_dir_lock(path: &Path, info: &DataDirLockInfo) -> bool {
    serde_json::to_string(info).is_ok_and(|json| write_bytes_atomic_strict(path, json.as_bytes()).is_ok())
}

pub(crate) static DATA_DIR_READONLY: AtomicBool = AtomicBool::new(false);
pub(crate) static DATA_DIR_LOCK_HOST: Mutex<Option<String>> = Mutex::new(None);

// Set by enable_demo_mode()/disable_demo_mode() after their flush and before the datapath
// redirect and restart. From then on nothing may write: db_conn()/data_dir() already
// resolve to the other store, so a write (the exit flush, a settings tick, an in-flight
// command) would put real data in the demo store or the reverse. Reset on a failed flip;
// on success the restarted process starts with false.
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

// Judges a foreign lock dead from what this process saw on its own monotonic clock, never
// from the other machine's timestamp: dead only when its content has not changed for
// DATA_DIR_LOCK_STALE_SECS. A live owner rewrites it every refresh.
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

// The claim itself, free of statics so tests can race it. With no lock file the claim uses
// create_new, which only one starter wins. Any other failure is not ownership: the lock is
// judged like a foreign one, and an unreadable folder counts as busy. A takeover happens
// only after the stale limit, then the file is read back and the claim holds only if it
// carries our id.
pub(crate) fn claim_data_dir_lock(path: &Path, me: &LockMe, obs: &mut LockObserver, now: Instant, settle: Duration) -> LockOutcome {
    use std::io::Write;
    let unknown = || LockOutcome::ReadOnly("unknown".to_string());
    let info = DataDirLockInfo { host: me.host.to_string(), pid: me.pid, seen: now_secs(), id: me.id.to_string(), beat: 0 };
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
            // A lock naming this machine is never foreign, whatever its age (crash and restart on one
            // computer).
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

// One refresh of an owned lock: read first, rewrite only our own claim.
pub(crate) fn refresh_owned_lock(path: &Path, me: &LockMe, beat: u64) -> RefreshOutcome {
    let fresh = DataDirLockInfo { host: me.host.to_string(), pid: me.pid, seen: now_secs(), id: me.id.to_string(), beat };
    match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<DataDirLockInfo>(&text) {
            Ok(e) if e.id == me.id => {
                write_data_dir_lock(path, &fresh);
                RefreshOutcome::Kept
            }
            Ok(e) => RefreshOutcome::Lost(e.host),
            Err(_) => RefreshOutcome::Skipped,
        },
        // A vanished lock is recreated like a first claim, so a claim another machine made
        // meanwhile wins and the next refresh reports the loss.
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

// Decides read-only vs. owned for dir and claims it if owned. Called at load_store() and
// after set_data_dir(). The dataDirLock toggle (default on) skips all of this when off. A
// foreign lock not yet watched for the whole stale limit keeps the store read-only.
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

// Scheduler tick. An owner reads the lock first and gives up the folder when it names
// another claim (after a long sleep or clock skew), never erasing the taker's lock. A
// read-only process never writes while the owner is alive and takes over once the lock has
// stood still for the stale limit.
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

// Polled once at startup to fill the persistent read-only banner before the first render.
#[tauri::command]
pub(crate) fn data_dir_lock_status() -> DataDirLockStatus {
    DataDirLockStatus {
        read_only: DATA_DIR_READONLY.load(Ordering::Relaxed),
        other_host: DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    }
}

// The single choke point for save_prompts, save_prompts_and_trash, save_trash and
// write_settings_outcome: a real refusal that emits the same event with the same host name,
// whichever action triggered it.
pub(crate) fn data_dir_write_blocked(app: &AppHandle) -> bool {
    // No event here: the window is already on its way down for app.restart().
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

// Known synchronised-folder conventions: the environment variable a client publishes (more
// reliable than a folder name a tenant can rename), then the folder name. Returns the
// provider's display name, or None for a local folder.
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

// Used when picking a new data folder (before set_data_dir commits) and at startup against
// the folder in use.
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
    dec_str_strict(stored).unwrap_or_default()
}

// dec_str that tells "could not be read" (None) from "reads as empty" (Some("")),
// for the callers that must never act on a value they could not read.
pub(crate) fn dec_str_strict(stored: &str) -> Option<String> {
    if let Some(rest) = stored.strip_prefix(PLAINTEXT_ESCAPE) {
        return Some(rest.to_string());
    }
    #[cfg(windows)]
    if let Some(b64) = stored.strip_prefix(ENC_PREFIX) {
        let ct = base64_decode(b64);
        if !ct.is_empty() {
            if let Some(pt) = store_crypt::unprotect(&ct) {
                if let Ok(s) = String::from_utf8(pt) {
                    return Some(s);
                }
            }
        }
        return None;
    }
    Some(stored.to_string())
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
    // prompt_versions predates the variants column. A new install gets it from CREATE TABLE;
    // an existing data.db needs the ALTER. Checked via PRAGMA table_info so a real failure
    // (locked, full, corrupt disk) is not swallowed as "already migrated".
    let has_variants_column = conn
        .prepare("SELECT 1 FROM pragma_table_info('prompt_versions') WHERE name = 'variants'")
        .and_then(|mut s| s.exists([]))
        .unwrap_or(true); // a failing PRAGMA means the connection is already broken; skip the ALTER
    if !has_variants_column {
        // A failed ALTER fails db_conn itself, which every caller treats as "DB unavailable".
        if conn.execute("ALTER TABLE prompt_versions ADD COLUMN variants TEXT NOT NULL DEFAULT ''", []).is_err() {
            return false;
        }
    }
    true
}

pub(crate) fn db_conn(app: &AppHandle) -> Option<Connection> {
    // The choke point every writer goes through, including those that never call
    // data_dir_write_blocked() (save_editor_draft, save_edit_state, the copy log, version
    // snapshots). Checking here makes the demo-transition lock block all of them. None reads as
    // "DB unavailable", which callers already handle.
    if DEMO_TRANSITION_LOCKED.load(Ordering::Relaxed) {
        return None;
    }
    db_conn_at(&data_dir(app))
}

// Split out of db_conn() so the command line's headless path (no AppHandle) opens the same
// connection with the same pragmas and schema init.
pub(crate) fn db_conn_at(dir: &Path) -> Option<Connection> {
    let conn = Connection::open(dir.join("data.db")).ok()?;
    // busy_timeout is per connection (not persisted in the file), so it is set every time. The
    // schema batch and migration probe are idempotent and run once per process.
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

// What the prompts/trash tables are known to hold: "<table>\u{1}<id>" maps to a hash of
// that row's plaintext JSON. Sealing the same text twice gives different bytes, and
// decrypting the table to compare would cost what this map saves, so a save skips
// re-sealing untouched rows by hash.
//
// Anything that changes those tables outside write_rows (a restored or swapped data.db, a
// raw DELETE, a move to another folder) must call Store::forget_written_rows. That keeps
// the keys, which record that this process knew the row and may delete it later, and
// voids the hashes.
pub(crate) type WrittenRows = HashMap<String, u64>;

// The hash every invalidated entry carries. row_hash never returns it, so such an
// entry matches no content and the row is rewritten by the next save.
pub(crate) const STALE_ROW_HASH: u64 = 0;

pub(crate) fn row_key(table: &str, id: &str) -> String {
    // \u{1} cannot occur in a table name, so the two halves can never run together.
    format!("{table}\u{1}{id}")
}

pub(crate) fn row_hash(json: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    json.hash(&mut h);
    // 0 is reserved for an invalidated entry (STALE_ROW_HASH).
    h.finish().max(1)
}

// Two prompts sharing one id can only be a caller bug; writing them would silently
// collapse into one row. The whole write is rejected.
pub(crate) fn duplicate_row_id_error(table: &str) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY),
        Some(format!("duplicate id in {table}")),
    )
}

// Bring one table in line with rows, touching as little as possible: deleted rows are
// removed, changed rows re-sealed and rewritten, moved rows only get their ord corrected.
// Runs inside the caller's transaction. `table` is always one of our two hardcoded names,
// so interpolating it is safe.
//
// Returns the map entries for the rows as they now stand; the caller adopts them only
// after COMMIT succeeds.
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
    // Ids and positions only, nothing decrypted. Row presence comes from here, not the map, so
    // a row missing from the table is re-inserted even if the map lists it.
    let mut present: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = tx.prepare(&format!("SELECT id, ord FROM {table}"))?;
        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            present.insert(r.get(0)?, r.get(1)?);
        }
    }
    {
        // A row missing from keep is deleted only if this process already knew it (present in
        // written). With allowMultipleInstances another process can commit a new row between this
        // process's load and save; deleting on absence alone would destroy that unseen row.
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

// Lists every row currently in `table` as known to this process (stale hash), so
// the next write_rows deletes the ones the new set leaves out. Only for a write
// that replaces the table wholesale: write_rows normally spares rows it never saw.
pub(crate) fn claim_present_rows(conn: &Connection, table: &str, written: &mut WrittenRows) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(&format!("SELECT id FROM {table}"))?;
    let ids: Vec<String> = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    for id in ids {
        written.entry(row_key(table, &id)).or_insert(STALE_ROW_HASH);
    }
    Ok(())
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

// Same schema and shape as the prompt half, in its own table. A trashed prompt is simply
// absent from prompts, so every existing read of store.prompts stays correct without a
// deleted_at filter at every call site.
pub(crate) fn db_write_trash(conn: &mut Connection, trash: &[Prompt], written: &mut WrittenRows) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    let fresh = write_rows(&tx, "trash", trash, written)?;
    tx.commit()?;
    adopt_written_rows(written, "trash", fresh);
    Ok(())
}

// delete_prompt/restore_prompt move a row between the two tables. One transaction for both
// halves means the move lands whole or not at all, instead of stranding the prompt in
// neither table.
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

// An import that replaces both halves: prompt rows and the settings row land in one
// transaction, so a failure never leaves one half imported.
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

// written collects what the table was found to hold (see WrittenRows), recorded straight
// from the rows as read, so the first save only writes what changed.
//
// The second return value is a failure count like db_load's: a row that could not be read,
// decrypted or parsed is present but unaccounted for, and the caller must fold it into the
// write-lock decision so a trash mutation cannot erase it.
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
                                    // An unsealed row gets the stale hash so the next write seals it; the key stays known so a
                                    // purge still deletes it.
                                    let hash = if encrypted {
                                        row_hash(&serde_json::to_string(&p).unwrap_or_default())
                                    } else {
                                        STALE_ROW_HASH
                                    };
                                    written.insert(row_key("trash", &p.id), hash);
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

// Returns (prompts, settings, any_plaintext, load_failures). load_failures counts rows that
// are present but unaccounted for: an encrypted row that would not decrypt or parse, an
// unreadable row, or the SELECT itself failing. The true prompt count is then unknown, so
// the caller must avoid any destructive rewrite of the table.
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
                                    // A legacy plaintext row is left out of the map so it counts as changed and is sealed on
                                    // the next save.
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
    // A genuinely absent row (fresh install) is not a failure; only a row that is there but
    // unreadable counts (see settings_locked).
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

// Peeks allowMultipleInstances before the Tauri builder exists, since the single-instance
// plugin is attached at builder time. Mirrors default_data_dir()'s portable/APPDATA
// resolution and opens the DB directly. Any failure yields false (single instance).
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

// Trim the history to the cap (newest cap rows by rowid, which is insertion order) and the
// retention window. The kept block is rowid in (MAX-cap, MAX]; age pruning only deletes
// from the old end, so the block stays contiguous and the cap is never exceeded. Steady
// state deletes at most one row per call.
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

// Append one copy event, then trim to the cap and the retention window.
pub(crate) fn db_append_copy(conn: &Connection, entry: &CopyEntry, cap: usize, cutoff: Option<u64>) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO copy_log(ts, id) VALUES(?1, ?2)", params![entry.ts as i64, entry.id])?;
    db_trim_copy(conn, cap, cutoff)
}

// One journal row as shown in the UI. Unlike CopyEntry it carries the rowid, which a
// delete needs to target the exact row in grouped mode.
#[derive(Serialize)]
pub(crate) struct JournalEntry {
    pub(crate) rowid: i64,
    pub(crate) id: String,
    pub(crate) ts: u64,
}

// Copy counts for the last `days` UTC days, oldest first, zero-filled: one GROUP BY by
// day-since-epoch merged against a dense day range in Rust.
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
        // The whole newest row per prompt, not MAX(rowid) beside a separately computed MAX(ts),
        // which could pair a delete target with another copy's timestamp.
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

// Prompt version history: snapshots of a prompt's previous name/text, encrypted like the
// prompt rows, in data.db so history never bloats settings.
#[derive(Serialize)]
pub(crate) struct PromptVersion {
    pub(crate) id: i64,
    pub(crate) ts: u64,
    pub(crate) name: String,
    pub(crate) text: String,
    // Entries are content: without them a restore would drop entries 2..n. JSON-encoded in its
    // own column.
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
// A prompt's last real content edit is the newest ts in prompt_versions (only name or text
// changes get a version row). One GROUP BY for all prompts; a prompt absent from the
// result was never edited with history on, or never edited.
pub(crate) fn db_last_edited(conn: &Connection) -> std::collections::HashMap<String, u64> {
    let mut out = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT prompt_id, MAX(ts) FROM prompt_versions GROUP BY prompt_id") {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))) {
            out.extend(rows.flatten());
        }
    }
    out
}
// None when the row is missing or any of its sealed columns cannot be read (the
// store was written under another Windows account): handing back empty fields
// would let a restore overwrite a live prompt with nothing.
pub(crate) fn db_get_version(conn: &Connection, id: i64) -> Option<(String, String, String, Vec<String>)> {
    let (prompt_id, name, text, variants): (String, String, String, String) = conn
        .query_row(
            "SELECT prompt_id, name, text, variants FROM prompt_versions WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .ok()?;
    let name = dec_str_strict(&name)?;
    let text = dec_str_strict(&text)?;
    let variants = dec_str_strict(&variants)?;
    Some((
        prompt_id,
        name,
        upgrade_placeholders(&text),
        decode_variants(&variants).iter().map(|v| upgrade_placeholders(v)).collect(),
    ))
}
// The orphan sweep deletes rows: never in a read-only instance (the store belongs to
// another machine) and never while a JSON fallback still holds prompts.
pub(crate) fn version_sweep_allowed(dir: &Path, read_only: bool) -> bool {
    !read_only && !dir.join("prompts.json").exists()
}

// Removes version history whose prompt is in neither the library nor the trash. Works on
// the plaintext id column alone, so it is safe even when the rows could not be decrypted.
// Returns the number of rows removed.
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
    // Called from a permanent purge: a failure silently leaving the text behind in
    // prompt_versions is a privacy gap, not clutter.
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

// Fingerprint of a seeded example prompt's editable content, so "remove examples" can tell
// whether the user made it their own. Mirrors SlimPrompt's image_rev instead of a stored
// field.
pub(crate) fn example_seed_fingerprint(name: &str, text: &str, image: &str) -> u64 {
    let image_rev = if image.is_empty() { 0 } else { text_hash(image) };
    text_hash(&format!("{name}\u{0}{text}\u{0}{image_rev}"))
}
pub(crate) fn db_add_clip(conn: &Connection, ts: u64, text: &str, cap: usize) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO clip_inbox(ts, text) VALUES(?1, ?2)", params![ts as i64, enc_str(text)])?;
    db_trim_clips(conn, cap)
}
// Keeps the newest `cap` rows. Counts rows, not id distance: ids have gaps after a
// single dismiss, and an undo can bring older ids back.
pub(crate) fn db_trim_clips(conn: &Connection, cap: usize) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM clip_inbox WHERE id NOT IN (SELECT id FROM clip_inbox ORDER BY id DESC LIMIT ?1)",
        params![cap as i64],
    )?;
    Ok(())
}
// A restored row goes back under its old id when that id is free, so it still sorts
// older than anything stored since; otherwise it is a plain new row.
pub(crate) fn db_insert_clip_at(conn: &Connection, id: Option<i64>, ts: u64, text: &str) -> rusqlite::Result<()> {
    if let Some(id) = id {
        let taken: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM clip_inbox WHERE id = ?1)", params![id], |r| r.get(0))?;
        if !taken {
            conn.execute("INSERT INTO clip_inbox(id, ts, text) VALUES(?1, ?2, ?3)", params![id, ts as i64, enc_str(text)])?;
            return Ok(());
        }
    }
    conn.execute("INSERT INTO clip_inbox(ts, text) VALUES(?1, ?2)", params![ts as i64, enc_str(text)])?;
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

// load_store()'s half of the relative to absolute file_path round trip (the other half is
// strip_data_dir_prefix), run over both prompts and trash and unit-testable without an
// AppHandle.
pub(crate) fn resolve_relative_file_paths(dir: &Path, rows: &mut [Prompt]) {
    for p in rows.iter_mut() {
        if !p.file_path.is_empty() && Path::new(&p.file_path).is_relative() {
            p.file_path = dir.join(&p.file_path).to_string_lossy().to_string();
        }
    }
}

// True when dir is exactly where demo mode's redirect points: an exact path comparison,
// never a folder-name guess.
pub(crate) fn data_dir_is_demo(app: &AppHandle, dir: &Path) -> bool {
    dir == default_data_dir(app).join(DEMO_SUBFOLDER)
}

// The other half of load_store()'s relative to absolute file_path resolution, reversed
// right before anything is persisted or exported, so a demo prompt's attachment is never
// saved as an absolute path containing the Windows account name. It only touches a path
// whose parent is exactly the live data dir. Every caller of db_write_prompts,
// db_write_prompts_and_trash, sync_write_all_impl and export_prompts must run its prompts
// through this first.
// Only in demo mode (is_demo, an exact path check made by the caller): in portable mode the
// data dir is the exe folder and custom data dirs are common, so a real attachment picked
// from directly inside it would otherwise be stripped to a bare filename that breaks after
// a data-dir move.
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
    // Safety valve: if some rows failed to decrypt at load, the in-memory list is a subset of
    // what is on disk, and rewriting the table would delete the unreadable rows. Prompt
    // changes are not persisted until that is resolved.
    if store.prompts_locked {
        return false;
    }
    let dir = data_dir(app);
    let prompts = strip_data_dir_prefix(&dir, data_dir_is_demo(app, &dir), &store.prompts);
    if let Some(mut conn) = db_conn(app) {
        // The map only advances on a committed write, so a failure leaves it describing what the
        // rolled-back table holds; the JSON fallback below must not touch it either.
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
    // Last resort: the JSON fallback, which load_store() reads back and retries into the DB on
    // the next launch. If even this fails the change is lost, so leave a trace in the log.
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

// The atomic counterpart to save_prompts + save_trash for delete_prompt/restore_prompt
// (see db_write_prompts_and_trash).
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

// Same shape as save_prompts (database first, JSON fallback second) and the choke point for
// every settings mutation. Returning bool plus the persist-failed event lets the frontend
// listener cover all callers.
pub(crate) fn save_settings(app: &AppHandle, settings: &Settings) -> bool {
    let _writing = settings_write_lock();
    let outcome = write_settings_outcome(app, settings);
    SETTINGS_DELAY.settle_immediate(outcome);
    outcome == SettingsWrite::Written
}

// A settings write that accompanies a prompt/trash write. A locked settings row (present but
// unreadable) refuses every settings write by design and has its own notice, so it counts as
// written here: rolling the prompt side back would block adding and deleting prompts for the whole
// session. Only a real write failure returns false and makes the caller undo the pair.
pub(crate) fn save_settings_paired(app: &AppHandle, settings: &Settings) -> bool {
    settings_pair_written(save_settings(app, settings), SETTINGS_LOCKED.load(Ordering::Relaxed))
}

pub(crate) fn settings_pair_written(written: bool, locked: bool) -> bool {
    written || locked
}

// What a settings write did. Refused is a by-design no-op (read-only data folder, demo
// transition, locked settings row) that a retry cannot change; only Failed is worth owing again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsWrite {
    Written,
    Refused,
    Failed,
}

// The write itself, without the "cancel a collected write" step: the collecting
// path below calls this one so it does not clear its own mark twice.
pub(crate) fn write_settings_outcome(app: &AppHandle, settings: &Settings) -> SettingsWrite {
    mirror_minimize_to_tray(settings);
    if data_dir_write_blocked(app) {
        return SettingsWrite::Refused;
    }
    // Safety valve: the row on disk is present but unreadable, so the in-memory Settings are
    // defaults. Rewriting would destroy the real row.
    if SETTINGS_LOCKED.load(Ordering::Relaxed) {
        return SettingsWrite::Refused;
    }
    if let Some(conn) = db_conn(app) {
        if db_write_settings(&conn, settings).is_ok() {
            drop_stale_fallback(&data_dir(app), "settings.json");
            // Announced only on the true to false transition (swap), not on every successful write.
            if SETTINGS_WRITE_FAILED.swap(false, Ordering::Relaxed) {
                let _ = app.emit("settings-write-recovered", ());
            }
            return SettingsWrite::Written;
        }
    }
    if !write_sealed_json(&data_dir(app).join("settings.json"), settings) {
        append_crash_log("persist failed: settings could not be written to the database or the JSON fallback");
        SETTINGS_WRITE_FAILED.store(true, Ordering::Relaxed);
        let _ = app.emit("persist-failed", "settings");
        return SettingsWrite::Failed;
    }
    if SETTINGS_WRITE_FAILED.swap(false, Ordering::Relaxed) {
        let _ = app.emit("settings-write-recovered", ());
    }
    SettingsWrite::Written
}

// Counter writes are collected instead of going to disk one at a time. save_settings
// re-encrypts and rewrites the whole settings block, but rotation pointers, usage counters
// and "last used" stamps change on every copy. They mark the block dirty instead, and the
// shared timer thread writes it once the marking has been quiet for a moment.
//
// Only the disk write waits: callers update store.settings under the store lock before
// marking. If the process is killed outright, up to one second of counter state is lost (a
// rotation pointer, a use count, a "last used" stamp), never a prompt, text or a setting
// the user changed. User edits write through at once, and orderly exit, session end,
// backup, export, import, reset and data-folder move flush what is owed first.
pub(crate) const SETTINGS_QUIET_MS: u64 = 1000;
// How often the timer thread checks the mark: finer than the quiet window so the write
// lands just behind it. A round with nothing owed costs one uncontended mutex.
pub(crate) const SETTINGS_FLUSH_TICK_MS: u64 = 250;
// After a collected write failed, the timer thread waits this long before trying again, so a
// broken disk is not hit (and logged) every tick. Exit and backup flushes ignore it.
pub(crate) const SETTINGS_RETRY_MS: u64 = 30_000;

// Marked = a settings write is owed since that instant. Each mark inside the quiet window
// pushes the write out, so a burst of copies becomes one write.
pub(crate) struct SettingsDelay {
    pub(crate) marked: Mutex<Option<Instant>>,
    pub(crate) quiet: Mutex<Duration>,
    // Earliest instant the timer thread may retry after a failed write.
    pub(crate) retry_at: Mutex<Option<Instant>>,
}

impl SettingsDelay {
    pub(crate) const fn new(quiet_ms: u64) -> Self {
        SettingsDelay {
            marked: Mutex::new(None),
            quiet: Mutex::new(Duration::from_millis(quiet_ms)),
            retry_at: Mutex::new(None),
        }
    }

    pub(crate) fn mark(&self) {
        *self.marked.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    }

    // Is anything owed at all? Cheap enough for the idle tick to ask every round.
    pub(crate) fn owed(&self) -> bool {
        self.marked.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    // Owed and past the retry backoff: the idle tick asks this before taking any lock, so a
    // debt waiting out its backoff costs nothing. `take(false)` still decides.
    pub(crate) fn due(&self) -> bool {
        if !self.owed() {
            return false;
        }
        let retry_at = self.retry_at.lock().unwrap_or_else(|e| e.into_inner());
        !retry_at.is_some_and(|t| Instant::now() < t)
    }

    // The ONE place that decides whether a collected write happens, so the number
    // of `true` answers is exactly the number of disk writes on this path.
    // `force` skips the quiet window and the retry backoff: an exit or a backup cannot wait
    // them out. A `true` answer clears the backoff; the caller reports a failed write
    // through `failed`.
    pub(crate) fn take(&self, force: bool) -> bool {
        let quiet = *self.quiet.lock().unwrap_or_else(|e| e.into_inner());
        let mut marked = self.marked.lock().unwrap_or_else(|e| e.into_inner());
        let mut retry_at = self.retry_at.lock().unwrap_or_else(|e| e.into_inner());
        if !force && retry_at.is_some_and(|t| Instant::now() < t) {
            return false;
        }
        match *marked {
            Some(at) if force || at.elapsed() >= quiet => {
                *marked = None;
                *retry_at = None;
                true
            }
            _ => false,
        }
    }

    // The write that `take` approved did not land: it is owed again, but not before the
    // backoff has passed. A newer mark that arrived during the write is kept as it is.
    pub(crate) fn failed(&self) {
        let mut marked = self.marked.lock().unwrap_or_else(|e| e.into_inner());
        if marked.is_none() {
            *marked = Some(Instant::now());
        }
        *self.retry_at.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(Instant::now() + Duration::from_millis(SETTINGS_RETRY_MS));
    }

    // After a write that `take` approved: only a real failure is owed again. A refusal by
    // design would refuse every retry too (and toast each time), so its debt stays dropped.
    pub(crate) fn settle_collected(&self, outcome: SettingsWrite) {
        if outcome == SettingsWrite::Failed {
            self.failed();
        }
    }

    // After an immediate write of the whole block: written or refused leaves nothing owed; a
    // failed one covers nothing, so the collected change stays owed.
    pub(crate) fn settle_immediate(&self, outcome: SettingsWrite) {
        if outcome != SettingsWrite::Failed {
            self.discard();
        }
    }

    // An immediate write of the same block has just happened: nothing left owed.
    pub(crate) fn discard(&self) {
        *self.marked.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *self.retry_at.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

pub(crate) static SETTINGS_DELAY: SettingsDelay = SettingsDelay::new(SETTINGS_QUIET_MS);

// Serializes the writes, so a collected write on the timer thread cannot land on a newer
// immediate one. Lock order: store lock first, this one second.
pub(crate) static SETTINGS_WRITE: Mutex<()> = Mutex::new(());

pub(crate) fn settings_write_lock() -> std::sync::MutexGuard<'static, ()> {
    SETTINGS_WRITE.lock().unwrap_or_else(|e| e.into_inner())
}

// A counter or pointer change, already correct in memory: one disk write per quiet second
// instead of one per copy. The caller must update store.settings first.
pub(crate) fn save_settings_soon() {
    SETTINGS_DELAY.mark();
}

// Write a collected change out now. `settings` is the live block, which the
// caller already holds under the store lock.
pub(crate) fn flush_settings(app: &AppHandle, settings: &Settings) {
    let _writing = settings_write_lock();
    if SETTINGS_DELAY.take(true) {
        SETTINGS_DELAY.settle_collected(write_settings_outcome(app, settings));
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
    if !SETTINGS_DELAY.due() {
        return;
    }
    let Some(state) = app.try_state::<Db>() else {
        return;
    };
    let store = state.lock().unwrap_or_else(|e| e.into_inner());
    let _writing = settings_write_lock();
    if SETTINGS_DELAY.take(false) {
        SETTINGS_DELAY.settle_collected(write_settings_outcome(app, &store.settings));
    }
}

// The self-filling placeholder names, kept in step with DYN_VARS in ui/promptVars.js.
pub(crate) const DYN_NAMES: [&str; 16] = [
    "date", "time", "datetime", "weekday", "yesterday", "tomorrow", "week", "month",
    "year", "timestamp", "epoch", "clipboard", "username", "uuid", "random", "counter",
];

// Placeholders are written {?Name}, {?Name=default}, {?Name|A, B} and {?@date}. Rewrites
// every old-style token ({#{Name}#}, {#{/date\}#}, bare {#{date}#}) of text in source
// order. Idempotent. A token whose body is empty or would exceed the new body limit stays
// as typed.
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
    // id is the prompts table's primary key, so a duplicate fails the whole rewrite, and
    // save_prompts then quietly writes the JSON fallback that the database path ignores: the
    // import looks fine and the old prompts return after a restart. Only an import or a
    // hand-edited backup can duplicate an id; the second prompt gets a fresh one.
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

// One-time data upgrade for stored history: every version row and trash entry in the old
// placeholder spelling is rewritten, so a restored snapshot reads right. Returns whether it
// all landed; the caller records the new data version only then, and the rewrite is
// idempotent, so a failed run repeats at the next start.
pub(crate) fn upgrade_stored_placeholders(app: &AppHandle, store: &mut Store) -> bool {
    let Some(mut conn) = db_conn(app) else {
        return false;
    };
    if upgrade_history_placeholders(&mut conn, &mut store.trash).is_err() {
        return false;
    }
    // The prompts were upgraded in memory by migrate_prompts and are written here with the
    // trash. A locked store (rows that failed to decrypt) is never written.
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
            // dec_str answers "" for what it cannot read; a non-empty stored value that reads back
            // empty is left alone.
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
        // A row (prompts, trash or settings) could not be read back or decrypted: preserve the raw
        // DB and lock the affected writes (prompts_locked, SETTINGS_LOCKED).
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
        // Empty DB but legacy JSON present: import it once. A corrupt settings row must not count
        // as "nothing here yet", or an unrelated legacy JSON file would paper over SETTINGS_LOCKED.
        if prompts.is_empty() && settings_opt.is_none() && !settings_unreadable {
            let j_settings_r: SealedRead<Settings> = read_sealed_json(&dir.join("settings.json"));
            let j_prompts_r: SealedRead<Vec<Prompt>> = read_sealed_json(&dir.join("prompts.json"));
            // An unreadable legacy file (wrong Windows account, corruption) is not "nothing to
            // import". Skip the whole import this launch and leave the file for inspection.
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
                    // Once the data is in the DB, drop the legacy JSON files so the import never reruns.
                    if prompts_ok && settings_ok {
                        let _ = fs::remove_file(dir.join("prompts.json"));
                        let _ = fs::remove_file(dir.join("settings.json"));
                    }
                }
                prompts = j_prompts;
                settings = j_settings;
            }
        } else {
            // prompts.json and settings.json otherwise exist only as a fallback from a failed DB write
            // earlier, deleted once the DB accepts the data again. Their presence means the DB never
            // got that change, so the file is the truth. Retry the write; if the DB still refuses, keep
            // the file and loaded data so every launch retries instead of losing the edit.
            // Skipped when rows failed to decrypt: the store is locked then (prompts_locked), and a
            // partial retry would fight that lock.
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
        // Force a resave when the "ai" theme is rewritten to "neon", or the fix lives only in
        // memory until an unrelated change persists it.
        let theme_ai_before =
            settings.theme == "ai" || settings.custom_themes.iter().any(|c| c.base == "ai");
        let pruned_removed_values = settings.has_removed_ui_values();
        // An unreadable settings row, or a missing one next to existing data, belongs to an
        // existing install and must not start pruning under the new defaults.
        if settings_unreadable
            || (settings_defaulted && (!prompts.is_empty() || copy_log_has_rows(&conn)))
        {
            settings.retention_defaults_v2 = false;
        }
        let pin_retention = !settings.retention_defaults_v2;
        settings.migrate();
        load_pick_history(&settings); // restore the colour-pick history mirror
        // One-time: move any legacy in-blob copy history into the copy_log table. Gated on a
        // persistent meta marker written in the same transaction as the rows, so a failed resave
        // cannot duplicate the history on the next launch.
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
        // A relative file_path (produced only by demo_prompts(), never by the attachment picker)
        // is resolved against the live data dir here instead of being stored absolute.
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
        // An existing install never sees the first-run tour; the tour's own Finish/Skip would never
        // write the flag for a store discovered this way.
        if tour_should_be_marked_seen(!store.prompts.is_empty(), store.settings.examples_seeded) {
            store.settings.ui_flags.entry("tourSeen".to_string()).or_insert(true);
        }
        // A purge whose version-history DELETE failed is not retried by the purge itself, so the
        // text would stay in prompt_versions. Only when no JSON fallback is waiting, so the tables
        // are the whole library.
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
    let (mut store, settings_unreadable) = load_store_from_json_parts(&dir);
    // The same write-lock the database path sets for a settings row it could not
    // read: a later settings write must not replace the unreadable file with defaults.
    SETTINGS_LOCKED.store(settings_unreadable, Ordering::Relaxed);
    acquire_data_dir_lock(&dir, data_dir_lock_enabled(&store.settings));
    if tour_should_be_marked_seen(!store.prompts.is_empty(), store.settings.examples_seeded) {
        store.settings.ui_flags.entry("tourSeen".to_string()).or_insert(true);
    }
    store.normalize_folders();
    sweep_trash(app, &mut store); // no-op here (fallback path never loads trash), kept for symmetry
    store
}

// The dataDirLock toggle: absent or true is on, explicit false is off (like colorPicker
// and the other default-on flags).
pub(crate) fn data_dir_lock_enabled(settings: &Settings) -> bool {
    settings.ui_flags.get("dataDirLock") != Some(&false)
}

// What a launch can still recover when the database cannot be opened. Split out of
// load_store so the write-lock decision is testable without an AppHandle. With no second
// source, an unreadable file locks writes (like decrypt_fails in load_store) instead of
// presenting and saving an empty store.
#[cfg(test)]
pub(crate) fn load_store_from_json(dir: &Path) -> Store {
    load_store_from_json_parts(dir).0
}

// Also says whether settings.json was there but unreadable, so the caller that
// owns the process-wide settings write-lock can set it.
pub(crate) fn load_store_from_json_parts(dir: &Path) -> (Store, bool) {
    let mut unreadable = false;
    let mut settings_unreadable = false;
    let mut settings_missing = false;
    let mut settings = match read_sealed_json::<Settings>(&dir.join("settings.json")) {
        SealedRead::Loaded(s) => s,
        SealedRead::Unreadable => {
            unreadable = true;
            settings_unreadable = true;
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
    // An earlier session already wrote data.db, so its rows exist on disk and db_conn just
    // could not open it this time. Without prompts.json either, an empty-but-unlocked store
    // would let the next save write that emptiness over the table and to prompts.json, which
    // load_store then trusts over the DB. settings.json is irrelevant here: a session whose
    // settings write fell back to the file leaves settings.json without prompts.json.
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
    let store = Store {
        prompts,
        trash: Vec::new(),
        settings,
        prompts_locked: unreadable,
        written: std::cell::RefCell::new(WrittenRows::new()),
    };
    (store, settings_unreadable)
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

// Non-blocking variant for code on the event-loop thread: None while another thread
// holds the mutex (a backup or import), a poisoned one is taken over like lock() does.
pub(crate) fn try_lock_ignoring_poison<T>(m: &Mutex<T>) -> Option<std::sync::MutexGuard<'_, T>> {
    match m.try_lock() {
        Ok(g) => Some(g),
        Err(std::sync::TryLockError::Poisoned(e)) => Some(e.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => None,
    }
}

pub(crate) fn lock<'a>(state: &'a State<'a, Db>) -> std::sync::MutexGuard<'a, Store> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}
