use super::*;

// ---------- Paths + persistence ----------

pub(crate) fn default_data_dir(app: &AppHandle) -> PathBuf {
    // Portable build keeps its data beside the exe so it travels with the folder;
    // installed builds use %APPDATA%\<DATA_FOLDER>. If the exe folder is not
    // writable (e.g. run from a read-only location), fall back to AppData.
    if is_portable() {
        if let Some(dir) = portable_data_dir() {
            if portable_dir_usable(&dir) {
                return dir;
            }
        }
    }
    let dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|p| p.join(DATA_FOLDER))
        .or_else(|| app.path().app_data_dir().ok())
        .unwrap_or_else(|| std::env::temp_dir().join(DATA_FOLDER));
    let _ = fs::create_dir_all(&dir);
    migrate_legacy_data(app, &dir);
    dir
}

// Whether `dir` can hold the store: it exists (or can be made) and a file can be
// created and removed in it. `create_dir_all` alone succeeds on an existing
// read-only folder, so the portable choice needs a real write probe.
pub(crate) fn dir_is_writable(dir: &std::path::Path) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".write-probe-{}", std::process::id()));
    let ok = fs::write(&probe, b"").is_ok();
    let _ = fs::remove_file(&probe);
    ok
}

// The portable folder is probed once per process: the answer does not change
// while the app runs and `data_dir` is called on every store access.
fn portable_dir_usable(dir: &std::path::Path) -> bool {
    static USABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *USABLE.get_or_init(|| dir_is_writable(dir))
}

// Portable data lives directly beside the exe (no sub-folder) so a single
// portable download stays self-contained and the store travels with it.
pub(crate) fn portable_data_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(|d| d.to_path_buf())
}

// Files that make up a store, in copy order.
pub(crate) const STORE_FILES: [&str; 3] = ["data.db", "prompts.json", "settings.json"];

// The installed copy's store, if this is a portable run and one exists elsewhere. Nothing is
// copied here: the app asks first (see `takeover_offer`) in its own, themed window.
pub(crate) fn appdata_store_dir(app: &AppHandle, portable: &std::path::Path) -> Option<PathBuf> {
    let src = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|p| p.join(DATA_FOLDER))
        .or_else(|| app.path().app_data_dir().ok())?;
    if src.as_path() == portable {
        return None;
    }
    STORE_FILES.iter().any(|f| src.join(f).exists()).then_some(src)
}

// One-time move of the store out of the old identifier-based folder (brand::LEGACY_DATA_FOLDER)
// into DATA_FOLDER. Both the folder Tauri resolves and the frozen legacy name are tried, so
// it survives a change in how app_data_dir is built.
pub(crate) fn migrate_legacy_data(app: &AppHandle, new: &std::path::Path) {
    if new.join("data.db").exists() {
        return;
    }
    let mut olds: Vec<PathBuf> = Vec::new();
    if let Ok(dir) = app.path().app_data_dir() {
        olds.push(dir);
    }
    if !brand::LEGACY_DATA_FOLDER.is_empty() {
        if let Some(roaming) = std::env::var_os("APPDATA") {
            olds.push(PathBuf::from(roaming).join(brand::LEGACY_DATA_FOLDER));
        }
    }
    for old in olds {
        if old.as_path() == new || !STORE_FILES.iter().any(|f| old.join(f).exists()) {
            continue;
        }
        for f in STORE_FILES {
            if old.join(f).exists() {
                let _ = fs::copy(old.join(f), new.join(f));
            }
        }
        return;
    }
}

// Portable build = run without the installer's uninstall.exe beside the exe
// (standard installs write one; portable mode and dev runs do not). Portable
// always fully closes on window-close; installed honours the tray setting.
pub(crate) fn is_portable() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| !d.join("uninstall.exe").exists()))
        .unwrap_or(true)
}

// Shared by data_dir() and the command line's headless resolver below: a
// "datapath" pointer file in the canonical dir can redirect the store to a
// user-chosen folder (expert menu); anything missing or invalid falls back to
// the canonical dir, so data is never lost.
pub(crate) fn resolve_datapath_redirect(base: PathBuf) -> PathBuf {
    if let Ok(s) = fs::read_to_string(base.join("datapath")) {
        let p = PathBuf::from(s.trim());
        if !s.trim().is_empty() && p.is_dir() {
            return p;
        }
    }
    base
}

// Effective store location: the canonical dir, redirected by a valid "datapath" file.
pub(crate) fn data_dir(app: &AppHandle) -> PathBuf {
    let resolved = resolve_datapath_redirect(default_data_dir(app));
    // Recorded for `append_crash_log`'s scrubbing, see `LAST_KNOWN_DATA_DIR`.
    *LAST_KNOWN_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(resolved.clone());
    resolved
}

// The command line's own copy of default_data_dir(): the same portable/APPDATA resolution
// without the AppHandle-only fallback and the legacy-folder migration (a no-op once data.db
// exists, as it does for any install a command-line launch would find). Needs no window.
pub(crate) fn default_data_dir_headless() -> PathBuf {
    if is_portable() {
        if let Some(dir) = portable_data_dir() {
            if portable_dir_usable(&dir) {
                return dir;
            }
        }
    }
    let dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|p| p.join(DATA_FOLDER))
        .unwrap_or_else(|| std::env::temp_dir().join(DATA_FOLDER));
    let _ = fs::create_dir_all(&dir);
    dir
}

pub(crate) fn data_dir_headless() -> PathBuf {
    resolve_datapath_redirect(default_data_dir_headless())
}

// The store's JSON files (the DB-write fallback and the legacy pre-SQLite format) hold full
// prompt bodies and private notes, so they are DPAPI-sealed like the SQLite rows. `dec_str`
// passes an un-prefixed value through, so an old plaintext install still migrates and is
// re-sealed on its next write.
// "Missing" (safe to treat as empty) and "unreadable" (a file is there but DPAPI/JSON decoding
// failed: wrong Windows account, corruption) are different: folding both into a bare default
// would silently replace real data with an empty one.
pub(crate) enum SealedRead<T> {
    Missing,
    Unreadable,
    Loaded(T),
}

pub(crate) fn read_sealed_json<T: for<'de> Deserialize<'de>>(path: &PathBuf) -> SealedRead<T> {
    match fs::read_to_string(path) {
        Ok(raw) if !raw.trim().is_empty() => match serde_json::from_str(&dec_str(&raw)) {
            Ok(v) => SealedRead::Loaded(v),
            Err(_) => SealedRead::Unreadable,
        },
        Ok(_) => SealedRead::Missing, // empty file: nothing meaningful was ever written
        Err(_) => SealedRead::Missing,
    }
}

// Every import reads a user-picked file (possibly from someone else) fully into memory, so a
// wrong pick (a disk image, a video) must fail with a clear message instead of exhausting
// memory. Mirrors the cap `inflate_capped` applies.
pub(crate) const MAX_IMPORT_BYTES: u64 = 512 * 1024 * 1024;
pub(crate) const MAX_THEME_BYTES: u64 = 4 * 1024 * 1024; // a theme is a handful of colour values

pub(crate) fn size_error(max: u64) -> String {
    format!("file is too large; the limit is {} MB", max / (1024 * 1024))
}

// For the one case that cannot read the bytes itself: the image given to Windows' text
// recognizer is opened by Windows, so measuring it up front is the only check available.
pub(crate) fn check_import_size(path: &std::path::Path, max: u64) -> Result<(), String> {
    let len = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len > max {
        return Err(format!(
            "file is too large ({} MB); the limit is {} MB",
            len / (1024 * 1024),
            max / (1024 * 1024)
        ));
    }
    Ok(())
}

// One open, one read, one decision: measuring and then reading would open the file twice, and
// it could grow or be swapped in between. Reading one byte past the limit proves "too large"
// from the bytes actually in hand.
pub(crate) fn read_file_capped(path: &std::path::Path, max: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    file.take(max + 1).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    if buf.len() as u64 > max {
        return Err(size_error(max));
    }
    Ok(buf)
}

// The text counterpart, for the Markdown sync folder: those files belong to the user and the
// folder is rescanned on a timer, so anything past the limit is skipped instead of read every cycle.
pub(crate) fn read_text_capped(path: &std::path::Path, max: u64) -> Option<String> {
    String::from_utf8(read_file_capped(path, max).ok()?).ok()
}

// The JSON pair is only the fallback for a failed database write. Once the database has
// accepted the same rows, a leftover file from an earlier failed write is stale and would be
// read back as the truth on the next launch (see load_store).
pub(crate) fn drop_stale_fallback(dir: &Path, file: &str) {
    let path = dir.join(file);
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

// Atomic write: returns whether the data reached disk, so callers can log a persist failure.
pub(crate) fn write_sealed_json<T: Serialize>(path: &Path, data: &T) -> bool {
    match serde_json::to_string(data).map(|j| enc_str(&j)) {
        Ok(json) => write_bytes_atomic(path, json.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

// Never truncate an existing file in place: a crash or a cloud-sync client reading mid-write
// would see a valid head with a truncated body and import it as truth. Temp-then-rename next
// to the target. The temp name appends ".tmp-<id>" to the whole file name (path.md ->
// path.md.tmp-<id>), so a filter on one extension (sync_sweep_orphans' ".md") leaves an
// interrupted write's temp file alone.
pub(crate) fn write_file_atomic(path: &Path, content: &str) -> bool {
    write_bytes_atomic(path, content.as_bytes()).is_ok()
}

// The same for bytes that are not necessarily text (a conflict copy keeps a file exactly as
// saved), with the reason on failure. Data is flushed before the rename, the rename replaces
// the target in one step, and on any error the temp file is removed and the old target stays.
//
// A symlink target is followed: the file it points at is replaced, the link stays. Where the
// replace is not allowed (no right to create files there, a file held open without delete
// sharing), it falls back to writing in place.
pub(crate) fn write_bytes_atomic(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let is_link = fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
    let target = if is_link {
        match fs::canonicalize(path) {
            Ok(real) => real,
            // A dangling link: nothing to replace, write through it.
            Err(_) => return fs::write(path, content),
        }
    } else {
        path.to_path_buf()
    };
    match replace_via_temp(&target, content) {
        Err(e) if matches!(e.kind(), std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Unsupported | std::io::ErrorKind::ResourceBusy) => {
            fs::write(&target, content)
        }
        other => other,
    }
}

// The replace with no fallback: a refused swap is an error and the old file stays
// exactly as it was. For a file other processes read while it is rewritten (the
// data-folder lock), where an in-place write would show them a half-written file.
pub(crate) fn write_bytes_atomic_strict(path: &Path, content: &[u8]) -> std::io::Result<()> {
    replace_via_temp(path, content)
}

fn replace_via_temp(path: &Path, content: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".tmp-{}", gen_id()));
    let tmp = path.with_file_name(name);
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(content)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

// Whether a file name is one of the temp files write_bytes_atomic names:
// "<file>.tmp-" followed by gen_id's "p<nanos>_<count>". Nothing else matches, so a
// sweep never touches a file somebody else made.
pub(crate) fn is_atomic_temp_name(name: &str) -> bool {
    let Some((stem, id)) = name.rsplit_once(".tmp-") else { return false };
    let Some(rest) = id.strip_prefix('p') else { return false };
    let Some((nanos, seq)) = rest.split_once('_') else { return false };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    !stem.is_empty() && digits(nanos) && digits(seq)
}

// Removes the atomic-write temp files a crash left in `dir` (not below it) that are
// older than `min_age`; a write in flight is younger and is left alone. Returns how
// many were removed.
pub(crate) fn sweep_atomic_temps(dir: &Path, min_age: Duration) -> usize {
    let mut removed = 0;
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        if !is_atomic_temp_name(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let old_enough = entry
            .metadata()
            .ok()
            .filter(|m| m.is_file())
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age >= min_age);
        if old_enough && fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

// Capitalise a brand word for a dialog title ("prompts" -> "Prompts").
pub(crate) fn title_case(w: &str) -> String {
    let mut c = w.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

// A path that reaches another machine by name: UNC (`\\host\share`, `//host/share`,
// `\\?\UNC\...`). Opening one makes Windows send the user's credentials to that host, so
// nothing an import or a renderer call chose may name one. A drive letter is never judged here,
// mapped or not: a mapped drive is a connection the user made themselves, so naming it sends no
// credentials to a host somebody else picked. This is the one gate for that rule; every place
// that opens a stored or imported path asks it.
// A verbatim (`\\?\`) or NT (`\??\`) path counts as local only when the rest is a local
// drive letter or a volume GUID: GLOBALROOT and device names reach a host as well.
pub(crate) fn is_network_path(raw: &str) -> bool {
    let t = raw.trim();
    let b = t.as_bytes();
    let sep = |c: u8| c == b'\\' || c == b'/';
    let rest = if t.get(..4) == Some(r"\??\") || (b.len() >= 4 && sep(b[0]) && sep(b[1]) && b[2] == b'?' && sep(b[3])) {
        Some(&t[4..])
    } else {
        None
    };
    if let Some(rest) = rest {
        let rb = rest.as_bytes();
        if rb.len() >= 2 && rb[0].is_ascii_alphabetic() && rb[1] == b':' {
            return false;
        }
        return !is_volume_guid_path(rest);
    }
    // Any two leading separators, mixed kinds included, reach a host.
    if b.len() >= 2 && sep(b[0]) && sep(b[1]) {
        return true;
    }
    false
}

// The network gate for a path that may sit in one of the app's own folders (the data folder,
// the recording folder): those are the user's own choice, so a path inside one passes even when
// that folder is a UNC share, while every other network spelling stays refused. Decided on
// text alone, so a refused path is never touched. A path that climbs with ".." is never
// matched to a folder, and neither is a device or verbatim spelling that is not `UNC`.
pub(crate) fn is_foreign_network_path(raw: &str, own_dirs: &[PathBuf]) -> bool {
    is_network_path(raw) && !is_inside_own_network_dir(raw, own_dirs)
}

fn is_inside_own_network_dir(raw: &str, own_dirs: &[PathBuf]) -> bool {
    let t = raw.trim();
    if t.split(['\\', '/']).any(|c| c == "..") {
        return false;
    }
    let low = t.replace('\\', "/").to_ascii_lowercase();
    let verbatim = ["//?/", "/??/", "//./"].iter().any(|p| low.starts_with(p));
    let unc = ["//?/unc/", "/??/unc/", "//./unc/"].iter().any(|p| low.starts_with(p));
    if verbatim && !unc {
        return false;
    }
    // Only a folder that is itself on a share can vouch for a share path; a local folder
    // never exempts a name that reaches a host.
    own_dirs.iter().any(|d| is_network_path(&d.to_string_lossy()) && path_is_inside_dir(Path::new(t), d))
}

// Whether a path handed in from the UI may not become (or stay) a prompt's attachment: it
// names another machine (outside the app's own folders, `own_dirs`), or it is one of the app's
// own store files (the database, its journal, the settings) directly inside the data folder.
// The attachment copies live in the `files` subfolder and stay allowed. Judged on text first,
// so a refused network path is never touched; a file that does not exist is not refused here
// (an attachment that moved must remain re-savable), and an empty path means "none".
pub(crate) fn attachment_path_refused(own_dirs: &[PathBuf], data_dir: &Path, raw: &str) -> bool {
    if raw.trim().is_empty() {
        return false;
    }
    if is_foreign_network_path(raw, own_dirs) {
        return true;
    }
    let (Ok(file), Ok(dir)) = (fs::canonicalize(raw), fs::canonicalize(data_dir)) else { return false };
    file.parent() == Some(dir.as_path())
}

// `Volume{01234567-89ab-cdef-0123-456789abcdef}` followed by nothing or a separator.
fn is_volume_guid_path(rest: &str) -> bool {
    let b = rest.as_bytes();
    const HEAD: &[u8] = b"volume{";
    if b.len() < HEAD.len() + 37 || !b[..HEAD.len()].eq_ignore_ascii_case(HEAD) {
        return false;
    }
    let guid = &b[HEAD.len()..HEAD.len() + 36];
    let shaped = guid.iter().enumerate().all(|(i, c)| match i {
        8 | 13 | 18 | 23 => *c == b'-',
        _ => c.is_ascii_hexdigit(),
    });
    let tail = &b[HEAD.len() + 36..];
    shaped && tail[0] == b'}' && (tail.len() == 1 || tail[1] == b'\\' || tail[1] == b'/')
}
