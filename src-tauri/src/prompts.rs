use super::*;

// ---------- Prompt commands ----------

// Async: clones one prompt that may carry a large base64 image, off the UI thread. Callers
// expect the real editable text, so a confidential entry is decrypted with the session key,
// or refused while the vault is locked rather than handing back ciphertext a save could
// then overwrite.
#[tauri::command]
pub(crate) async fn get_prompt(state: State<'_, Db>, id: String) -> Result<Option<Prompt>, String> {
    let store = lock(&state);
    let Some(p) = store.prompts.iter().find(|p| p.id == id) else {
        return Ok(None);
    };
    if !p.secret {
        return Ok(Some(p.clone()));
    }
    let key = secret_session_key(secret_timeout_min(&store.settings));
    let (text, variants) = reveal_secret_text(p, key)?;
    let mut plain = p.clone();
    plain.text = text;
    plain.variants = variants;
    Ok(Some(plain))
}

// First free [col,row] in row-major order; None if the grid is full.
pub(crate) fn first_free_cell(view: &View) -> Option<[u32; 2]> {
    first_free_cell_excluding(view, &[])
}

// The active view's grid-folder cells. Every caller that hands out a free cell in
// the active view needs these: they live outside view.layouts, so first_free_cell()
// alone cannot see them.
pub(crate) fn active_view_folder_cells(settings: &Settings) -> Vec<[u32; 2]> {
    let Some(vid) = settings.views.get(settings.active_index()).map(|v| v.id.clone()) else {
        return Vec::new();
    };
    folder_cells_in_view(settings, &vid)
}

// The cells and member ids of one view's grid folders, read once. Both live outside
// view.layouts, so every layout rebuild or batch placement has to exclude them.
pub(crate) fn view_folder_sets(
    settings: &Settings,
    view_id: &str,
) -> (std::collections::HashSet<(u32, u32)>, std::collections::HashSet<String>) {
    let cells = folder_cells_in_view(settings, view_id).into_iter().map(|c| (c[0], c[1])).collect();
    (cells, folder_members_in_view(settings, view_id))
}

// Same, but also treats `extra` cells (e.g. another grid folder's own cell, which
// lives outside view.layouts) as taken. Without this, a cell handed out here could
// coincide with a folder's cell: the tile would save fine but never draw, since
// the renderer gives a folder priority over whatever else sits on its cell.
pub(crate) fn first_free_cell_excluding(view: &View, extra: &[[u32; 2]]) -> Option<[u32; 2]> {
    let key = grid_key(view.cols, view.rows);
    let empty = HashMap::new();
    let layout = view.layouts.get(&key).unwrap_or(&empty);
    let mut occupied: std::collections::HashSet<[u32; 2]> = layout.values().cloned().collect();
    occupied.extend(extra.iter().cloned());
    for row in 0..view.rows {
        for col in 0..view.cols {
            if !occupied.contains(&[col, row]) {
                return Some([col, row]);
            }
        }
    }
    None
}

// Places every id from ids not already in layout into a free cell, counting extra_occupied
// (e.g. a grid folder's own cell) as taken. All-or-nothing: if there is no room for every
// id, nothing is placed and layout is unchanged, so a caller that already removed the ids
// elsewhere can react. folder_members are ids already inside a grid folder of this view:
// they need no cell and never get a layout entry, and a stray entry for one is dropped once
// the batch fits, so an inconsistent old save heals.
pub(crate) fn place_ids_in_view(
    layout: &mut std::collections::HashMap<String, [u32; 2]>,
    ids: &[String],
    cols: u32,
    rows: u32,
    extra_occupied: &std::collections::HashSet<(u32, u32)>,
    folder_members: &std::collections::HashSet<String>,
) -> bool {
    let mut occupied: std::collections::HashSet<(u32, u32)> =
        layout.values().map(|c| (c[0], c[1])).collect();
    occupied.extend(extra_occupied.iter().copied());
    let (need, free) = view_capacity(layout, ids, cols, rows, extra_occupied, folder_members);
    if need > free {
        return false;
    }
    for id in ids {
        if folder_members.contains(id.as_str()) {
            layout.remove(id.as_str());
            continue;
        }
        if layout.contains_key(id) {
            continue;
        }
        'find: for row in 0..rows {
            for col in 0..cols {
                if !occupied.contains(&(col, row)) {
                    layout.insert(id.clone(), [col, row]);
                    occupied.insert((col, row));
                    break 'find;
                }
            }
        }
    }
    true
}

// The one capacity accounting for a placement: how many of ids still need a cell, and how
// many cells are free once the layout and extra_occupied are counted.
fn view_capacity(
    layout: &std::collections::HashMap<String, [u32; 2]>,
    ids: &[String],
    cols: u32,
    rows: u32,
    extra_occupied: &std::collections::HashSet<(u32, u32)>,
    folder_members: &std::collections::HashSet<String>,
) -> (usize, usize) {
    let mut occupied: std::collections::HashSet<(u32, u32)> =
        layout.values().map(|c| (c[0], c[1])).collect();
    occupied.extend(extra_occupied.iter().copied());
    let need = ids
        .iter()
        .filter(|id| !folder_members.contains(id.as_str()) && !layout.contains_key(id.as_str()))
        .count();
    let free = (cols as usize * rows as usize).saturating_sub(occupied.len());
    (need, free)
}

// Same accounting `place_ids_in_view` does internally, so a caller that just got
// `false` back can tell the user how many cells are still missing.
pub(crate) fn view_capacity_deficit(
    layout: &std::collections::HashMap<String, [u32; 2]>,
    ids: &[String],
    cols: u32,
    rows: u32,
    extra_occupied: &std::collections::HashSet<(u32, u32)>,
    folder_members: &std::collections::HashSet<String>,
) -> usize {
    let (need, free) = view_capacity(layout, ids, cols, rows, extra_occupied, folder_members);
    need.saturating_sub(free)
}

// Drops ids from every layout size a view has had and from its members list: the source
// side of a cross-view move (batch_prompts "moveView") once the target accepted the ids.
pub(crate) fn strip_ids_from_view(view: &mut View, ids: &[String]) {
    for layout in view.layouts.values_mut() {
        for id in ids {
            layout.remove(id);
        }
    }
    view.members.retain(|m| !ids.contains(m));
}

// Mutation and save core for batch_prompts' addView, removeView and moveView, free of
// AppHandle so the rollback is unit-testable. save commits settings like save_settings does
// and reports whether it landed; on false this restores settings.views and reports
// "save-failed", the contract batch_prompts gives its other actions. A missing view is a
// silent no-op.
pub(crate) fn apply_view_batch_action(
    settings: &mut Settings,
    action: &str,
    ids: &[String],
    view_id: &str,
    from_view_id: &str,
    mut save: impl FnMut(&Settings) -> bool,
) -> Result<(), String> {
    // A grid folder occupies a cell via tile_folders, outside view.layouts. Read both sets before
    // views is borrowed mutably.
    let (folder_cells, folder_members) = view_folder_sets(settings, view_id);
    match action {
        "addView" | "removeView" => {
            let Some(vidx) = settings.views.iter().position(|v| v.id == view_id) else {
                return Ok(());
            };
            let before = settings.views.clone();
            let (cols, rows) = (settings.views[vidx].cols, settings.views[vidx].rows);
            let key = grid_key(cols, rows);
            let layout = settings.views[vidx].layouts.entry(key).or_default();
            if action == "removeView" {
                for id in ids {
                    layout.remove(id);
                }
                settings.views[vidx].members.retain(|m| !ids.contains(m));
            } else {
                if !place_ids_in_view(layout, ids, cols, rows, &folder_cells, &folder_members) {
                    let deficit = view_capacity_deficit(layout, ids, cols, rows, &folder_cells, &folder_members);
                    // The empty layout entry created above must not linger to be saved later.
                    settings.views = before;
                    return Err(format!("view-full:{deficit}"));
                }
                for id in ids {
                    if !settings.views[vidx].members.iter().any(|m| m == id) {
                        settings.views[vidx].members.push(id.clone());
                    }
                }
            }
            if !save(settings) {
                settings.views = before;
                return Err("save-failed".into());
            }
            Ok(())
        }
        // Both layouts change in the same in-memory settings before the single save, so a failure
        // or crash is seen only before or after the whole move, never midway.
        "moveView" => {
            let to_idx = settings.views.iter().position(|v| v.id == view_id);
            let from_idx = settings.views.iter().position(|v| v.id == from_view_id);
            let (Some(to_idx), Some(from_idx)) = (to_idx, from_idx) else {
                return Ok(());
            };
            let before = settings.views.clone();
            let (cols, rows) = (settings.views[to_idx].cols, settings.views[to_idx].rows);
            let key = grid_key(cols, rows);
            let layout = settings.views[to_idx].layouts.entry(key).or_default();
            // All-or-nothing - only drop the source placement once the
            // target has durably accepted every id.
            if !place_ids_in_view(layout, ids, cols, rows, &folder_cells, &folder_members) {
                let deficit = view_capacity_deficit(layout, ids, cols, rows, &folder_cells, &folder_members);
                settings.views = before;
                return Err(format!("view-full:{deficit}"));
            }
            for id in ids {
                if !settings.views[to_idx].members.iter().any(|m| m == id) {
                    settings.views[to_idx].members.push(id.clone());
                }
            }
            if to_idx != from_idx {
                strip_ids_from_view(&mut settings.views[from_idx], ids);
            }
            if !save(settings) {
                settings.views = before;
                return Err("save-failed".into());
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

// "Store files" opt-in flag (off by default).
pub(crate) fn store_files_enabled(store: &Store) -> bool {
    store.settings.ui_flags.get("storeFiles").copied().unwrap_or(false)
}

// Staged copies that are on disk (or about to be) but not yet referenced by any
// prompt in the store. sweep_orphan_files treats them as referenced.
pub(crate) static PENDING_ATTACHMENTS: Mutex<Option<std::collections::HashSet<String>>> = Mutex::new(None);

// The one way two spellings of a Windows path are compared: extended-length prefix dropped,
// forward slashes turned into backslashes, case folded.
pub(crate) fn fold_path(path: &str) -> String {
    path.trim_start_matches(r"\\?\").replace('/', "\\").to_lowercase()
}

pub(crate) fn pending_key(path: &str) -> String {
    fold_path(path)
}

pub(crate) fn pending_attachments_contain(path: &str) -> bool {
    let guard = PENDING_ATTACHMENTS.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_ref().is_some_and(|s| s.contains(&pending_key(path)))
}

// Owns the copies one add/update made before the store lock. While alive they are
// "pending" (never swept). Dropped: pending entries are released, and unless
// `commit` ran, the copies are removed again (validation failure, vanished id).
pub(crate) struct StagedCopies {
    pub(crate) paths: Vec<String>,
    pub(crate) committed: bool,
}

impl StagedCopies {
    pub(crate) fn new() -> Self {
        StagedCopies { paths: Vec::new(), committed: false }
    }
    pub(crate) fn commit(&mut self) {
        self.committed = true;
    }
    pub(crate) fn stage(&mut self, files_dir: &std::path::Path, path: &str) -> String {
        store_attached_file_in(files_dir, path, Some(self))
    }
}

impl Drop for StagedCopies {
    fn drop(&mut self) {
        // While unwinding, the in-memory prompt may already point at the staged path;
        // keep the file and let a later sweep collect a true orphan.
        if !self.committed && !std::thread::panicking() {
            for p in &self.paths {
                let _ = fs::remove_file(p);
            }
        }
        let mut guard = PENDING_ATTACHMENTS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(set) = guard.as_mut() {
            for p in &self.paths {
                set.remove(&pending_key(p));
            }
        }
    }
}

// Copy an attached file into <data>/files so the prompt keeps working even if the
// original is moved or deleted. Returns the stored path, or the original unchanged
// on any problem or if it is already inside the store (never loses the reference).
pub(crate) fn store_attached_file_in(files_dir: &std::path::Path, path: &str, staged: Option<&mut StagedCopies>) -> String {
    if path.is_empty() {
        return String::new();
    }
    let src = std::path::Path::new(path);
    if src.starts_with(files_dir) || !src.is_file() {
        return path.to_string();
    }
    if fs::create_dir_all(files_dir).is_err() {
        return path.to_string();
    }
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let dest = files_dir.join(format!("{}_{}", gen_id(), name));
    let dest_str = dest.to_string_lossy().to_string();
    // Registered BEFORE the copy so a sweep can never see the file unprotected.
    if let Some(st) = staged {
        PENDING_ATTACHMENTS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert_with(Default::default)
            .insert(pending_key(&dest_str));
        st.paths.push(dest_str.clone());
    }
    match fs::copy(src, &dest) {
        Ok(_) => dest_str,
        Err(_) => {
            let _ = fs::remove_file(&dest);
            path.to_string()
        }
    }
}

// Remove copies in <data>/files that no prompt references any more. Gated by the
// cleanupFiles flag (default on) and only touches files in our own store folder. Split into
// a cheap in-memory step (the referenced set, computed under the store lock) and this slow
// filesystem sweep, which must run after the lock is released so a large attachments folder
// cannot freeze other windows. Callers build the set with orphan_cleanup_enabled +
// orphan_referenced_set inside their locked block, then call this once the lock is dropped.
pub(crate) fn sweep_orphan_files(app: &AppHandle, referenced: &std::collections::HashSet<String>) {
    sweep_orphan_files_in(&data_dir(app).join("files"), referenced, || {
        app.try_state::<Db>().map(|state| {
            let store = lock(&state);
            orphan_referenced_set(&store)
        })
    });
}

pub(crate) fn sweep_orphan_files_in(
    files_dir: &std::path::Path,
    referenced: &std::collections::HashSet<String>,
    live_now: impl FnOnce() -> Option<std::collections::HashSet<String>>,
) {
    let entries = match fs::read_dir(files_dir) {
        Ok(e) => e,
        Err(_) => return, // no store folder yet -> nothing to clean
    };
    // Compare case-insensitively with normalized separators (Windows) so a still
    // referenced file is never mistaken for an orphan and deleted.
    let norm = pending_key;
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && !referenced.contains(&norm(&path.to_string_lossy())) && !pending_attachments_contain(&path.to_string_lossy()) {
            candidates.push(path);
        }
    }
    if candidates.is_empty() {
        return;
    }
    // referenced is a snapshot from before the lock was released, and add/update copy a new
    // attachment into this folder before its prompt is in the store (StagedCopies registers
    // those as pending). Re-check every candidate against the store's current state and the
    // pending set right before deleting: one more in-memory HashSet build, no filesystem work
    // under the lock.
    let Some(live) = live_now() else { return };
    for path in candidates {
        if !live.contains(&norm(&path.to_string_lossy())) && !pending_attachments_contain(&path.to_string_lossy()) {
            let _ = fs::remove_file(&path);
        }
    }
}

pub(crate) fn orphan_cleanup_enabled(store: &Store) -> bool {
    store.settings.ui_flags.get("cleanupFiles").copied().unwrap_or(true)
}

// An mp4 that never reached its stop path has no top-level "moov" atom, so it is a crash
// leftover rather than merely in use. Walks only box headers (size + fourcc), never the
// payload, and needs no ffmpeg. size==1 means a 64-bit largesize follows; size==0 means the
// box runs to EOF.
pub(crate) fn mp4_has_moov(path: &std::path::Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = fs::File::open(path) else { return false };
    let Ok(len) = f.metadata().map(|m| m.len()) else { return false };
    let mut pos: u64 = 0;
    let mut header = [0u8; 8];
    while pos + 8 <= len {
        if f.seek(SeekFrom::Start(pos)).is_err() || f.read_exact(&mut header).is_err() {
            return false;
        }
        let size32 = u32::from_be_bytes(header[0..4].try_into().unwrap()) as u64;
        let is_moov = &header[4..8] == b"moov";
        let box_size = match size32 {
            1 => {
                let mut ext = [0u8; 8];
                if f.read_exact(&mut ext).is_err() {
                    return false;
                }
                u64::from_be_bytes(ext)
            }
            0 => len.saturating_sub(pos),
            n => n,
        };
        if box_size < 8 {
            return false; // malformed: stop rather than loop forever
        }
        // A moov box whose declared size runs past the file length means a crash right after the
        // header landed, so accepting the header alone would let truncated recordings pass as
        // healthy.
        if is_moov {
            return pos.saturating_add(box_size) <= len;
        }
        pos = pos.saturating_add(box_size);
    }
    false
}

// Only mp4 gets the moov check: the only format the recorder ever writes
// before an optional post-stop conversion. A non-mp4 file here is either
// mid-conversion output or something else, neither of which this sweep judges.
pub(crate) fn recording_is_broken(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mp4")) && !mp4_has_moov(path)
}

// Removes recordings in <data>/recordings that nobody decided on. They sit there from the
// moment WGC finishes writing until the post-capture menu decides attach, save or discard;
// one never decided on (app closed with the menu open) would otherwise stay forever.
// Age-gated like sweep_stale_temp_files rather than immediate, since a recording open in
// that menu is legitimately unreferenced: only files older than STALE_TEMP_FILE_AGE
// qualify. Runs once at startup.
pub(crate) fn sweep_stale_recordings(
    app: &AppHandle,
    referenced: &std::collections::HashSet<String>,
    broken_sweep_on: bool,
    broken_grace: Duration,
    retention_age: Option<Duration>,
) -> (u32, u64) {
    // A read-only instance judges "unreferenced" against a store that may be stale and must not
    // delete files in the folder another machine owns.
    if data_dir_write_blocked_quiet() {
        return (0, 0);
    }
    sweep_stale_recordings_in(&data_dir(app).join("recordings"), referenced, &[], broken_sweep_on, broken_grace, retention_age)
}

// Split out from sweep_stale_recordings so it is testable against a plain tempdir. Three
// independent reasons to remove a file: it is old and nobody has decided on it, it is a
// broken (moov-less) mp4 past a much shorter grace period, or it is past the retention
// window (90 days unless changed). Every path in protected (the current recording's output
// and mic sidecar, the video editor's open source and its sidecar) is excluded
// unconditionally, since deleting a file mid-capture or under an open editor is far worse.
// Retention only touches unreferenced files: a recording attached to a saved prompt is
// never a candidate.
pub(crate) fn sweep_stale_recordings_in(
    dir: &Path,
    referenced: &std::collections::HashSet<String>,
    protected: &[PathBuf],
    broken_sweep_on: bool,
    broken_grace: Duration,
    retention_age: Option<Duration>,
) -> (u32, u64) {
    let Ok(entries) = fs::read_dir(dir) else { return (0, 0) };
    let norm = pending_key;
    let mut count = 0u32;
    let mut bytes = 0u64;
    for e in entries.flatten() {
        let path = e.path();
        if !path.is_file() || protected.iter().any(|p| p == &path) || camera::is_kept_abandoned(&path) {
            continue;
        }
        // A "<name>.mic.wav" sidecar is referenced whenever its recording is (the mp4 may be
        // attached while the editor has not yet merged the two tracks). A "<name>.cam.json" sidecar
        // follows the same rule for its video (webm or mp4), or it would age out while the
        // recording is still attached.
        let is_referenced = referenced.contains(&norm(&path.to_string_lossy()))
            || mic_sidecar_video_path(&path).is_some_and(|v| referenced.contains(&norm(&v.to_string_lossy())))
            || keys_sidecar_video_path(&path).is_some_and(|v| referenced.contains(&norm(&v.to_string_lossy())))
            || camera::sidecar_video_path(&path).is_some_and(|v| referenced.contains(&norm(&v.to_string_lossy())));
        let Ok(meta) = e.metadata() else { continue };
        let age = meta.modified().ok().and_then(|t| SystemTime::now().duration_since(t).ok());
        let stale_undecided = !is_referenced && age.is_some_and(|a| a >= STALE_TEMP_FILE_AGE);
        let broken = broken_sweep_on && !is_referenced && age.is_some_and(|a| a >= broken_grace) && recording_is_broken(&path);
        let aged_out = !is_referenced && retention_age.is_some_and(|r| age.is_some_and(|a| a >= r));
        if stale_undecided || broken || aged_out {
            let size = meta.len();
            if fs::remove_file(&path).is_ok() {
                count += 1;
                bytes += size;
            }
        }
    }
    (count, bytes)
}

// Startup's one-shot result: Db is not managed yet in setup() and no window exists to toast
// into, so the frontend collects it early in its init via take_recording_sweep_notice().
pub(crate) static PENDING_SWEEP_NOTICE: Mutex<Option<(u32, u64)>> = Mutex::new(None);

#[tauri::command]
pub(crate) fn take_recording_sweep_notice() -> Option<(u32, u64)> {
    PENDING_SWEEP_NOTICE.lock().unwrap_or_else(|e| e.into_inner()).take()
}

// How long a recording may sit unfinished before the sweep treats it as broken
// (minutes), and how often the periodic sweep runs (minutes).
pub(crate) const REC_BROKEN_GRACE_MIN: f64 = 5.0;
pub(crate) const REC_SWEEP_EVERY_MIN: f64 = 60.0;

pub(crate) fn recording_sweep_grace(app: &AppHandle) -> (bool, Duration) {
    app.try_state::<Db>()
        .map(|s| {
            let store = s.lock().unwrap_or_else(|e| e.into_inner());
            let on = store.settings.ui_flags.get("recSweepBroken") != Some(&false);
            (on, Duration::from_secs((REC_BROKEN_GRACE_MIN * 60.0) as u64))
        })
        .unwrap_or((true, Duration::from_secs(5 * 60)))
}

// How long a KEPT recording (referenced or not) is allowed to age before
// an unreferenced one becomes fair game for the sweep above: 0 disables this
// leg; a new install ships 90 days.
pub(crate) fn recording_retention_age(app: &AppHandle) -> Option<Duration> {
    let days = app
        .try_state::<Db>()
        .map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).settings.ui_values.get("recordingRetentionDays").copied().unwrap_or(RECORDING_RETENTION_DAYS_DEFAULT))
        .unwrap_or(0.0);
    if days <= 0.0 {
        None
    } else {
        Some(Duration::from_secs((days as u64).saturating_mul(86_400)))
    }
}

// Runs every REC_SWEEP_EVERY_MIN minutes.
pub(crate) fn recording_sweep_interval() -> Duration {
    Duration::from_secs((REC_SWEEP_EVERY_MIN * 60.0) as u64)
}

// Shared by the periodic job and "once a recording ends": both run after
// Db/RecordingState are managed and a window exists, so both can just
// emit: unlike the startup path above, which cannot.
pub(crate) fn sweep_recordings_and_notify(app: &AppHandle) {
    let (count, bytes) = sweep_recordings_now_result(app);
    if count > 0 {
        append_crash_log(&format!("recording sweep: removed {count} file(s), {bytes} bytes"));
        let _ = app.emit("recording-sweep-done", serde_json::json!({ "count": count, "bytes": bytes }));
    }
}

pub(crate) fn sweep_recordings_now_result(app: &AppHandle) -> (u32, u64) {
    if data_dir_write_blocked_quiet() {
        return (0, 0);
    }
    let Some(state) = app.try_state::<Db>() else { return (0, 0) };
    let referenced = orphan_referenced_set(&lock(&state));
    let (broken_on, grace) = recording_sweep_grace(app);
    let (mut count, mut bytes) = sweep_stale_recordings_in(
        &data_dir(app).join("recordings"),
        &referenced,
        &protected_recording_paths(app),
        broken_on,
        grace,
        recording_retention_age(app),
    );
    let (c, b) = sweep_discarded_recordings(app);
    count += c;
    bytes += b;
    (count, bytes)
}

// A discarded recording sits in its folder's "verworfen" subfolder and is stale-eligible
// past recDiscardKeepH with no referenced check (a discarded recording is attached to
// nothing). recDiscardKeepH <= 0 disables this leg. Covers both places a working recording
// can live (data_dir's recordings folder and the current recordingDir), since
// discard_recording_file accepts either.
pub(crate) fn sweep_discarded_recordings(app: &AppHandle) -> (u32, u64) {
    if data_dir_write_blocked_quiet() {
        return (0, 0);
    }
    let Some(state) = app.try_state::<Db>() else { return (0, 0) };
    let (keep_h, custom) = {
        let g = lock(&state);
        (g.settings.ui_values.get("recDiscardKeepH").copied().unwrap_or(24.0), g.settings.ui_texts.get("recordingDir").cloned().unwrap_or_default())
    };
    if keep_h <= 0.0 {
        return (0, 0);
    }
    let age = Duration::from_secs((keep_h * 3600.0) as u64);
    let default_dir = data_dir(app).join("recordings");
    let custom_dir = recording_dir(&custom);
    let mut count = 0u32;
    let mut bytes = 0u64;
    for dir in [default_dir.clone(), custom_dir].into_iter().collect::<std::collections::HashSet<_>>() {
        let (c, b) = discarded_stale_in(&dir.join(DISCARD_SUBDIR), age);
        count += c;
        bytes += b;
    }
    (count, bytes)
}

// The discarded-recordings row's on-demand trigger. sweep_recordings_now answers for the
// recordings row and also runs this sweep in its combined total, so a dedicated command
// keeps the two rows from double-reporting.
#[tauri::command(async)]
pub(crate) fn sweep_discarded_recordings_now(app: AppHandle) -> (u32, u64) {
    if data_dir_write_blocked(&app) {
        return (0, 0);
    }
    sweep_discarded_recordings(&app)
}

pub(crate) fn discarded_stale_in(dir: &Path, age: Duration) -> (u32, u64) {
    let Ok(entries) = fs::read_dir(dir) else { return (0, 0) };
    let mut count = 0u32;
    let mut bytes = 0u64;
    for e in entries.flatten() {
        let path = e.path();
        if !is_video_folder_file(&path) { continue; }
        let Ok(meta) = e.metadata() else { continue };
        let Some(elapsed) = meta.modified().ok().and_then(|t| SystemTime::now().duration_since(t).ok()) else { continue };
        if elapsed < age { continue; }
        let size = meta.len();
        if fs::remove_file(&path).is_ok() {
            count += 1;
            bytes += size;
            let _ = fs::remove_file(path.with_extension("mic.wav"));
            // The key-overlay sidecar goes with its video.
            let _ = fs::remove_file(sidecar_path(&path, KEYS_SIDECAR_EXT));
            let _ = fs::remove_file(sidecar_path(&path, CAM_SIDECAR_EXT));
        }
    }
    (count, bytes)
}

// The "clean up now" button: runs the same sweep on demand and always reports what it did,
// even nothing, unlike the silent background sweep.
#[tauri::command(async)]
pub(crate) fn sweep_recordings_now(app: AppHandle) -> (u32, u64) {
    // Raises the read-only notice for a click; the shared sweep below only skips.
    if data_dir_write_blocked(&app) {
        return (0, 0);
    }
    sweep_recordings_now_result(&app)
}

// Every path the sweep must never touch beyond what referenced covers: the active
// recording's output and mic sidecar (no db entry yet), and the video editor's open source
// and its not-yet-muxed sidecar, which would otherwise be fair game after the stale/broken
// grace period.
pub(crate) fn protected_recording_paths(app: &AppHandle) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(p) = active_recording_path(app) {
        push_recording_with_sidecars(&mut out, &p);
    }
    out.extend(protected_take_paths());
    // The live camera take: unreferenced and, while paused, not growing, so its age alone would
    // make it fair game for the sweep a screen recording's stop triggers.
    if let Some(cam) = app.try_state::<camera::CameraState>() {
        out.extend(camera::live_session_paths(&cam.0.lock().unwrap_or_else(|e| e.into_inner())));
    }
    let mut editor_sources = Vec::new();
    if let Some(state) = app.try_state::<VideoEditorPending>() {
        editor_sources.push(state.lock().clone());
    }
    // A source offered to the open editor is protected next to the one it shows.
    editor_sources.extend(VE_PENDING_REQUEST.lock().unwrap_or_else(|e| e.into_inner()).clone());
    for (src, _origin_dir, overlay) in editor_sources {
        if !src.is_empty() {
            push_recording_with_sidecars(&mut out, Path::new(&src));
        }
        if !overlay.is_empty() {
            let overlay = PathBuf::from(overlay);
            out.push(sidecar_path(&overlay, CAM_SIDECAR_EXT));
            out.push(overlay);
        }
    }
    out
}

// Earlier parts of a "finish and continue" take that the result dialog is
// deciding on. None is referenced by a prompt yet and they can be older than
// the sweep age, so the sweep must leave them alone until the dialog has
// saved or discarded them.
pub(crate) static REC_RESULT_PARTS: Mutex<Vec<RecordingPartInfo>> = Mutex::new(Vec::new());

// The same parts while a Save copies them: taken out of the list above for the duration, but
// still not the sweep's to delete.
pub(crate) static REC_PARTS_SAVING: Mutex<Vec<RecordingPartInfo>> = Mutex::new(Vec::new());

// The file the result dialog currently shows (the raw capture, then the
// converted copy). It sits unreferenced for as long as the dialog stays open,
// which can outlast the sweep age.
pub(crate) static REC_RESULT_PENDING: Mutex<Option<PathBuf>> = Mutex::new(None);

fn push_recording_with_sidecars(out: &mut Vec<PathBuf>, video: &Path) {
    out.push(video.with_extension("mic.wav"));
    #[cfg(windows)]
    out.push(sidecar_path(video, KEYS_SIDECAR_EXT));
    out.push(sidecar_path(video, CAM_SIDECAR_EXT));
    out.push(video.to_path_buf());
}

// Parts of a chain that is still recording, parts waiting in the result
// dialog and the file the dialog shows, each with its sidecars.
pub(crate) fn protected_take_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let parts: Vec<String> = REC_SESSION_PARTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .chain(REC_RESULT_PARTS.lock().unwrap_or_else(|e| e.into_inner()).iter())
        .chain(REC_PARTS_SAVING.lock().unwrap_or_else(|e| e.into_inner()).iter())
        .flat_map(|p| [Some(p.path.clone()), p.mic_wav_path.clone()])
        .flatten()
        .collect();
    for p in parts {
        push_recording_with_sidecars(&mut out, Path::new(&p));
    }
    if let Some(p) = REC_RESULT_PENDING.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        push_recording_with_sidecars(&mut out, p);
    }
    out
}

// The result dialog reports the file it shows on open and after every change
// of that path (conversion, move), and null when it closes.
#[tauri::command]
pub(crate) fn set_pending_recording(path: Option<String>) {
    *REC_RESULT_PENDING.lock().unwrap_or_else(|e| e.into_inner()) = path.filter(|p| !p.is_empty()).map(PathBuf::from);
}

#[cfg(windows)]
pub(crate) fn active_recording_path(app: &AppHandle) -> Option<PathBuf> {
    app.try_state::<RecordingState>()?.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|s| s.output_path.clone())
}

// A trashed prompt owns its files until purged: leaving trash out of this scan would let an
// unrelated cleanup sweep away a file a trashed prompt needs to be restorable.
pub(crate) fn orphan_referenced_set(store: &Store) -> std::collections::HashSet<String> {
    let norm = pending_key;
    store
        .prompts
        .iter()
        .chain(store.trash.iter())
        .flat_map(|p| [p.file_path.clone(), p.icon_path.clone()])
        .filter(|s| !s.is_empty())
        .map(|s| norm(&s))
        .collect()
}

// Async: persisting all prompts (base64 images) to SQLite must not block the UI thread.
// Every field the editor form can set arrives as its own parameter, mirroring update_prompt
// one to one.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn add_prompt(
    app: AppHandle,
    state: State<'_, Db>,
    name: String,
    text: String,
    color: String,
    image: Option<String>,
    show_image: Option<bool>,
    copy_image: Option<bool>,
    file_path: Option<String>,
    pdf_page: Option<u32>,
    icon_path: Option<String>,
    caption: Option<String>,
    caption_size: Option<u32>,
    font: Option<String>,
    font_size: Option<u32>,
    favorite: Option<bool>,
    tags: Option<Vec<String>>,
    notes: Option<String>,
    hotkey: Option<String>,
    hotkey_paste: Option<String>,
    variants: Option<Vec<String>>,
    rotate_mode: Option<String>,
    rotate_auto: Option<bool>,
    // Create the entry already confidential: sealed before its first write, so the
    // plain text never reaches the database, a backup or the Markdown sync folder.
    secret: Option<bool>,
) -> Result<Prompt, String> {
    // Text from outside (a pasted prompt, an old client) may still use the retired
    // placeholder spelling; the stored prompt never does.
    let text = upgrade_placeholders(&text);
    let variants = variants.map(|v| v.iter().map(|s| upgrade_placeholders(s)).collect::<Vec<String>>());
    // The slow steps (copying a large attachment, ffprobe, OCR) run before the store lock is
    // taken so other windows keep working; the lock is retaken only to validate and insert.
    let (store_files, ocr_index_on, max_chars) = {
        let store = lock(&state);
        (
            store_files_enabled(&store),
            store.settings.ui_flags.get("ocrIndex").copied().unwrap_or(false),
            OCR_INDEX_MAX_CHARS,
        )
    };
    // A path naming another machine or the app's own store files is refused before any copy
    // or probe touches it (the editor's pickers never produce one).
    let store_dir = data_dir(&app);
    let own_dirs = own_storage_dirs(&app, &state);
    for p in [file_path.as_deref(), icon_path.as_deref()].into_iter().flatten() {
        if attachment_path_refused(&own_dirs, &store_dir, p) || !crate::asset_scope::source_trusted_now(&app, &state, p) {
            return Err("source-not-allowed".to_string());
        }
    }
    let mut fp = file_path.unwrap_or_default();
    let mut ip = icon_path.unwrap_or_default();
    let mut staged_copies = StagedCopies::new();
    if store_files {
        let files_dir = data_dir(&app).join("files");
        fp = staged_copies.stage(&files_dir, &fp);
        ip = staged_copies.stage(&files_dir, &ip);
    }
    // Measured once, right here, never again on a render.
    let media_source = if !fp.is_empty() { Some(fp.as_str()) } else if !ip.is_empty() { Some(ip.as_str()) } else { None };
    let (media_duration_ms, media_width, media_height, media_size_bytes) = match media_source {
        Some(p) => probe_media_meta(ffprobe_search_path(&app).as_deref(), Path::new(p)),
        None => (0, 0, 0, 0),
    };
    let image_value = image.unwrap_or_default();
    // Read and indexed once, here: an OCR pass on every render would defeat the search index.
    let image_text = resolved_image_text(&image_value, ocr_index_on, |img| ocr_index_text(img, max_chars));
    let prompt = {
        let mut store = lock(&state);
        let tags = sanitize_tags(tags.unwrap_or_default(), &canonical_tags(&store.prompts))?;
        let notes = sanitize_notes(&notes.unwrap_or_default())?;
        // The OCR above ran against a flag snapshot: text only exists while the index is on.
        let image_text = if ocr_index_flag_on(&store) { image_text } else { String::new() };
        let variants = sanitize_variants(variants.unwrap_or_default());
        let rotate_mode = sanitize_rotate_mode(rotate_mode.unwrap_or_default(), &variants);
        let rotate_auto = sanitize_rotate_auto(rotate_auto.unwrap_or(false), &variants);
        let others: Vec<&Prompt> = store.prompts.iter().collect();
        let rec_stop_hk = active_rec_hotkey(&store.settings.rec_stop_hotkey, "recStopHotkey", &store.settings.ui_flags);
        let rec_pause_hk = active_rec_hotkey(&store.settings.rec_pause_hotkey, "recPauseHotkey", &store.settings.ui_flags);
        let voice_hk = active_voice_hotkey(&store.settings.voice_hotkey, &store.settings.ui_flags);
        let hotkey = sanitize_hotkey(&hotkey.unwrap_or_default(), &store.settings.hotkey, voice_hk, rec_stop_hk, rec_pause_hk, &others)?;
        let hotkey_paste = if hotkey.is_empty() {
            String::new() // no binding, no action to remember for it
        } else {
            sanitize_hotkey_mode(&hotkey_paste.unwrap_or_default())
        };
        let color = valid_tag_color(&color)?;
        let prompt = Prompt {
            id: gen_id(),
            trashed_at: 0,
            name,
            text,
            color,
            image: image_value,
            show_image: show_image.unwrap_or(false),
            copy_image: copy_image.unwrap_or(false),
            file_path: fp,
            pdf_page: pdf_page.unwrap_or(0),
            icon_path: ip,
            caption: caption.unwrap_or_default(),
            caption_size: clamp_caption_size(caption_size.unwrap_or(0)),
            font: font.unwrap_or_default(),
            font_size: clamp_font_size(font_size.unwrap_or(0)),
            favorite: favorite.unwrap_or(false),
            tags,
            notes,
            hotkey,
            hotkey_paste,
            variants,
            rotate_mode,
            rotate_auto,
            media_duration_ms,
            media_width,
            media_height,
            media_size_bytes,
            image_text,
            secret: false,
        };
        let prompt = if secret == Some(true) {
            let Some(key) = secret_session_key(secret_timeout_min(&store.settings)) else {
                return Err("secret-locked".to_string());
            };
            secret_convert_prompt(&prompt, true, &key)?.unwrap_or(prompt)
        } else {
            prompt
        };
        add_placed_prompt_core(&mut store, &prompt, |st, original| {
            save_settings_then_rows(&app, st, original, |s| save_prompts(&app, s))
        })?;
        // After the pair landed: an early return above drops the staged copies uncommitted, which
        // removes them again.
        staged_copies.commit();
        prompt
    };
    rebuild_tray(&app);
    // Arm the new accelerator. The re-arm drops one the OS refuses, so the
    // caller is handed the stored state, not the state before arming.
    if !prompt.hotkey.is_empty() {
        rearm_hotkeys(&app);
        let store = lock(&state);
        if let Some(fresh) = store.prompts.iter().find(|p| p.id == prompt.id) {
            return Ok(fresh.clone());
        }
    }
    Ok(prompt)
}

// Seats a new prompt in the next free cell of the active view and persists the settings row and
// the prompt rows as one unit (`save` gets the store and the settings from before the call). When
// either write fails the store is put back exactly as it was and nothing is left half-saved: no
// prompt without its cell, no cell for a prompt that is not on disk.
pub(crate) fn add_placed_prompt_core(store: &mut Store, prompt: &Prompt, save: impl FnOnce(&Store, &Settings) -> bool) -> Result<(), String> {
    let original_settings = store.settings.clone();
    // A grid folder owns its cell outside view.layouts and the renderer prefers the folder, so a
    // cell handed out without excluding folder cells saves fine and never draws.
    let folder_cells = active_view_folder_cells(&store.settings);
    let view = store.settings.active_view_mut();
    if let Some(cell) = first_free_cell_excluding(view, &folder_cells) {
        let key = grid_key(view.cols, view.rows);
        view.layouts.entry(key).or_default().insert(prompt.id.clone(), cell);
    }
    view.members.push(prompt.id.clone());
    store.prompts.push(prompt.clone());
    if !save(store, &original_settings) {
        // Reported instead of handing back an id the editor believes is saved.
        store.prompts.pop();
        store.settings = original_settings;
        return Err("save-failed".into());
    }
    Ok(())
}

// One example prompt as the frontend supplies it: name/text come from i18n.js
// (resolved to whatever language the UI is actually showing), the image is the
// one small embedded asset shared by all languages.
#[derive(Deserialize)]
pub(crate) struct NewExample {
    pub(crate) name: String,
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) image: String,
}

// Seeds the three onboarding examples once, on a genuinely fresh install. Checked and
// flipped atomically under one lock so racing calls (a double invoke from a slow first
// paint) cannot seed twice. A library emptied again by the user does not reseed;
// examples_seeded remembers.
#[tauri::command]
pub(crate) async fn seed_example_prompts(app: AppHandle, state: State<'_, Db>, examples: Vec<NewExample>) -> Result<Vec<Prompt>, String> {
    let created = {
        let mut store = lock(&state);
        if !store.prompts.is_empty() || store.settings.examples_seeded {
            return Ok(Vec::new());
        }
        let mut created = Vec::new();
        for ex in examples {
            let prompt = Prompt {
                id: gen_id(),
                trashed_at: 0,
                name: ex.name,
                text: ex.text,
                color: String::new(),
                show_image: !ex.image.is_empty(),
                image: ex.image,
                copy_image: false,
                file_path: String::new(),
                pdf_page: 0,
                icon_path: String::new(),
                caption: String::new(),
                caption_size: 0,
                font: String::new(),
                font_size: 0,
                favorite: false,
                tags: Vec::new(),
                notes: String::new(),
                hotkey: String::new(),
                hotkey_paste: String::new(),
                variants: Vec::new(),
                rotate_mode: String::new(),
                rotate_auto: false,
                media_duration_ms: 0,
                media_width: 0,
                media_height: 0,
                media_size_bytes: 0,
                // Onboarding examples are never OCR-indexed: ocrIndex ships off,
                // and seeding runs before the user has ever seen the switch.
                image_text: String::new(),
                secret: false,
            };
            created.push(prompt);
        }
        seed_examples_core(&mut store, created, |st, original| {
            save_settings_then_rows(&app, st, original, |s| save_prompts(&app, s))
        })?
    };
    rebuild_tray(&app); // Mirrors add_prompt: the tray's recent/fav sections can list these too
    Ok(created)
}

// Places the seeded examples in the active view, flags the library as seeded and persists the
// settings row and the prompt rows as one unit. A failed write puts the store back exactly as it
// was (nothing seeded, no placement), so a retry cannot duplicate a batch that half landed.
pub(crate) fn seed_examples_core(store: &mut Store, created: Vec<Prompt>, save: impl FnOnce(&Store, &Settings) -> bool) -> Result<Vec<Prompt>, String> {
    let original_settings = store.settings.clone();
    for prompt in &created {
        let view = store.settings.active_view_mut();
        if let Some(cell) = first_free_cell(view) {
            let key = grid_key(view.cols, view.rows);
            view.layouts.entry(key).or_default().insert(prompt.id.clone(), cell);
        }
        view.members.push(prompt.id.clone());
        store.settings.example_prompt_ids.push(prompt.id.clone());
        store.settings.example_prompt_seeds.insert(
            prompt.id.clone(),
            example_seed_fingerprint(&prompt.name, &prompt.text, &prompt.image),
        );
        store.prompts.push(prompt.clone());
    }
    store.settings.examples_seeded = true;
    if !save(store, &original_settings) {
        let ids: std::collections::HashSet<&str> = created.iter().map(|p| p.id.as_str()).collect();
        store.prompts.retain(|p| !ids.contains(p.id.as_str()));
        store.settings = original_settings;
        return Err("save-failed".into());
    }
    Ok(created)
}

// Of ids (normally settings.example_prompt_ids), which are still unedited. No entry in
// seeds (seeded before the field existed) counts as unchanged, as does an id whose prompt
// is already gone. Shared by dismiss_example_prompts and example_prompts_pending so they
// cannot disagree.
pub(crate) fn examples_still_pristine<'a>(
    ids: impl IntoIterator<Item = &'a String>,
    seeds: &HashMap<String, u64>,
    prompts: &[Prompt],
) -> Vec<String> {
    ids.into_iter()
        .filter(|id| match seeds.get(*id) {
            None => true,
            Some(&seed) => prompts
                .iter()
                .find(|p| &p.id == *id)
                .map(|p| example_seed_fingerprint(&p.name, &p.text, &p.image) == seed)
                .unwrap_or(true),
        })
        .cloned()
        .collect()
}

// "Remove examples": deletes exactly the ids seed_example_prompts recorded that the user has
// not edited, through the same batch-delete path (to trash) as a manual multi-select
// delete.
#[tauri::command]
pub(crate) async fn dismiss_example_prompts(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    let (ids_to_delete, taken_ids, taken_seeds) = {
        let mut store = lock(&state);
        let ids = std::mem::take(&mut store.settings.example_prompt_ids);
        let seeds = std::mem::take(&mut store.settings.example_prompt_seeds);
        (examples_still_pristine(&ids, &seeds, &store.prompts), ids, seeds)
    };
    // The lists were cleared in memory above; nothing else may write the settings soon, so
    // persist the cleared lists here or they come back on the next start.
    let persist = |app: &AppHandle| {
        let store = lock(&state);
        save_settings(app, &store.settings);
    };
    if ids_to_delete.is_empty() {
        persist(&app);
        return Ok(());
    }
    let app_for_save = app.clone();
    let result = batch_prompts(app, state.clone(), ids_to_delete, "delete".to_string(), None, None, None, None, None)
        .await
        .map(|_| ());
    if result.is_err() {
        // The delete did not happen: the examples stay, so the lists naming them must
        // too, or "Remove examples" could never be retried.
        let mut store = lock(&state);
        store.settings.example_prompt_ids = taken_ids;
        store.settings.example_prompt_seeds = taken_seeds;
    } else {
        persist(&app_for_save);
    }
    result
}

// The ids "Remove examples" would actually delete now: the unedited ones that still exist.
// examples_still_pristine alone also lists an id whose prompt the user already deleted by hand,
// which would inflate the button's count.
pub(crate) fn examples_pending_ids<'a>(
    ids: impl IntoIterator<Item = &'a String>,
    seeds: &HashMap<String, u64>,
    prompts: &[Prompt],
) -> Vec<String> {
    let mut out = examples_still_pristine(ids, seeds, prompts);
    out.retain(|id| prompts.iter().any(|p| &p.id == id));
    out
}

// Count for the "Remove examples (n)" button label: how many seeded examples would
// actually be deleted right now, same rule dismiss_example_prompts itself applies.
#[tauri::command(async)]
pub(crate) fn example_prompts_pending(state: State<Db>) -> usize {
    let store = lock(&state);
    examples_pending_ids(&store.settings.example_prompt_ids, &store.settings.example_prompt_seeds, &store.prompts).len()
}

// A save carries the image only when the user touched it: no field means leave the stored
// picture alone, and the editor's remove button sends an explicit empty string. Images do
// not ride along with every read of the list, so a save must not mistake "not loaded here"
// for "deleted".
pub(crate) fn merge_image(stored: &str, incoming: Option<String>) -> String {
    match incoming {
        Some(img) => img,
        None => stored.to_string(),
    }
}

// What an edit does to a prompt's text and rotation entries. `None` keeps what is
// stored, a confidential entry's ciphertext included (so it needs no key and a
// caller that never held the real text cannot wipe it); `Some` is new plain
// content, sealed here for a confidential entry and refused while it is locked.
pub(crate) fn edited_text_and_variants(
    p: &Prompt,
    text: Option<String>,
    variants: Option<Vec<String>>,
    secret_key: Option<[u8; 32]>,
) -> Result<(String, Vec<String>), String> {
    if text.is_none() && variants.is_none() {
        return Ok((p.text.clone(), p.variants.clone()));
    }
    if !p.secret {
        return Ok((text.unwrap_or_else(|| p.text.clone()), variants.unwrap_or_else(|| p.variants.clone())));
    }
    // The editor's plain value is re-encrypted before it touches the entry; while
    // locked there is no key, so the whole save is refused rather than silently
    // discarding the real ciphertext.
    let Some(key) = secret_key else {
        return Err("secret-locked".to_string());
    };
    // The stored ciphertext must open with this key: a different vault's entry would
    // otherwise be replaced for good by whatever the editor shows.
    if reveal_secret_text(p, Some(key)).is_err() {
        return Err("secret-wrong-key".to_string());
    }
    Ok((
        text.map_or_else(|| p.text.clone(), |t| secret_encrypt(&t, &key)),
        variants.map_or_else(|| p.variants.clone(), |v| v.iter().map(|s| secret_encrypt(s, &key)).collect()),
    ))
}

// The one prompt as the window's list holds it (see slim_prompts): a confidential
// entry arrives decrypted while the vault is open and empty while it is locked,
// never as the sealed text the store keeps.
fn slim_one(state: &State<'_, Db>, p: &Prompt) -> SlimPrompt {
    let key = {
        let store = lock(state);
        secret_session_key(secret_timeout_min(&store.settings))
    };
    slim_prompts(std::slice::from_ref(p), key).remove(0)
}

// Async: same reason as add_prompt, and one parameter per editable field.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn update_prompt(
    app: AppHandle,
    state: State<'_, Db>,
    id: String,
    name: String,
    // Absent text/variants keeps what is stored, so a write that changes one flag (a hotkey,
    // auto-rotate) cannot replace a confidential entry's real text with a cache.
    text: Option<String>,
    color: String,
    // Absent keeps the stored image. The editor sends it only when the picture changed, so an
    // image that never loaded cannot erase the stored one; an explicit Some("") is the
    // remove-image click.
    image: Option<String>,
    show_image: Option<bool>,
    copy_image: Option<bool>,
    file_path: Option<String>,
    pdf_page: Option<u32>,
    icon_path: Option<String>,
    caption: Option<String>,
    caption_size: Option<u32>,
    font: Option<String>,
    font_size: Option<u32>,
    favorite: Option<bool>,
    tags: Option<Vec<String>>,
    notes: Option<String>,
    hotkey: Option<String>,
    hotkey_paste: Option<String>,
    variants: Option<Vec<String>>,
    rotate_mode: String,
    // Option, unlike rotate_mode: a caller that omits it must not reset an
    // unrelated field, so a missing value keeps the prompt's own.
    rotate_auto: Option<bool>,
) -> Result<Option<SlimPrompt>, String> {
    // Upgraded before anything below (the version snapshot included) sees the text.
    let text = text.map(|t| upgrade_placeholders(&t));
    let variants = variants.map(|v| sanitize_variants(v.iter().map(|s| upgrade_placeholders(s)).collect()));
    // Copying an attachment, probing it and OCR-ing a new picture happen before the store lock,
    // against a short snapshot of what the prompt holds. The locked section uses those results
    // only if the prompt still holds exactly that; a concurrent edit falls back to doing the
    // work inline.
    let (store_files, staged_ocr_on, staged_ocr_max, staged_cur) = {
        let store = lock(&state);
        let cur = store.prompts.iter().find(|p| p.id == id).map(|p| (p.image.clone(), p.file_path.clone(), p.icon_path.clone()));
        (
            store_files_enabled(&store),
            store.settings.ui_flags.get("ocrIndex").copied().unwrap_or(false),
            OCR_INDEX_MAX_CHARS,
            cur,
        )
    };
    // A path naming another machine or the app's own store files is refused before any copy
    // or probe touches it (the editor's pickers never produce one). A value the prompt already
    // holds is not asked again: an unrelated edit sends the stored path back unchanged, and
    // that must not fail because the file has since moved to a place the gate refuses.
    let store_dir = data_dir(&app);
    let own_dirs = own_storage_dirs(&app, &state);
    let held_fp = staged_cur.as_ref().map(|c| c.1.as_str());
    let held_ip = staged_cur.as_ref().map(|c| c.2.as_str());
    // Per path: true = unchanged but now refused (kept as stored, never copied or probed).
    let mut keep_as_is = [false, false];
    for (i, (p, held)) in [(file_path.as_deref(), held_fp), (icon_path.as_deref(), held_ip)].into_iter().enumerate() {
        let Some(p) = p else { continue };
        if attachment_path_refused(&own_dirs, &store_dir, p) {
            if held != Some(p) {
                return Err("source-not-allowed".to_string());
            }
            keep_as_is[i] = true;
        } else if held != Some(p) && !crate::asset_scope::source_trusted_now(&app, &state, p) {
            // A new source must be an app folder or a file the user picked; the value the prompt
            // already holds is never asked again.
            return Err("source-not-allowed".to_string());
        }
    }
    let mut staged_copies = StagedCopies::new();
    let files_dir = store_dir.join("files");
    let file_path = file_path.map(|p| if store_files && !keep_as_is[0] { staged_copies.stage(&files_dir, &p) } else { p });
    let icon_path = icon_path.map(|p| if store_files && !keep_as_is[1] { staged_copies.stage(&files_dir, &p) } else { p });
    let ffprobe = ffprobe_search_path(&app);
    let staged_image_text = staged_cur.as_ref().and_then(|(cur_img, _, _)| {
        image.as_deref().is_some_and(|v| v != cur_img).then(|| {
            resolved_image_text(&merge_image(cur_img, image.clone()), staged_ocr_on, |img| ocr_index_text(img, staged_ocr_max))
        })
    });
    let staged_media = staged_cur.as_ref().and_then(|(_, cur_fp, cur_ip)| {
        let changed = file_path.as_deref().is_some_and(|v| v != cur_fp) || icon_path.as_deref().is_some_and(|v| v != cur_ip);
        if !changed {
            return None;
        }
        let fp_now = file_path.clone().unwrap_or_else(|| cur_fp.clone());
        let ip_now = icon_path.clone().unwrap_or_else(|| cur_ip.clone());
        // A kept refused value is never probed either (a share would be contacted).
        let usable = |p: &String| !p.is_empty() && !attachment_path_refused(&own_dirs, &store_dir, p);
        let source = if usable(&fp_now) { Some(fp_now) } else if usable(&ip_now) { Some(ip_now) } else { None };
        Some(match source {
            Some(mp) => probe_media_meta(ffprobe.as_deref(), Path::new(&mp)),
            None => (0, 0, 0, 0),
        })
    });
    let updated = {
        let mut store = lock(&state);
        let tags = match tags {
            Some(list) => Some(sanitize_tags(list, &canonical_tags(&store.prompts))?),
            None => None,
        };
        let notes = match notes {
            Some(text) => Some(sanitize_notes(&text)?),
            None => None,
        };
        let hotkey = match hotkey {
            Some(raw) => {
                let others: Vec<&Prompt> = store.prompts.iter().filter(|p| p.id != id).collect();
                let rec_stop_hk = active_rec_hotkey(&store.settings.rec_stop_hotkey, "recStopHotkey", &store.settings.ui_flags);
                let rec_pause_hk = active_rec_hotkey(&store.settings.rec_pause_hotkey, "recPauseHotkey", &store.settings.ui_flags);
                let voice_hk = active_voice_hotkey(&store.settings.voice_hotkey, &store.settings.ui_flags);
                Some(sanitize_hotkey(&raw, &store.settings.hotkey, voice_hk, rec_stop_hk, rec_pause_hk, &others)?)
            }
            None => None,
        };
        // Read before the mutable prompt borrow (a MutexGuard cannot be split into disjoint field
        // borrows); only run if the attachment changed.
        let ocr_index_on = store.settings.ui_flags.get("ocrIndex").copied().unwrap_or(false);
        let ocr_max_chars = OCR_INDEX_MAX_CHARS;
        // Read before the mutable prompt borrow, same reason.
        let hist_on = history_enabled(&store.settings);
        let hist_cap = history_cap(&store.settings);
        // Read before the mutable prompt borrow too: needed to re-encrypt a confidential entry's
        // edited text instead of overwriting its ciphertext.
        let secret_key = secret_session_key(secret_timeout_min(&store.settings));
        // The colour lands in a CSS property in every window: only #rrggbb (or empty) is stored.
        let color = valid_tag_color(&color)?;
        let found = store.prompts.iter_mut().find(|p| p.id == id);
        match found {
            Some(p) => {
                // Full pre-edit snapshot, so a failed save can roll the live entry back instead of leaving
                // unsaved changes on screen that revert on restart. Also the version-history source.
                let original = p.clone();
                // The staged results are valid only for the state they were computed against.
                let staged_valid = staged_cur.as_ref().is_some_and(|(i, f, c)| *i == p.image && *f == p.file_path && *c == p.icon_path);
                // A confidential entry's text/variants must never land in storage
                // as whatever the editor sent (see edited_text_and_variants).
                let (text, variants) = edited_text_and_variants(p, text, variants, secret_key)?;
                p.name = crate::md_sync::sync_clean_name(&name);
                p.text = text;
                p.color = valid_tag_color(&color).unwrap_or_else(|_| original.color.clone());
                p.variants = variants;
                p.rotate_mode = sanitize_rotate_mode(rotate_mode, &p.variants);
                if let Some(auto) = rotate_auto { p.rotate_auto = sanitize_rotate_auto(auto, &p.variants); }
                // Only a real change to the picture re-indexes it: cleared when the picture is gone, read
                // (while ocrIndex is on) when replaced, so an old picture's text never survives under a new
                // one. The read happened before the lock; when it no longer fits (the switch or the picture
                // changed in between) the text stays empty, since recognising here would block every
                // command that needs the store. The picture-text backfill fills it in later.
                let image_changed = image.as_deref().is_some_and(|v| v != p.image);
                p.image = merge_image(&p.image, image);
                if image_changed {
                    p.image_text = match &staged_image_text {
                        Some(t) if staged_valid && staged_ocr_on == ocr_index_on && staged_ocr_max == ocr_max_chars => t.clone(),
                        _ => String::new(),
                    };
                }
                if let Some(si) = show_image { p.show_image = si; }
                if let Some(ci) = copy_image { p.copy_image = ci; }
                // A real change to either path re-measures the attachment; a save that never touched it
                // must not re-run ffprobe on every save.
                let attachment_changed = file_path.as_deref().is_some_and(|v| v != p.file_path)
                    || icon_path.as_deref().is_some_and(|v| v != p.icon_path);
                if let Some(fp) = file_path { p.file_path = fp; }
                if let Some(pp) = pdf_page { p.pdf_page = pp; }
                if let Some(ip) = icon_path { p.icon_path = ip; }
                if attachment_changed {
                    let (d, w, h, sz) = match staged_media {
                        Some(m) if staged_valid => m,
                        _ => {
                            let media_source = if !p.file_path.is_empty() { Some(p.file_path.as_str()) } else if !p.icon_path.is_empty() { Some(p.icon_path.as_str()) } else { None };
                            match media_source {
                                Some(mp) => probe_media_meta(ffprobe.as_deref(), Path::new(mp)),
                                None => (0, 0, 0, 0),
                            }
                        }
                    };
                    p.media_duration_ms = d;
                    p.media_width = w;
                    p.media_height = h;
                    p.media_size_bytes = sz;
                }
                if let Some(c) = caption { p.caption = c; }
                if let Some(cs) = caption_size { p.caption_size = clamp_caption_size(cs); }
                if let Some(f) = font { p.font = f; }
                if let Some(fs) = font_size {
                    p.font_size = clamp_font_size(fs);
                }
                if let Some(fav) = favorite { p.favorite = fav; }
                if let Some(t) = tags { p.tags = t; }
                if let Some(n) = notes { p.notes = n; }
                if let Some(hp) = &hotkey_paste { p.hotkey_paste = sanitize_hotkey_mode(hp); }
                // Only a real change re-arms; every save would otherwise churn
                // through unregister/register for nothing.
                let rebind = matches!(&hotkey, Some(h) if *h != p.hotkey);
                if let Some(h) = hotkey { p.hotkey = h; }
                if p.hotkey.is_empty() { p.hotkey_paste.clear(); } // no binding, no action
                let clone = p.clone();
                // Nothing reached disk (or the JSON fallback) - restore the pre-edit
                // snapshot instead of handing back an id the editor will believe is
                // safely saved.
                if !save_prompts(&app, &store) {
                    if let Some(slot) = store.prompts.iter_mut().find(|x| x.id == id) {
                        *slot = original;
                    }
                    return Err("save-failed".into());
                }
                staged_copies.commit();
                // Snapshot the pre-edit name/text/variants only once the edit is durably saved: inserting
                // earlier left a version row for an edit that never happened when the save failed, and
                // could push real history past hist_cap. Entries are content, so editing only entry 2 of a
                // rotating prompt still gets a row. rotate_mode is excluded (order/random is a setting).
                if hist_on && (original.name != clone.name || original.text != clone.text || original.variants != clone.variants) {
                    match db_conn(&app) {
                        // A failure loses one version snapshot, not the saved edit; not worth failing the edit, but
                        // logged so a missing restore point is traceable.
                        Some(c) => {
                            if let Err(e) = db_add_version(&c, &id, now_secs(), &original.name, &original.text, &original.variants, hist_cap) {
                                append_crash_log(&format!("version history insert failed for prompt {id}: {e}"));
                            }
                        }
                        None => append_crash_log(&format!("version history insert skipped for prompt {id}: no DB connection")),
                    }
                }
                // Sweep the old attachment only once the new reference is durably saved, or a failed save
                // leaves the DB pointing at a deleted file. Only the cheap in-memory part runs under the
                // lock; a save that touched neither attachment nor picture skips the directory scan.
                let orphan_scan = (attachment_changed || image_changed)
                    .then(|| orphan_cleanup_enabled(&store).then(|| orphan_referenced_set(&store)))
                    .flatten();
                let pill_box = pill_box_of(&store.settings, &clone);
                Some((clone, pill_box, rebind, orphan_scan))
            }
            None => None,
        }
    };
    let (updated, pill_box, rebind, orphan_scan) = match updated {
        Some((p, s, r, o)) => (Some(p), s, r, o),
        None => (None, (0.0, 0.0), false, None),
    };
    // The actual filesystem sweep, deferred until the store lock (dropped
    // when the block above ended) is no longer held.
    if let Some(referenced) = orphan_scan {
        sweep_orphan_files(&app, &referenced);
    }
    // Re-arm outside the lock, then hand back what is actually stored: the re-arm
    // clears an accelerator the OS refused.
    let updated = match updated {
        Some(p) if rebind => {
            rearm_hotkeys(&app);
            let store = lock(&state);
            Some(store.prompts.iter().find(|x| x.id == p.id).cloned().unwrap_or(p))
        }
        other => other,
    };
    let slim = updated.as_ref().map(|p| slim_one(&state, p));
    if let Some(p) = &updated {
        let _ = app.emit("prompt-updated", p.clone());
        rebuild_tray(&app); // A rename shows up in the tray entries
        // An open pill switches between text pill and image box live.
        if let Some(win) = app.get_webview_window(&flabel(&p.id)) {
            let (w, h) = pill_box;
            let _ = win.set_size(tauri::LogicalSize::new(w, h));
        }
    }
    Ok(slim)
}

// List a prompt's version snapshots (newest first).
#[tauri::command]
pub(crate) async fn list_versions(app: AppHandle, state: State<'_, Db>, prompt_id: String) -> Result<Vec<PromptVersion>, String> {
    // Snapshots taken while the entry was confidential hold ciphertext and stay that way after
    // it is made plain. The history panel shows and restores plain text, so every sealed one is
    // opened here, or refused while the vault is locked.
    let (key, secret) = {
        let store = lock(&state);
        let secret = store.prompts.iter().find(|p| p.id == prompt_id).is_some_and(|p| p.secret);
        (secret_session_key(secret_timeout_min(&store.settings)), secret)
    };
    if secret && key.is_none() {
        return Err("secret-locked".to_string());
    }
    let mut list = db_conn(&app).map(|c| db_list_versions(&c, &prompt_id)).unwrap_or_default();
    for v in list.iter_mut() {
        (v.text, v.variants) = open_snapshot(&v.text, &v.variants, key)?;
    }
    Ok(list)
}

// The library's "last edited" sort mode. A prompt missing from the map has
// never had a version row (see db_last_edited): the frontend sorts those last.
#[tauri::command]
pub(crate) async fn last_edited_map(app: AppHandle) -> Result<std::collections::HashMap<String, u64>, String> {
    Ok(db_conn(&app).map(|c| db_last_edited(&c)).unwrap_or_default())
}

// Removes the newest version row of a prompt: the undo snapshot a restore took before its
// save failed.
pub(crate) fn db_drop_latest_version(conn: &Connection, prompt_id: &str) {
    if let Err(e) = conn.execute(
        "DELETE FROM prompt_versions WHERE id = (SELECT MAX(id) FROM prompt_versions WHERE prompt_id = ?1)",
        params![prompt_id],
    ) {
        append_crash_log(&format!("could not drop the undo snapshot of prompt {prompt_id}: {e}"));
    }
}

// Restore a snapshot. The current state is snapshotted first, so a restore is
// itself undoable and nothing is ever lost.
#[tauri::command]
pub(crate) async fn restore_version(app: AppHandle, state: State<'_, Db>, version_id: i64) -> Result<Option<Prompt>, String> {
    // Refused before the undo snapshot below is written into a folder another machine owns.
    if data_dir_write_blocked(&app) {
        return Err("readonly".into());
    }
    let restored = {
        let mut store = lock(&state);
        let record = match db_conn(&app).and_then(|c| db_get_version(&c, version_id)) {
            Some(r) => r,
            None => return Ok(None),
        };
        let (pid, name, text, variants) = record;
        let cap = history_cap(&store.settings);
        // Same "read before the mutable prompt borrow" rule as update_prompt.
        let secret_key = secret_session_key(secret_timeout_min(&store.settings));
        // Plain from here on: a sealed snapshot is opened (or refused while locked)
        // whatever the entry's flag says now, so ciphertext can never be written
        // into an entry that is not confidential.
        let (text, variants) = open_snapshot(&text, &variants, secret_key)?;
        if let Some(p) = store.prompts.iter_mut().find(|p| p.id == pid) {
            let original = p.clone();
            // A locked vault refuses before anything is written, so no snapshot row exists for a
            // restore that never happens.
            if p.secret && secret_key.is_none() {
                return Err("secret-locked".to_string());
            }
            // The undo snapshot of the current state must itself succeed before overwriting it, or a
            // restore could not be undone and "nothing is ever lost" would break. Inserted without
            // trimming: the history is cut to its cap only once the restore has saved, so a failed
            // one never evicts a real version.
            let snapshot_ok = db_conn_write(&app)
                .map(|c| db_insert_version(&c, &pid, now_secs(), &p.name, &p.text, &p.variants).is_ok())
                .unwrap_or(false);
            if !snapshot_ok {
                return Err("snapshot-failed".into());
            }
            // A snapshot from before the entry became confidential holds plain text; a later one holds
            // ciphertext readable with the current vault key. The entry stays confidential after a
            // restore, so plain text is encrypted here and ciphertext is kept as is.
            let (text, variants) = if p.secret {
                let Some(key) = secret_key else {
                    return Err("secret-locked".to_string());
                };
                let variants = variants.iter().map(|v| secret_encrypt_if_plain(v, &key)).collect();
                (secret_encrypt_if_plain(&text, &key), variants)
            } else {
                (text, variants)
            };
            p.name = crate::md_sync::sync_clean_name(&name);
            p.text = text;
            // Re-validate the restored list anyway (a version predating MAX_ROTATE_VARIANTS must not
            // slip a too-long list past the shared floor).
            p.variants = sanitize_variants(variants);
            // A restore landing on zero entries must not leave "random" mode on an empty list.
            p.rotate_mode = sanitize_rotate_mode(p.rotate_mode.clone(), &p.variants);
            p.rotate_auto = sanitize_rotate_auto(p.rotate_auto, &p.variants);
            // What the stored entry holds (sealed for a confidential one) is what the
            // windows are told; only the caller of this command gets the opened copy.
            let stored = p.clone();
            let mut clone = p.clone();
            // The editor shows and re-saves plain text: the stored prompt keeps its
            // ciphertext, the copy handed back is opened with the same key.
            if p.secret {
                if let Some(key) = secret_key {
                    clone.text = reveal_if_sealed(&clone.text, &key);
                    clone.variants = clone.variants.iter().map(|v| reveal_if_sealed(v, &key)).collect();
                }
            }
            // A failed save rolls the live entry back, as in update_prompt, or the UI shows a restored
            // prompt that reverts on the next launch.
            if !save_prompts(&app, &store) {
                if let Some(slot) = store.prompts.iter_mut().find(|x| x.id == pid) {
                    *slot = original;
                }
                // The restore did not happen: take its untrimmed undo snapshot back out, so the
                // history is exactly as it was.
                if let Some(c) = db_conn(&app) {
                    db_drop_latest_version(&c, &pid);
                }
                return Err("save-failed".into());
            }
            if let Some(c) = db_conn(&app) {
                let _ = db_trim_versions(&c, &pid, cap);
            }
            Some((clone, stored))
        } else {
            None
        }
    };
    if let Some((_, stored)) = &restored {
        let _ = app.emit("prompt-updated", stored.clone());
        rebuild_tray(&app);
    }
    Ok(restored.map(|(opened, _)| opened))
}

// The prompt "restore as new" builds from a version snapshot. Name and text/variants come
// from the version; colour, image and tags from the current prompt, since prompt_versions
// never stored them. Everything else starts fresh as for any new entry.
pub(crate) fn build_restored_prompt(source: &Prompt, version_name: &str, version_text: String, version_variants: Vec<String>, suffix: &str) -> Prompt {
    let variants = sanitize_variants(version_variants.iter().map(|s| upgrade_placeholders(s)).collect());
    let rotate_mode = sanitize_rotate_mode(String::new(), &variants);
    let name = if suffix.is_empty() { version_name.to_string() } else { format!("{version_name} {suffix}") };
    Prompt {
        id: gen_id(),
        trashed_at: 0,
        name,
        text: upgrade_placeholders(&version_text),
        color: source.color.clone(),
        image: source.image.clone(),
        show_image: source.show_image,
        copy_image: source.copy_image,
        file_path: String::new(),
        pdf_page: 0,
        icon_path: String::new(),
        caption: String::new(),
        caption_size: 0,
        font: String::new(),
        font_size: 0,
        favorite: false,
        tags: source.tags.clone(),
        notes: String::new(),
        hotkey: String::new(),
        hotkey_paste: String::new(),
        variants,
        rotate_mode,
        rotate_auto: false,
        // No attachment is carried over either (file_path/icon_path above), so
        // there is nothing for any of these to describe.
        media_duration_ms: 0,
        media_width: 0,
        media_height: 0,
        media_size_bytes: 0,
        image_text: String::new(),
        // Starts non-confidential; restore_version_as_new seals the copy and marks it
        // confidential when its source is, once the snapshot is open as plain text.
        secret: false,
    }
}

// A copy restored from a confidential entry is confidential too: its (plain) text and
// entries are sealed with the vault key before it is stored.
pub(crate) fn seal_restored_prompt(prompt: &mut Prompt, key: &[u8; 32]) {
    prompt.secret = true;
    prompt.text = secret_encrypt(&prompt.text, key);
    prompt.variants = prompt.variants.iter().map(|v| secret_encrypt(v, key)).collect();
}

// Keeps a version snapshot and the current entry: unlike restore_version this adds a new
// entry so the two can be compared side by side. One backend command (not a form the
// frontend prefills), reusing add_prompt's grid-placement path and firing no event
// add_prompt would not.
#[tauri::command]
pub(crate) async fn restore_version_as_new(app: AppHandle, state: State<'_, Db>, version_id: i64, suffix: String) -> Result<Option<Prompt>, String> {
    let created = {
        let mut store = lock(&state);
        let record = match db_conn(&app).and_then(|c| db_get_version(&c, version_id)) {
            Some(r) => r,
            None => return Ok(None),
        };
        let (pid, name, text, variants) = record;
        let Some(source) = store.prompts.iter().find(|p| p.id == pid).cloned() else {
            return Ok(None);
        };
        // A snapshot of a confidential entry may hold ciphertext. It is opened with the
        // session key (refused while locked) and the copy stays confidential, sealed again,
        // so the text never lands in a plain-looking entry.
        let key = secret_session_key(secret_timeout_min(&store.settings));
        if source.secret && key.is_none() {
            return Err("secret-locked".to_string());
        }
        let (text, variants) = open_snapshot(&text, &variants, key)?;
        let mut prompt = build_restored_prompt(&source, &name, text, variants, &suffix);
        if source.secret {
            if let Some(k) = key {
                seal_restored_prompt(&mut prompt, &k);
            }
        }
        // Same placement as add_prompt: next free cell of the active view, normally the view whose
        // history was open.
        add_placed_prompt_core(&mut store, &prompt, |st, original| {
            save_settings_then_rows(&app, st, original, |s| save_prompts(&app, s))
        })?;
        prompt
    };
    rebuild_tray(&app);
    Ok(Some(created))
}

// Star / unstar a prompt in the library.
#[tauri::command(async)]
pub(crate) fn set_favorite(app: AppHandle, state: State<Db>, id: String, favorite: bool) -> Result<(), String> {
    let ok = {
        let mut store = lock(&state);
        match store.prompts.iter_mut().find(|p| p.id == id) {
            Some(p) => {
                let previous = p.favorite;
                p.favorite = favorite;
                // A failed save must not leave the UI believing the star took:
                // roll the in-memory flag back so the next render shows the truth.
                if save_prompts(&app, &store) {
                    true
                } else {
                    if let Some(slot) = store.prompts.iter_mut().find(|x| x.id == id) {
                        slot.favorite = previous;
                    }
                    false
                }
            }
            None => true, // nothing to persist
        }
    };
    if !ok {
        return Err("save-failed".into());
    }
    rebuild_tray(&app); // The favourites section follows the stars
    Ok(())
}

// Async: keeps the prompt-table rewrite off the UI thread.
#[tauri::command]
// "Delete" moves a prompt into the trash table: media files and version history stay
// untouched until a real purge (purge_prompt, empty_trash or the retention sweep). Views,
// floating, folders, hotkeys and tray are app-level associations released as before; a
// restored prompt gets a fresh placement.
pub(crate) async fn delete_prompt(app: AppHandle, state: State<'_, Db>, id: String) -> Result<bool, String> {
    let changed = {
        let mut store = lock(&state);
        delete_prompt_core(&mut store, &id, now_secs(), |st, original| {
            save_settings_then_rows(&app, st, original, |s| save_prompts_and_trash(&app, s))
        })?
    };
    // Only now: a failed save returned above with the prompt still in the library,
    // and its pill has to stay open then.
    close_floating_window(&app, &id);
    if changed {
        rearm_hotkeys(&app); // A deleted prompt gives its accelerator back
        rebuild_tray(&app);
        sync_float_windows(&app); // A folder left with one member dissolves
        if let Some(conn) = db_conn(&app) {
            discard_editor_draft_if(&conn, |t| t == id);
        }
    }
    Ok(changed)
}

// Moves one prompt into the trash and lets go of its app-level associations (cells, membership,
// pill, scale, video prefs, folder). The settings row and the prompt/trash rows are written as one
// unit; on a failed write the prompt is back in the library and the associations are restored, so
// the disk never holds a view or pill naming an id that already sits in the trash. Returns whether
// a prompt moved.
pub(crate) fn delete_prompt_core(store: &mut Store, id: &str, now: u64, save: impl FnOnce(&Store, &Settings) -> bool) -> Result<bool, String> {
    let original_settings = store.settings.clone();
    let moved = match store.prompts.iter().position(|p| p.id == id) {
        Some(pos) => {
            let mut p = store.prompts.remove(pos);
            p.trashed_at = now;
            crate::md_sync::sync_owe_marker(&mut store.settings, &p);
            store.trash.insert(0, p); // newest-deleted first
            Some(pos)
        }
        None => None,
    };
    for view in &mut store.settings.views {
        for layout in view.layouts.values_mut() {
            layout.remove(id);
        }
        view.members.retain(|m| m != id);
    }
    store.settings.floating.remove(id);
    store.settings.float_scale.remove(id);
    store.settings.float_size.remove(id);
    store.settings.video_prefs.remove(id);
    store.normalize_folders(); // And out of whatever folder held it
    if let Some(pos) = moved {
        if !save(store, &original_settings) {
            if !store.trash.is_empty() {
                let mut p = store.trash.remove(0);
                p.trashed_at = 0;
                let at = pos.min(store.prompts.len());
                store.prompts.insert(at, p);
            }
            store.settings = original_settings;
            return Err("save-failed".into());
        }
    }
    Ok(moved.is_some())
}

// The in-memory half of a batch delete: the prompts move to the trash (newest-deleted
// first, keeping the batch's own order) and every app-level association is let go:
// view membership and cells, pills, scale, video prefs, folders. Returns the untouched
// originals so the caller can undo the move if the write fails.
pub(crate) fn move_batch_to_trash(store: &mut Store, ids: &[String], now: u64) -> Vec<Prompt> {
    let idset: std::collections::HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
    let mut moved: Vec<Prompt> = Vec::new();
    store.prompts.retain(|p| {
        if idset.contains(p.id.as_str()) {
            moved.push(p.clone());
            false
        } else {
            true
        }
    });
    if moved.is_empty() {
        return moved;
    }
    // .cloned() (not .into_iter()) so `moved` still holds the untouched
    // (trashed_at: 0) originals if the save fails.
    for mut p in moved.iter().cloned().rev() {
        p.trashed_at = now;
        crate::md_sync::sync_owe_marker(&mut store.settings, &p);
        store.trash.insert(0, p);
    }
    for view in &mut store.settings.views {
        for layout in view.layouts.values_mut() {
            layout.retain(|k, _| !idset.contains(k.as_str()));
        }
        view.members.retain(|m| !idset.contains(m.as_str()));
    }
    for id in ids {
        store.settings.floating.remove(id);
        store.settings.float_scale.remove(id);
        store.settings.float_size.remove(id);
        store.settings.video_prefs.remove(id);
    }
    store.normalize_folders();
    moved
}

// Batch counterpart of delete_prompt_core: the same unit of writes, and on failure the whole
// selection is back in the library. The prompts return at the end of store.prompts, not their
// original indices: this is a recovery path, and the visible grid position lives in view.layouts.
// Returns whether anything moved.
pub(crate) fn delete_batch_core(store: &mut Store, ids: &[String], now: u64, save: impl FnOnce(&Store, &Settings) -> bool) -> Result<bool, String> {
    let original_settings = store.settings.clone();
    let moved = move_batch_to_trash(store, ids, now);
    if moved.is_empty() {
        return Ok(false);
    }
    if !save(store, &original_settings) {
        store.trash.drain(0..moved.len());
        store.prompts.extend(moved);
        store.settings = original_settings;
        return Err("save-failed".into());
    }
    Ok(true)
}

// Batch operations: apply one action to many prompts in a single store pass (no per-item
// save loop, so 100+ selected prompts never stall the UI). Returns the fresh state so the
// caller re-renders once. Actions: delete, favorite, color, addView, removeView, tag-add,
// tag-remove.
// One flag per independent batch action the library toolbar can trigger; splitting into
// several commands would only move the argument count to the call sites.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn batch_prompts(
    app: AppHandle,
    state: State<'_, Db>,
    ids: Vec<String>,
    action: String,
    color: Option<String>,
    favorite: Option<bool>,
    view_id: Option<String>,
    from_view_id: Option<String>,
    tag: Option<String>,
) -> Result<Settings, String> {
    let idset: std::collections::HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
    let mut store = lock(&state);
    match action.as_str() {
        "delete" => {
            // Move to trash, same as the single-prompt delete_prompt command:
            // media files and version history stay untouched until a real purge.
            let deleted = delete_batch_core(&mut store, &ids, now_secs(), |st, original| {
                save_settings_then_rows(&app, st, original, |s| save_prompts_and_trash(&app, s))
            })?;
            if deleted {
                if let Some(conn) = db_conn(&app) {
                    discard_editor_draft_if(&conn, |t| idset.contains(t));
                }
            }
        }
        "favorite" => {
            let fav = favorite.unwrap_or(true);
            let previous: Vec<(String, bool)> = store
                .prompts
                .iter()
                .filter(|p| idset.contains(p.id.as_str()))
                .map(|p| (p.id.clone(), p.favorite))
                .collect();
            for p in store.prompts.iter_mut() {
                if idset.contains(p.id.as_str()) {
                    p.favorite = fav;
                }
            }
            // A failed save must roll every touched prompt back, same pattern as
            // the "delete" branch above: otherwise the UI shows a batch-favorite that
            // silently reverts for some or all of the selection on the next launch.
            if !save_prompts(&app, &store) {
                for (id, was) in previous {
                    if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
                        p.favorite = was;
                    }
                }
                return Err("save-failed".into());
            }
        }
        "color" => {
            let c = valid_tag_color(&color.unwrap_or_default())?;
            let previous: Vec<(String, String)> = store
                .prompts
                .iter()
                .filter(|p| idset.contains(p.id.as_str()))
                .map(|p| (p.id.clone(), p.color.clone()))
                .collect();
            for p in store.prompts.iter_mut() {
                if idset.contains(p.id.as_str()) {
                    p.color.clone_from(&c);
                }
            }
            if !save_prompts(&app, &store) {
                for (id, was) in previous {
                    if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
                        p.color = was;
                    }
                }
                return Err("save-failed".into());
            }
        }
        "addView" | "removeView" => {
            let vid = view_id.unwrap_or_default();
            apply_view_batch_action(&mut store.settings, &action, &ids, &vid, "", |s| save_settings(&app, s))?;
        }
        // Both layouts change in the same in-memory settings before the single save, so a failure
        // or crash is seen only before or after the whole move, never midway.
        "moveView" => {
            let to_id = view_id.unwrap_or_default();
            let from_id = from_view_id.unwrap_or_default();
            apply_view_batch_action(&mut store.settings, &action, &ids, &to_id, &from_id, |s| save_settings(&app, s))?;
        }
        // Adding a tag a prompt already carries, or removing one it never had, just leaves that
        // prompt untouched. Casing follows the library's existing tag (canonical_tags), like the
        // single-prompt editor.
        "tag-add" | "tag-remove" => {
            let name = valid_tag_name(&tag.unwrap_or_default())?;
            let known = canonical_tags(&store.prompts);
            let previous: Vec<(String, Vec<String>)> = store
                .prompts
                .iter()
                .filter(|p| idset.contains(p.id.as_str()))
                .map(|p| (p.id.clone(), p.tags.clone()))
                .collect();
            let result = if action == "tag-add" {
                add_tag_to_ids(&mut store.prompts, &idset, &name, &known)
            } else {
                remove_tag_from_ids(&mut store.prompts, &idset, &tag_key(&name));
                Ok(())
            };
            if let Err(e) = result {
                for (id, tags) in previous {
                    if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
                        p.tags = tags;
                    }
                }
                return Err(e);
            }
            if !save_prompts(&app, &store) {
                for (id, tags) in previous {
                    if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
                        p.tags = tags;
                    }
                }
                return Err("save-failed".into());
            }
        }
        _ => return Err("bad-action".into()),
    }
    // Return only the (small) settings: the caller mutates its local prompt list
    // in place, so we never re-serialise every prompt's base64 image over IPC.
    let settings = webview_settings(&store.settings);
    drop(store); // never re-arm shortcuts while the store mutex is held
    if action == "delete" {
        // Tauri window calls stay outside the store mutex (deadlock trap), and only
        // after the save succeeded: a failed one returned above with the prompts still
        // in the library, and their pills have to stay open then.
        for id in &ids {
            close_floating_window(&app, id);
        }
        rearm_hotkeys(&app); // The deleted prompts give their accelerators back
        sync_float_windows(&app); // Folders left too small dissolve
    }
    // A floating pill or folder window reads view membership too (pill folders group by view),
    // so moving or copying a prompt between views must reach it like any other settings change.
    if matches!(action.as_str(), "addView" | "removeView" | "moveView") {
        notify_float_settings(&app);
    }
    rebuild_tray(&app); // One rebuild for the whole batch, not one per prompt
    Ok(settings)
}
