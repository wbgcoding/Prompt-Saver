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

// Files the user handed to this session through a backend door: a file dialog, the clipboard, or
// a file dropped on the main window. A renderer can name any string in an IPC call, so an
// attachment source counts as the user's choice only if one of these doors recorded it first.
static PICKED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
const PICKED_MAX: usize = 4096;

fn picked_keys(raw: &str) -> Vec<String> {
    let path = Path::new(raw);
    let mut keys = vec![file_key(path)];
    if let Ok(real) = fs::canonicalize(path) {
        let key = file_key(&real);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

pub(crate) fn grant_picked_file(raw: &str) {
    if raw.trim().is_empty() {
        return;
    }
    let mut guard = PICKED.lock().unwrap_or_else(|e| e.into_inner());
    let set = guard.get_or_insert_with(HashSet::new);
    if set.len() >= PICKED_MAX {
        set.clear();
    }
    set.extend(picked_keys(raw));
}

fn is_picked_file(raw: &str) -> bool {
    let guard = PICKED.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_ref().is_some_and(|set| picked_keys(raw).iter().any(|k| set.contains(k)))
}

// Whether `raw` is a stored path marked untrusted in `marked` and not handed over again this
// session: such a path is kept as it is but never copied, probed or shown.
pub(crate) fn held_untrusted(marked: &std::collections::BTreeSet<String>, raw: &str) -> bool {
    !raw.trim().is_empty() && marked.contains(&stored_key(raw)) && !is_picked_file(raw)
}

// The key under which a stored prompt's path is remembered for the trust check.
pub(crate) fn stored_key(raw: &str) -> String {
    file_key(Path::new(raw))
}

// Whether an attachment source named in an IPC call may be copied, probed and shown: empty, a
// file inside one of the app's own folders (resolved, so ".." and links do not escape), a file
// the user picked this session, a file this session's video export wrote (it may sit in a
// folder picked once for that export), or a path a stored prompt already holds.
pub(crate) fn attachment_source_trusted(raw: &str, roots: &[PathBuf], stored: &HashSet<String>) -> bool {
    if raw.trim().is_empty() {
        return true;
    }
    if stored.contains(&stored_key(raw)) || is_picked_file(raw) || crate::video_editor::is_known_export_output(Path::new(raw)) {
        return true;
    }
    if Path::new(raw).components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return false;
    }
    let Ok(file) = fs::canonicalize(raw) else { return false };
    if !file.is_file() {
        return false;
    }
    let file_lower = PathBuf::from(file.to_string_lossy().to_lowercase());
    roots.iter().any(|root| {
        fs::canonicalize(root).is_ok_and(|r| file_lower.starts_with(PathBuf::from(r.to_string_lossy().to_lowercase())))
    })
}

// The folders a renderer-named attachment may come from: everything the app itself writes media
// to (data, recordings, screenshots, the export folder, its temp folder).
pub(crate) fn attachment_source_roots(app: &AppHandle, settings: &Settings) -> Vec<PathBuf> {
    let text = |k: &str| settings.ui_texts.get(k).cloned().unwrap_or_default();
    let mut roots = media_dirs(&data_dir(app), &text("recordingDir"), &std::env::temp_dir());
    roots.push(crate::snip::screenshot_dir(&text("screenshotDir")));
    let export = text("videoExportDir");
    if !export.trim().is_empty() {
        roots.push(PathBuf::from(export));
    }
    roots
}

// The keys of every attachment path the stored prompts and the trash hold and this machine
// trusts: a path an import or the synced folder named without the user's say-so is left out
// until the user picks the file again.
pub(crate) fn trusted_stored_keys(store: &Store) -> HashSet<String> {
    store
        .prompts
        .iter()
        .chain(store.trash.iter())
        .flat_map(|p| [p.file_path.as_str(), p.icon_path.as_str()])
        .filter(|s| !s.trim().is_empty())
        .map(stored_key)
        .filter(|k| !store.settings.untrusted_paths.contains(k))
        .collect()
}

// Whether a stored path was named by a foreign file and not vouched for since.
pub(crate) fn path_untrusted(settings: &Settings, raw: &str) -> bool {
    !raw.trim().is_empty() && settings.untrusted_paths.contains(&stored_key(raw))
}

// The check the attachment commands run before they copy or probe a source. Takes the store lock
// only to read the settings and the stored paths, so call it before the slow work.
pub(crate) fn source_trusted_now(app: &AppHandle, state: &State<'_, Db>, raw: &str) -> bool {
    if raw.trim().is_empty() {
        return true;
    }
    let (roots, stored) = {
        let store = lock(state);
        (attachment_source_roots(app, &store.settings), trusted_stored_keys(&store))
    };
    attachment_source_trusted(raw, &roots, &stored)
}

// The same question for a path a prompt file or the synced folder names. `stored` is what the
// store trusted before that file was read. A path that is not absolute, or that is a foreign
// network spelling, is never trusted (and the network one is never touched to find out).
fn imported_path_trusted(raw: &str, roots: &[PathBuf], stored: &HashSet<String>) -> bool {
    if raw.trim().is_empty() {
        return true;
    }
    if crate::paths::is_foreign_network_path(raw, roots) || !Path::new(raw.trim()).is_absolute() {
        return false;
    }
    attachment_source_trusted(raw, roots, stored)
}

// Marks the attachment paths of the prompts named in `ids` that did not pass the trust check
// against `pre_import` (what the store trusted before the foreign file was applied), and clears
// the mark of one that passes now. The paths stay in the prompts as written. Returns whether the
// marked set changed, so the caller knows to save the settings.
pub(crate) fn distrust_foreign_paths(store: &mut Store, ids: &[String], pre_import: &HashSet<String>, roots: &[PathBuf]) -> bool {
    let ids: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let before = store.settings.untrusted_paths.clone();
    let mut marked = std::mem::take(&mut store.settings.untrusted_paths);
    for p in store.prompts.iter().filter(|p| ids.contains(p.id.as_str())) {
        for raw in [p.file_path.as_str(), p.icon_path.as_str()] {
            if raw.trim().is_empty() {
                continue;
            }
            let key = stored_key(raw);
            if imported_path_trusted(raw, roots, pre_import) {
                marked.remove(&key);
            } else {
                marked.insert(key);
            }
        }
    }
    store.settings.untrusted_paths = marked;
    prune_untrusted(store);
    store.settings.untrusted_paths != before
}

// Forgets the marks no prompt and no trash entry holds any more. Returns whether any went.
pub(crate) fn prune_untrusted(store: &mut Store) -> bool {
    if store.settings.untrusted_paths.is_empty() {
        return false;
    }
    let held: HashSet<String> = store
        .prompts
        .iter()
        .chain(store.trash.iter())
        .flat_map(|p| [p.file_path.as_str(), p.icon_path.as_str()])
        .filter(|s| !s.trim().is_empty())
        .map(stored_key)
        .collect();
    let before = store.settings.untrusted_paths.len();
    store.settings.untrusted_paths.retain(|k| held.contains(k));
    store.settings.untrusted_paths.len() != before
}

// The user handed the file over again (a picker, a drop, the clipboard): the paths a saved prompt
// holds that were picked this session are trusted from now on. Returns whether a mark went.
pub(crate) fn trust_picked_paths(settings: &mut Settings, paths: &[&str]) -> bool {
    if settings.untrusted_paths.is_empty() {
        return false;
    }
    let before = settings.untrusted_paths.len();
    for raw in paths {
        if !raw.trim().is_empty() && is_picked_file(raw) {
            settings.untrusted_paths.remove(&stored_key(raw));
        }
    }
    settings.untrusted_paths.len() != before
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

// Every attachment the stored prompts and the trash point at, except the paths an import named
// that the user has not picked since.
pub(crate) fn allow_store_files(app: &AppHandle, store: &Store) {
    let data = data_dir(app);
    let custom = store.settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    let own_dirs = vec![data.clone(), crate::media_files::recording_dir(&custom)];
    // Paths already allowed are skipped before the (disk-touching) refusal check.
    let refused = |raw: &str| {
        if path_untrusted(&store.settings, raw) {
            return true;
        }
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
