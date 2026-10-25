use super::*;
use std::collections::HashSet;

// The asset protocol serves files to every window through `asset:` URLs. The static scope in
// tauri.conf.json allows nothing: what the windows show is allowed here at run time, once each,
// so a page that is subverted can read the folders and attachments the app itself works with,
// not the rest of the disk. The deny list in tauri.conf.json still wins over everything below
// (store files, backups, sidecars).
//
// Allowed: the data folder, the recording folder, the app's temp folder, every attachment path a
// prompt carries, the files a picker, the clipboard or a "Save as" hands back, and (by Tauri
// itself) files dropped onto a window.

// What has been allowed already, so a save that touches thousands of prompts pushes each path
// into the scope once instead of on every write.
static ALLOWED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

// First sighting of `key` in `seen`: true means the caller still has to allow it.
pub(crate) fn first_sighting(seen: &mut HashSet<String>, key: String) -> bool {
    seen.insert(key)
}

fn remember(key: String) -> bool {
    let mut guard = ALLOWED.lock().unwrap_or_else(|e| e.into_inner());
    first_sighting(guard.get_or_insert_with(HashSet::new), key)
}

fn forget(key: &str) {
    if let Some(set) = ALLOWED.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        set.remove(key);
    }
}

fn dir_key(dir: &Path) -> String {
    format!("d\u{1}{}", dir.to_string_lossy().to_lowercase())
}

fn file_key(path: &Path) -> String {
    format!("f\u{1}{}", path.to_string_lossy().to_lowercase())
}

// The folders the windows read media from: data folder (stills, thumbnails, raw recordings),
// recording folder (saved takes), and the app's own temp folder (editor working copies,
// filmstrips, clip exports). Empty and repeated entries are dropped.
pub(crate) fn media_dirs(data: &Path, recording_custom: &str, temp: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for dir in [data.to_path_buf(), crate::media_files::recording_dir(recording_custom), temp.join(brand::TEMP_DIR)] {
        if dir.as_os_str().is_empty() || out.iter().any(|d| d == &dir) {
            continue;
        }
        out.push(dir);
    }
    out
}

// The attachment paths of a prompt list that a window may show: the file and the icon, not
// empty, and not a foreign network path (the same refusal that keeps a path from being probed or
// copied also keeps it out of the scope, so showing it cannot make this machine contact a host).
pub(crate) fn prompt_asset_paths<'a>(prompts: &'a [Prompt], refused: &dyn Fn(&str) -> bool) -> Vec<&'a str> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out: Vec<&str> = Vec::new();
    for p in prompts {
        for raw in [p.file_path.as_str(), p.icon_path.as_str()] {
            if !raw.trim().is_empty() && seen.insert(raw) && !refused(raw) {
                out.push(raw);
            }
        }
    }
    out
}

fn allow_dir_now(app: &AppHandle, dir: &Path) {
    let key = dir_key(dir);
    if !remember(key.clone()) {
        return;
    }
    if app.asset_protocol_scope().allow_directory(dir, true).is_err() {
        forget(&key);
    }
}

// One file a window is about to show (a pick, a paste, a save-as target); "" is ignored.
pub(crate) fn allow_file(app: &AppHandle, raw: &str) {
    if raw.trim().is_empty() {
        return;
    }
    let path = Path::new(raw);
    let key = file_key(path);
    if !remember(key.clone()) {
        return;
    }
    if app.asset_protocol_scope().allow_file(path).is_err() {
        forget(&key);
    }
}

// The three media folders for the folders currently configured.
pub(crate) fn allow_app_dirs(app: &AppHandle, settings: &Settings) {
    let custom = settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    for dir in media_dirs(&data_dir(app), &custom, &std::env::temp_dir()) {
        allow_dir_now(app, &dir);
    }
}

// Every attachment the stored prompts and the trash point at.
pub(crate) fn allow_store_files(app: &AppHandle, store: &Store) {
    let data = data_dir(app);
    let custom = store.settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    let own_dirs = vec![data.clone(), crate::media_files::recording_dir(&custom)];
    // Paths already allowed are skipped before the (disk-touching) refusal check.
    let refused = |raw: &str| {
        let guard = ALLOWED.lock().unwrap_or_else(|e| e.into_inner());
        if guard.as_ref().is_some_and(|s| s.contains(&file_key(Path::new(raw)))) {
            return false;
        }
        drop(guard);
        attachment_path_refused(&own_dirs, &data, raw)
    };
    for raw in prompt_asset_paths(&store.prompts, &refused).into_iter().chain(prompt_asset_paths(&store.trash, &refused)) {
        allow_file(app, raw);
    }
}

// A freshly loaded store: folders and attachments in one go.
pub(crate) fn allow_loaded(app: &AppHandle, store: &Store) {
    allow_app_dirs(app, &store.settings);
    allow_store_files(app, store);
}
