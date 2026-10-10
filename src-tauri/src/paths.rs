use super::*;

// ---------- Paths + persistence ----------

pub(crate) fn default_data_dir(app: &AppHandle) -> PathBuf {
    // Portable build keeps its data beside the exe so it travels with the folder;
    // installed builds use %APPDATA%\<DATA_FOLDER>. If the exe folder is not
    // writable (e.g. run from a read-only location), fall back to AppData.
    if is_portable() {
        if let Some(dir) = portable_data_dir() {
            if fs::create_dir_all(&dir).is_ok() {
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

// Portable data lives directly beside the exe (no sub-folder) so a single
// portable download stays self-contained and the store travels with it.
pub(crate) fn portable_data_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(|d| d.to_path_buf())
}

// Files that make up a store, in copy order.
pub(crate) const STORE_FILES: [&str; 3] = ["data.db", "prompts.json", "settings.json"];

// The installed copy's store, if this is a portable run and one exists elsewhere.
// Nothing is copied here: the app asks first (see `takeover_offer`), in its own
// window, so the question inherits the theme and language instead of being a bare
// system box before anything is loaded.
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

// One-time move of the store out of the old identifier-based folder
// (brand::LEGACY_DATA_FOLDER) into DATA_FOLDER, so the rename never loses data.
// Both the identifier folder Tauri resolves and the frozen legacy name are tried:
// the second one survives even if Tauri ever changes how app_data_dir is built.
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

// Effective store location. A "datapath" pointer file in the canonical dir can
// redirect the store to a user-chosen folder (expert menu); on anything missing
// or invalid it falls back to the canonical dir, so data is never lost.
pub(crate) fn data_dir(app: &AppHandle) -> PathBuf {
    let resolved = resolve_datapath_redirect(default_data_dir(app));
    // Recorded for `append_crash_log`'s scrubbing, see `LAST_KNOWN_DATA_DIR`.
    *LAST_KNOWN_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(resolved.clone());
    resolved
}

// The command line's own copy of default_data_dir(): the same portable/APPDATA
// resolution, minus the AppHandle-only fallback (app.path().app_data_dir(),
// which default_data_dir only reaches when APPDATA itself is unset) and the
// legacy-folder migration (a no-op the moment data.db already exists, which it
// does for every install a command-line launch would ever find). A launch with
// no AppHandle in reach at all -- there is no window to build one for -- can
// still resolve the exact folder an ordinary launch would use.
pub(crate) fn default_data_dir_headless() -> PathBuf {
    if is_portable() {
        if let Some(dir) = portable_data_dir() {
            if fs::create_dir_all(&dir).is_ok() {
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

// The store's JSON files (prompts.json / settings.json: the DB-write fallback and the
// legacy pre-SQLite format) hold full prompt bodies and private notes, so they are
// DPAPI-sealed exactly like the SQLite rows. `dec_str` passes an un-prefixed value
// through unchanged, which is what still lets a genuinely old plaintext install be
// migrated in; it is re-sealed the next time it is written.
// "Missing" (nothing there yet, safe to treat as empty) and "unreadable" (a file IS
// there but DPAPI/JSON decoding failed: wrong Windows account, corruption) are
// different situations: a caller that folds both into a bare default silently
// replaces real, still-present data with an empty one.
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

// Every import reads a file the user picked (which may have come from someone else)
// fully into memory before parsing, so a wrong pick (a disk image, a video) has to fail
// with a clear message instead of exhausting memory. Mirrors the cap the decompression
// path already applies via `inflate_capped`.
pub(crate) const MAX_IMPORT_BYTES: u64 = 512 * 1024 * 1024;
pub(crate) const MAX_THEME_BYTES: u64 = 4 * 1024 * 1024; // a theme is a handful of colour values

pub(crate) fn size_error(max: u64) -> String {
    format!("file is too large; the limit is {} MB", max / (1024 * 1024))
}

// For the one case that cannot read the bytes itself: the image handed to Windows'
// text recognizer is opened by Windows, so measuring it up front is the only check
// available: and a plausible reject message is worth more there than none.
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

// One open, one read, one decision. Measuring the file and then reading it means
// two separate opens: the file can grow (or be swapped for a different one)
// in between, and the limit then describes a file that is no longer the one being
// read. Reading one byte past the limit is what makes "too large" provable from
// the bytes actually in hand.
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

// The text counterpart, for the Markdown sync folder: the files there belong to the
// user and nothing stops one of them from being enormous, while the folder is
// rescanned on a timer. Anything past the limit is skipped rather than read into
// memory on every single cycle.
pub(crate) fn read_text_capped(path: &std::path::Path, max: u64) -> Option<String> {
    String::from_utf8(read_file_capped(path, max).ok()?).ok()
}

// Atomic write: temp file then rename, so a crash never truncates data. Returns
// whether the data actually reached disk, so callers can log a persist failure.
// The JSON pair is only the fallback for a failed database write. Once the
// database has accepted the same rows, a file left over from an earlier failed
// write is stale and would be read back as the truth on the next launch (see
// load_store), overwriting what was just saved.
pub(crate) fn drop_stale_fallback(dir: &Path, file: &str) {
    let path = dir.join(file);
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

pub(crate) fn write_sealed_json<T: Serialize>(path: &Path, data: &T) -> bool {
    match serde_json::to_string(data).map(|j| enc_str(&j)) {
        Ok(json) => write_bytes_atomic(path, json.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

// Never truncate an existing file in place: a crash or a cloud-sync client
// reading mid-write would otherwise see a valid frontmatter head with a truncated
// body, and a later poll then imports that as truth. Temp-then-rename next to the
// target. The temp name keeps the caller's whole file name and appends
// ".tmp-<id>", so its extension is never the caller's own (path.md ->
// path.md.tmp-<id>): a filter that only touches one specific extension
// (sync_sweep_orphans' ".md" check) leaves a leftover temp file alone if the
// rename is ever interrupted.
pub(crate) fn write_file_atomic(path: &Path, content: &str) -> bool {
    write_bytes_atomic(path, content.as_bytes()).is_ok()
}

// The same, for bytes that are not necessarily text (a conflict copy keeps a file
// exactly as it was, whatever encoding an editor saved it in), and with the
// reason when it fails. The data is flushed to disk before the rename, the
// rename replaces an existing target in one step, and on any error the temp file
// is removed and the old target is left exactly as it was.
pub(crate) fn write_bytes_atomic(path: &Path, content: &[u8]) -> std::io::Result<()> {
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

// Capitalise a brand word for a dialog title ("prompts" -> "Prompts").
pub(crate) fn title_case(w: &str) -> String {
    let mut c = w.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
