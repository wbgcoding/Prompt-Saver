use super::*;

// ---------- Trash ----------

// What one row of the trash dialog reads, and nothing more: the dialog draws no
// picture, so the stored images (hundreds of kilobytes each) never travel. `text`
// is a short preview; a confidential entry's preview stays empty, since its stored
// text is ciphertext.
#[derive(Serialize)]
pub(crate) struct TrashRow {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) file_path: String,
    pub(crate) icon_path: String,
    pub(crate) copy_image: bool,
    pub(crate) has_image: bool,
    pub(crate) trashed_at: u64,
}

pub(crate) const TRASH_PREVIEW_CHARS: usize = 400;

pub(crate) fn trash_rows(trash: &[Prompt]) -> Vec<TrashRow> {
    trash
        .iter()
        .map(|p| TrashRow {
            id: p.id.clone(),
            name: p.name.clone(),
            text: if p.secret { String::new() } else { p.text.chars().take(TRASH_PREVIEW_CHARS).collect() },
            file_path: p.file_path.clone(),
            icon_path: p.icon_path.clone(),
            copy_image: p.copy_image,
            has_image: !p.image.is_empty(),
            trashed_at: p.trashed_at,
        })
        .collect()
}

#[tauri::command]
pub(crate) async fn list_trash(state: State<'_, Db>) -> Result<Vec<TrashRow>, String> {
    Ok(trash_rows(&lock(&state).trash))
}

// Seats a restored prompt in the active view: a free cell when there is one, and
// membership either way (a member without a cell shows as overflow, never lost).
pub(crate) fn seat_restored_in_active_view(settings: &mut Settings, id: &str) {
    let folder_cells = active_view_folder_cells(settings);
    let view = settings.active_view_mut();
    if let Some(cell) = first_free_cell_excluding(view, &folder_cells) {
        let key = grid_key(view.cols, view.rows);
        view.layouts.entry(key).or_default().insert(id.to_string(), cell);
    }
    if !view.members.iter().any(|m| m == id) {
        view.members.push(id.to_string());
    }
}

// Moves a trashed prompt back onto the active view and persists it. On a failed
// write the store is put back exactly as it was: the rollback copy is the FULL
// settings (never the window copy, which lacks the sealed backup password), so a failed restore
// cannot strip it from the live store.
pub(crate) fn restore_trashed_core(store: &mut Store, id: &str, save: impl FnOnce(&Store) -> bool) -> Result<(), String> {
    let Some(pos) = store.trash.iter().position(|p| p.id == id) else {
        return Err("not-found".into());
    };
    let original_settings = store.settings.clone();
    let mut p = store.trash.remove(pos);
    let original_trashed_at = p.trashed_at;
    p.trashed_at = 0;
    store.settings.sync_trash_marks.remove(id);
    // A trash entry from before the placeholder spelling changed comes back readable.
    upgrade_prompt_placeholders(&mut p);
    seat_restored_in_active_view(&mut store.settings, &p.id);
    store.prompts.push(p);
    // Same atomic prompts+trash write as delete_prompt - on failure, undo the
    // in-memory move (including the layout cell claimed above) instead of
    // reporting success for a restore that never reached disk.
    if !save(store) {
        if let Some(mut p) = store.prompts.pop() {
            p.trashed_at = original_trashed_at;
            let at = pos.min(store.trash.len());
            store.trash.insert(at, p);
        }
        store.settings = original_settings;
        return Err("save-failed".into());
    }
    Ok(())
}

// Places the restored prompt in the active view via the same first_free_cell scan
// add_prompt uses: if the view is full, the prompt still comes back (reachable through the
// library) without a grid cell, never a hard failure or a dropped prompt. Floating, folder
// membership and hotkey were let go at delete time and are not reconstructed.
#[tauri::command]
pub(crate) async fn restore_prompt(app: AppHandle, state: State<'_, Db>, id: String) -> Result<AppState, String> {
    let (app_state, restored) = {
        let mut store = lock(&state);
        let original_settings = store.settings.clone();
        restore_trashed_core(&mut store, &id, |st| {
            save_settings_then_rows(&app, st, &original_settings, |s| save_prompts_and_trash(&app, s))
        })?;
        let secret_key = secret_session_key(secret_timeout_min(&store.settings));
        (AppState { prompts: slim_prompts(&store.prompts, secret_key), settings: webview_settings(&store.settings) }, true)
    };
    if restored {
        rearm_hotkeys(&app); // The restored prompt's own hotkey, if any, is free again to re-claim
        rebuild_tray(&app);
    }
    Ok(app_state)
}

// Where a prompt sat when it was deleted, captured BEFORE the delete so an undo can put
// it back exactly: every view it belonged to with its position in the member list and
// its cell per grid size, its pill (position, scale), video prefs, and the grid and pill
// folder that held it. The trash row itself only keeps the prompt.
#[derive(Serialize, Deserialize, Clone, Default)]
pub(crate) struct ViewPlacement {
    pub(crate) view_id: String,
    pub(crate) index: usize,
    pub(crate) cells: HashMap<String, [u32; 2]>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub(crate) struct PromptPlacement {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) views: Vec<ViewPlacement>,
    #[serde(default)]
    pub(crate) floating: Option<Pos>,
    #[serde(default)]
    pub(crate) float_scale: Option<f64>,
    #[serde(default)]
    pub(crate) video_prefs: Option<VideoPrefs>,
    #[serde(default)]
    pub(crate) tile_folder: Option<TileFolder>,
    #[serde(default)]
    pub(crate) float_folder: Option<FloatFolder>,
}

pub(crate) fn capture_placements(settings: &Settings, ids: &[String]) -> Vec<PromptPlacement> {
    ids.iter()
        .map(|id| PromptPlacement {
            id: id.clone(),
            views: settings
                .views
                .iter()
                .filter_map(|v| {
                    let index = v.members.iter().position(|m| m == id)?;
                    let cells = v
                        .layouts
                        .iter()
                        .filter_map(|(k, layout)| layout.get(id).map(|c| (k.clone(), *c)))
                        .collect();
                    Some(ViewPlacement { view_id: v.id.clone(), index, cells })
                })
                .collect(),
            floating: settings.floating.get(id).cloned(),
            float_scale: settings.float_scale.get(id).copied(),
            video_prefs: settings.video_prefs.get(id).copied(),
            tile_folder: settings.tile_folders.iter().find(|f| f.members.contains(id)).cloned(),
            float_folder: settings.float_folders.iter().find(|f| f.members.contains(id)).cloned(),
        })
        .collect()
}

// Puts `id` on `cell` of the view's `key` grid; a cell that is taken (another tile, a
// grid folder) or lies outside the grid falls back to the first free cell of that grid.
pub(crate) fn place_on_cell_or_first_free(view: &mut View, key: &str, id: &str, cell: [u32; 2], folder_cells: &[[u32; 2]]) {
    let Some((cols, rows)) = key.split_once('x').and_then(|(c, r)| Some((c.parse::<u32>().ok()?, r.parse::<u32>().ok()?))) else {
        return;
    };
    let layout = view.layouts.entry(key.to_string()).or_default();
    let taken = |layout: &HashMap<String, [u32; 2]>, c: [u32; 2]| layout.values().any(|v| *v == c) || folder_cells.contains(&c);
    if cell[0] < cols && cell[1] < rows && !taken(layout, cell) {
        layout.insert(id.to_string(), cell);
        return;
    }
    for row in 0..rows {
        for col in 0..cols {
            if !taken(layout, [col, row]) {
                layout.insert(id.to_string(), [col, row]);
                return;
            }
        }
    }
}

// Undo for a delete: moves the trashed prompts back and re-applies what the snapshot
// holds. A failed write puts the store back exactly as it was (same full-settings
// rollback copy as restore_trashed_core). Ids that are no longer in the trash are
// skipped; none found at all is an error.
pub(crate) fn restore_placed_core(store: &mut Store, placements: &[PromptPlacement], save: impl FnOnce(&Store) -> bool) -> Result<usize, String> {
    let original_settings = store.settings.clone();
    let original_trash = store.trash.clone();
    let original_prompts = store.prompts.len();
    let mut restored: Vec<&PromptPlacement> = Vec::new();
    for pl in placements {
        if restored.iter().any(|r| r.id == pl.id) {
            continue;
        }
        let Some(pos) = store.trash.iter().position(|p| p.id == pl.id) else { continue };
        let mut p = store.trash.remove(pos);
        p.trashed_at = 0;
        upgrade_prompt_placeholders(&mut p);
        store.prompts.push(p);
        restored.push(pl);
    }
    if restored.is_empty() {
        return Err("not-found".into());
    }
    let restored_ids: std::collections::HashSet<String> = restored.iter().map(|r| r.id.clone()).collect();
    // Folders first: a rebuilt grid folder takes its old cell back before any tile is
    // placed, so a tile never lands on the cell the folder is about to need.
    let live_ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
    for pl in &restored {
        if let Some(snap) = &pl.tile_folder {
            restore_tile_folder(&mut store.settings, snap, &restored_ids, &live_ids);
        }
        if let Some(snap) = &pl.float_folder {
            restore_float_folder(&mut store.settings, snap, &restored_ids, &live_ids);
        }
    }
    for pl in &restored {
        for vp in &pl.views {
            let folder_cells: Vec<[u32; 2]> =
                store.settings.tile_folders.iter().filter(|f| f.view_id == vp.view_id).map(|f| f.cell).collect();
            let in_folder = store.settings.tile_folders.iter().any(|f| f.view_id == vp.view_id && f.members.contains(&pl.id));
            let Some(view) = store.settings.views.iter_mut().find(|v| v.id == vp.view_id) else { continue };
            if !view.members.contains(&pl.id) {
                let at = vp.index.min(view.members.len());
                view.members.insert(at, pl.id.clone());
            }
            if in_folder {
                continue; // a folder member owns no cell
            }
            for (key, cell) in &vp.cells {
                place_on_cell_or_first_free(view, key, &pl.id, *cell, &folder_cells);
            }
        }
        if let Some(pos) = &pl.floating {
            store.settings.floating.insert(pl.id.clone(), pos.clone());
        }
        if let Some(scale) = pl.float_scale {
            store.settings.float_scale.insert(pl.id.clone(), scale);
        }
        if let Some(prefs) = pl.video_prefs {
            store.settings.video_prefs.insert(pl.id.clone(), prefs);
        }
    }
    store.normalize_folders();
    if !save(store) {
        store.prompts.truncate(original_prompts);
        store.trash = original_trash;
        store.settings = original_settings;
        return Err("save-failed".into());
    }
    Ok(restored.len())
}

// A grid folder the delete dissolved (fewer than two members left) is rebuilt from its
// snapshot; one that survived just takes its members back at their old spots. A member
// inside a folder owns no cell, so its layout entries in that view are dropped again.
pub(crate) fn restore_tile_folder(settings: &mut Settings, snap: &TileFolder, restored: &std::collections::HashSet<String>, live_ids: &[String]) {
    if let Some(folder) = settings.tile_folders.iter_mut().find(|f| f.id == snap.id) {
        for (i, m) in snap.members.iter().enumerate() {
            if restored.contains(m) && !folder.members.contains(m) {
                let at = i.min(folder.members.len());
                folder.members.insert(at, m.clone());
            }
        }
    } else {
        let members: Vec<String> = snap
            .members
            .iter()
            .filter(|m| live_ids.contains(m) && !settings.tile_folders.iter().any(|f| f.members.contains(m)))
            .cloned()
            .collect();
        if members.len() < 2 {
            return;
        }
        let mut folder = snap.clone();
        folder.members = members;
        if let Some(view) = settings.views.iter().find(|v| v.id == folder.view_id) {
            let key = grid_key(view.cols, view.rows);
            let on_tile = view.layouts.get(&key).is_some_and(|l| l.iter().any(|(id, c)| *c == folder.cell && !folder.members.contains(id)));
            let on_folder = settings.tile_folders.iter().any(|f| f.view_id == folder.view_id && f.cell == folder.cell);
            if on_tile || on_folder {
                let others: Vec<[u32; 2]> = settings.tile_folders.iter().filter(|f| f.view_id == folder.view_id).map(|f| f.cell).collect();
                if let Some(c) = first_free_cell_excluding(view, &others) {
                    folder.cell = c;
                }
            }
        }
        settings.tile_folders.push(folder);
    }
    let Some(folder) = settings.tile_folders.iter().find(|f| f.id == snap.id) else { return };
    let (view_id, members) = (folder.view_id.clone(), folder.members.clone());
    if let Some(view) = settings.views.iter_mut().find(|v| v.id == view_id) {
        for layout in view.layouts.values_mut() {
            layout.retain(|id, _| !members.contains(id));
        }
    }
}

pub(crate) fn restore_float_folder(settings: &mut Settings, snap: &FloatFolder, restored: &std::collections::HashSet<String>, live_ids: &[String]) {
    if let Some(folder) = settings.float_folders.iter_mut().find(|f| f.id == snap.id) {
        for (i, m) in snap.members.iter().enumerate() {
            if restored.contains(m) && !folder.members.contains(m) {
                let at = i.min(folder.members.len());
                folder.members.insert(at, m.clone());
            }
        }
        return;
    }
    let members: Vec<String> = snap
        .members
        .iter()
        .filter(|m| live_ids.contains(m) && !settings.float_folders.iter().any(|f| f.members.contains(m)))
        .cloned()
        .collect();
    if members.len() >= 2 {
        let mut folder = snap.clone();
        folder.members = members;
        settings.float_folders.push(folder);
    }
}

// Read-only: the placement snapshot an undo needs, taken just before a batch delete.
#[tauri::command]
pub(crate) async fn placement_snapshot(state: State<'_, Db>, ids: Vec<String>) -> Result<Vec<PromptPlacement>, String> {
    Ok(capture_placements(&lock(&state).settings, &ids))
}

// Two writes that must both land: the first, then the second. When the first fails
// nothing is written; when the second fails the first is taken back, so the disk
// never holds placements for prompts that are still in the trash while the call
// reports an error.
pub(crate) fn save_pair_or_undo_first(first: impl FnOnce() -> bool, second: impl FnOnce() -> bool, undo_first: impl FnOnce()) -> bool {
    if !first() {
        return false;
    }
    if second() {
        return true;
    }
    undo_first();
    false
}

// The settings row and the prompt/trash rows of one change, written as a unit: settings first (a
// locked row counts as written, see save_settings_paired), then the rows; when the rows fail, the
// original settings go back. A failure of the settings write leaves the rows untouched on disk.
pub(crate) fn save_settings_then_rows(app: &AppHandle, st: &Store, original: &Settings, rows: impl FnOnce(&Store) -> bool) -> bool {
    save_pair_or_undo_first(
        || save_settings_paired(app, &st.settings),
        || rows(st),
        || {
            save_settings_paired(app, original);
        },
    )
}

// Undo of a batch delete with the snapshot taken before it: the placements and
// the prompts are written together or not at all.
#[tauri::command]
pub(crate) async fn restore_prompts_batch(app: AppHandle, state: State<'_, Db>, placements: Vec<PromptPlacement>) -> Result<AppState, String> {
    let app_state = {
        let mut store = lock(&state);
        let original_settings = store.settings.clone();
        restore_placed_core(&mut store, &placements, |st| {
            save_settings_then_rows(&app, st, &original_settings, |s| save_prompts_and_trash(&app, s))
        })?;
        let secret_key = secret_session_key(secret_timeout_min(&store.settings));
        AppState { prompts: slim_prompts(&store.prompts, secret_key), settings: webview_settings(&store.settings) }
    };
    rearm_hotkeys(&app);
    rebuild_tray(&app);
    sync_float_windows(&app); // pills and folders that came back
    Ok(app_state)
}

// Permanent delete: the trash entry, its media files and its whole version history are gone
// for good. Returns the orphan-file set to sweep instead of sweeping itself, so the caller
// can drop the store lock first and a big attachments folder does not lock out every other
// window during the fs scan.
pub(crate) fn purge_ids(app: &AppHandle, store: &mut Store, ids: &[String]) -> Result<Option<std::collections::HashSet<String>>, String> {
    let idset: std::collections::HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
    let before = store.trash.clone();
    let original_settings = store.settings.clone();
    store.trash.retain(|p| !idset.contains(p.id.as_str()));
    rotation_forget(&mut store.settings, ids); // no orphaned pointer/bag left behind

    // A failed write means the purged prompt comes back on the next restart (trash has no JSON
    // fallback like save_prompts), so its rotation position and version history must come back
    // with it rather than vanish for a purge that never persisted. The settings row goes first:
    // a purge reported as done must not leave a rotation pointer on a deleted id.
    if save_settings_then_rows(app, store, &original_settings, |s| save_trash(app, s)) {
        let orphan_scan = orphan_cleanup_enabled(store).then(|| orphan_referenced_set(store));
        // save_trash above returns true only through a working connection, so losing it between
        // the two calls is a narrow window, but a purge that leaves the prompt's text in
        // prompt_versions must not pass unrecorded.
        match db_conn(app) {
            Some(c) => {
                for id in ids {
                    db_delete_versions(&c, id);
                }
            }
            None => append_crash_log(
                "purge: no database connection, version history of the purged prompts was left in place",
            ),
        }
        Ok(orphan_scan)
    } else {
        // The disk-side trash still has these rows and they reappear on restart: restore the
        // in-memory copy so this session's trash view does not show them as gone.
        store.trash = before;
        store.settings = original_settings;
        Err("save-failed".to_string())
    }
}

#[tauri::command]
pub(crate) async fn purge_prompt(app: AppHandle, state: State<'_, Db>, id: String) -> Result<Vec<TrashRow>, String> {
    let (trash, orphan_scan) = {
        let mut store = lock(&state);
        let orphan_scan = purge_ids(&app, &mut store, std::slice::from_ref(&id))?;
        (trash_rows(&store.trash), orphan_scan)
    };
    if let Some(referenced) = orphan_scan {
        sweep_orphan_files(&app, &referenced);
    }
    Ok(trash)
}

#[tauri::command]
pub(crate) async fn empty_trash(app: AppHandle, state: State<'_, Db>) -> Result<Vec<TrashRow>, String> {
    let (trash, orphan_scan) = {
        let mut store = lock(&state);
        let ids: Vec<String> = store.trash.iter().map(|p| p.id.clone()).collect();
        let orphan_scan = purge_ids(&app, &mut store, &ids)?;
        (trash_rows(&store.trash), orphan_scan)
    };
    if let Some(referenced) = orphan_scan {
        sweep_orphan_files(&app, &referenced);
    }
    Ok(trash)
}

pub(crate) const TRASH_RETENTION_DAYS_DEFAULT: f64 = 30.0;

// The trash retention window in seconds, None when off (0 = forever, like historyDays).
pub(crate) fn trash_retention_secs(settings: &Settings) -> Option<u64> {
    let days = settings.ui_values.get("trashRetentionDays").copied().unwrap_or(TRASH_RETENTION_DAYS_DEFAULT);
    if days <= 0.0 {
        None
    } else {
        Some((days as u64).saturating_mul(86_400))
    }
}

// Pure decision, kept apart from sweep_trash's side effects (save/file-cleanup need
// an AppHandle, which cannot be constructed in a unit test) so the actual "what
// counts as stale" rule is directly testable.
pub(crate) fn stale_trash_ids(trash: &[Prompt], max_age: u64, now: u64) -> Vec<String> {
    trash
        .iter()
        .filter(|p| p.trashed_at > 0 && now.saturating_sub(p.trashed_at) > max_age)
        .map(|p| p.id.clone())
        .collect()
}

// Drop anything past the retention window. Runs once at startup, like prune_history, never
// on a timer, so a long-idle app does not surprise-purge something the user only just
// noticed in the trash.
pub(crate) fn sweep_trash(app: &AppHandle, store: &mut Store) {
    if !startup_sweeps_allowed() {
        return;
    }
    let Some(max_age) = trash_retention_secs(&store.settings) else {
        return;
    };
    let stale = stale_trash_ids(&store.trash, max_age, now_secs());
    if !stale.is_empty() {
        // Startup-only, before the store is behind its mutex: nothing else can
        // be waiting on the lock yet, so sweeping inline here is fine.
        if let Ok(Some(referenced)) = purge_ids(app, store, &stale) {
            sweep_orphan_files(app, &referenced);
        }
    }
}

// The trash row's on-demand trigger: the rule sweep_trash applies at startup, but explicit
// and always answered (even "nothing to clean up"), like sweep_recordings_now. A live click
// carries none of the surprise-purge risk of the startup-only sweep: the user asked for it
// now.
#[tauri::command(async)]
pub(crate) fn sweep_trash_now(app: AppHandle, state: State<'_, Db>) -> (u32, u64) {
    // The orphan sweep locks the store itself, so it runs only after this guard is
    // gone (same shape as empty_trash): holding both froze the whole app.
    let own_dirs = own_storage_dirs(&app, &state);
    let (count, bytes, orphan_scan) = {
        let mut store = lock(&state);
        let Some(max_age) = trash_retention_secs(&store.settings) else {
            return (0, 0);
        };
        let stale = stale_trash_ids(&store.trash, max_age, now_secs());
        if stale.is_empty() {
            return (0, 0);
        }
        let stale_paths: Vec<String> = store
            .trash
            .iter()
            .filter(|p| stale.contains(&p.id))
            .map(|p| p.file_path.clone())
            .collect();
        let (_, bytes) = attachments_bytes(&stale_paths, &own_dirs);
        let count = stale.len() as u32;
        let Ok(orphan_scan) = purge_ids(&app, &mut store, &stale) else {
            return (0, 0);
        };
        (count, bytes, orphan_scan)
    };
    if let Some(referenced) = orphan_scan {
        sweep_orphan_files(&app, &referenced);
    }
    (count, bytes)
}

// With no prompt left, nothing may still be placed: a leftover view member or layout
// cell would keep a grid cell taken for a prompt that does not exist, so the first
// new prompt would land behind a hole. The views themselves (names, sizes) stay.
pub(crate) fn clear_prompt_placements(settings: &mut Settings) {
    for view in settings.views.iter_mut() {
        view.members.clear();
        view.layouts.clear();
    }
    settings.floating.clear();
}

// Every table a data wipe empties with a raw DELETE. The prompts and trash
// tables are in the list although the two saves just emptied them: a save only
// removes rows this process had loaded, so a row it could not decrypt (the very
// reason a store can be locked) would otherwise survive the wipe.
pub(crate) const DATA_WIPE_TABLES: [&str; 7] =
    ["prompts", "trash", "prompt_versions", "copy_log", "clip_inbox", "edit_state", "editor_drafts"];

// A repack that meets another connection mid-write answers SQLITE_BUSY; it is tried again this
// far apart, and the whole wait is capped by a budget (a connection's own lock wait is 3 s per
// statement, so counting tries would let a held lock stall the wipe for minutes).
const SCRUB_PAUSE_MS: u64 = 120;
const SCRUB_BUDGET_MS: u64 = 10_000;
// The longest a single try may wait on the lock.
const SCRUB_TRY_WAIT_MS: u64 = 1_000;

// A wipe whose repack ran out of budget leaves this marker in the data folder; the next start
// finishes the repack.
const SCRUB_PENDING_FILE: &str = "scrub.pending";

// After a wipe the deleted rows still sit in SQLite's free pages and in the write-ahead log.
// Repacks the file, then folds the log back and truncates it, so the old bytes are overwritten.
// A busy file is retried for a few seconds; each step is only repeated until it has worked once.
pub(crate) fn scrub_freed_pages(conn: &Connection) -> bool {
    scrub_freed_pages_within(conn, std::time::Duration::from_millis(SCRUB_BUDGET_MS))
}

// The same with an explicit total budget. Returns false when the budget ran out first.
pub(crate) fn scrub_freed_pages_within(conn: &Connection, budget: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + budget;
    let mut packed = false;
    let mut folded = false;
    let mut first = true;
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if !first && left.is_zero() {
            break;
        }
        // One try never waits on the lock longer than what is left of the budget.
        let wait = left.min(std::time::Duration::from_millis(SCRUB_TRY_WAIT_MS));
        let _ = conn.busy_timeout(wait);
        packed = packed || conn.execute("VACUUM", []).is_ok();
        // The first column is 1 when a reader kept the log from being truncated.
        folded = folded || conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get::<_, i64>(0)).is_ok_and(|busy| busy == 0);
        if packed && folded {
            let _ = conn.busy_timeout(std::time::Duration::from_millis(3000));
            return true;
        }
        first = false;
        let pause = std::time::Duration::from_millis(SCRUB_PAUSE_MS).min(deadline.saturating_duration_since(std::time::Instant::now()));
        std::thread::sleep(pause);
    }
    let _ = conn.busy_timeout(std::time::Duration::from_millis(3000));
    false
}

pub(crate) fn scrub_pending_marker(dir: &Path) -> PathBuf {
    dir.join(SCRUB_PENDING_FILE)
}

// Remembers an unfinished repack for the next start.
pub(crate) fn mark_scrub_pending(dir: &Path) {
    let _ = std::fs::write(scrub_pending_marker(dir), b"1");
}

// Finishes a repack an earlier wipe could not. False when nothing was pending or it still
// does not work (the marker stays then); the marker goes only once the scrub worked.
pub(crate) fn finish_pending_scrub_in(dir: &Path, conn: &Connection) -> bool {
    let marker = scrub_pending_marker(dir);
    if !marker.exists() {
        return false;
    }
    if scrub_freed_pages(conn) {
        let _ = std::fs::remove_file(&marker);
        return true;
    }
    false
}

pub(crate) fn finish_pending_scrub(app: &AppHandle) {
    let dir = data_dir(app);
    if !scrub_pending_marker(&dir).exists() {
        return;
    }
    if let Some(conn) = db_conn_write(app) {
        if finish_pending_scrub_in(&dir, &conn) {
            append_crash_log("start-up: finished the database repack a data wipe had left open");
        }
    }
}

// True while any wiped table still holds a row; a table that cannot be counted counts as
// holding one, so a failed read never reports a wipe as done.
pub(crate) fn data_wipe_rows_left(c: &Connection) -> bool {
    DATA_WIPE_TABLES.iter().any(|table| {
        c.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table})"), [], |r| r.get::<_, i64>(0)).ok() != Some(0)
    })
}

// Empties every table in DATA_WIPE_TABLES; returns the ones that failed with their error.
pub(crate) fn wipe_data_tables(c: &Connection) -> Vec<(&'static str, rusqlite::Error)> {
    let mut failed = Vec::new();
    for table in DATA_WIPE_TABLES {
        if let Err(e) = c.execute(&format!("DELETE FROM {table}"), []) {
            failed.push((table, e));
        }
    }
    failed
}

// Factory reset: wipe all prompts AND all settings (views, theme, window,
// behaviour, fonts) and remove the autostart registry entry.
// Async: window closes + a full prompts/settings rewrite stay off the UI thread.
#[tauri::command]
pub(crate) async fn delete_all_data(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    // A read-only instance (the folder is held by another machine) must not reach
    // the raw table wipe below: the saves are refused there, the DELETEs are not.
    if data_dir_write_blocked(&app) {
        return Err("readonly".into());
    }
    let labels: Vec<String> = app
        .webview_windows()
        .keys()
        .filter(|l| l.starts_with("float-") || l.starts_with("folder-"))
        .cloned()
        .collect();
    for label in labels {
        if let Some(win) = app.get_webview_window(&label) {
            let _ = win.close();
        }
    }
    let mut incomplete = false;
    {
        let mut store = lock(&state);
        store.prompts.clear();
        store.trash.clear(); // a data wipe must not leave trashed prompts behind
        store.prompts_locked = false; // explicit wipe overrides the decrypt-failure lock
        store.normalize_folders(); // No prompts left, so no folder survives
        clear_prompt_placements(&mut store.settings);
        if !save_prompts(&app, &store) {
            incomplete = true;
        }
        if !save_trash(&app, &store) {
            incomplete = true;
        }
        // The views and folders still naming the deleted ids on disk would point at nothing.
        if !save_settings_paired(&app, &store.settings) {
            incomplete = true;
        }
        // Data-only wipe: keep every setting, drop the derived data tables. Each failure is logged
        // and reported, so a locked file or mid-wipe I/O error cannot leave private rows on disk
        // while the user is told everything is gone.
        match db_conn(&app) {
            Some(c) => {
                for (table, e) in wipe_data_tables(&c) {
                    incomplete = true;
                    append_crash_log(&format!("delete_all_data: could not clear {table}: {e}"));
                }
            }
            None => {
                incomplete = true;
                append_crash_log("delete_all_data: no DB connection, auxiliary tables not cleared");
            }
        }
        // The wipe above went straight to the tables: nothing is on disk for the
        // next write to skip.
        store.forget_written_rows();
    }
    // With the lock dropped: every picture and attachment the deleted prompts named is an orphan
    // now (the sweep re-checks the live store and keeps pending captures), and the freed
    // database pages and the log must not keep the old rows.
    remove_locked_backup(&data_dir(&app));
    let sweep_app = app.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        sweep_orphan_files(&sweep_app, &std::collections::HashSet::new());
        match db_conn(&sweep_app) {
            Some(c) => (scrub_freed_pages(&c), data_wipe_rows_left(&c)),
            None => (false, true),
        }
    })
    .await
    .map(|(scrubbed, rows_left)| {
        if !scrubbed {
            mark_scrub_pending(&data_dir(&app));
            append_crash_log("delete_all_data: could not repack the database, it is finished on the next start");
        }
        // A busy repack leaves the rows deleted; only rows still in a table make the wipe incomplete.
        if rows_left {
            incomplete = true;
        }
    });
    rearm_hotkeys(&app); // No prompts left to keep accelerators bound
    rebuild_tray(&app); // Nothing left to offer in the tray either
    if incomplete {
        return Err("delete-incomplete".into());
    }
    Ok(())
}

// The settings a reset leaves behind: every default, plus the values that are keys rather
// than preferences. The backup password (dropping it would make every protected backup
// unreadable) and the vault's
// salt and verifier (what the confidential prompts the reset keeps are sealed with).
pub(crate) fn settings_after_reset(old: &Settings) -> Settings {
    let mut fresh = Settings::default();
    fresh.migrate();
    // A reset must never start deleting what the old settings kept: each retention
    // window stays at whichever of the old value and the default keeps longer.
    for (k, def) in [
        ("historyDays", HISTORY_DAYS_DEFAULT),
        ("recordingRetentionDays", RECORDING_RETENTION_DAYS_DEFAULT),
        ("trashRetentionDays", TRASH_RETENTION_DAYS_DEFAULT),
    ] {
        let was = old.ui_values.get(k).copied().unwrap_or(def);
        if was <= 0.0 || was > def {
            fresh.ui_values.insert(k.to_string(), was.max(0.0));
        }
    }
    if let Some(pw) = old.ui_texts.get(BACKUP_PW_KEY) {
        fresh.ui_texts.insert(BACKUP_PW_KEY.to_string(), pw.clone());
    }
    carry_vault_keys(old, &mut fresh);
    fresh
}

// The in-memory half of a settings reset with its write. The write result decides: a failed write
// puts the old settings, the corrupt-row lock and the colour history back, so the session never
// shows factory settings while the disk keeps the old ones. Returns the old settings and whether
// the picture-text index was on.
pub(crate) fn reset_settings_core(store: &mut Store, write: impl FnOnce(&Store) -> bool) -> Result<(Settings, bool), String> {
    let old = store.settings.clone();
    let was_locked = SETTINGS_LOCKED.load(Ordering::Relaxed);
    let ocr_was_on = ocr_index_flag_on(store);
    // The backup password is a key, not a preference: dropping it would make
    // every password-protected backup permanently unreadable.
    store.settings = settings_after_reset(&store.settings);
    SETTINGS_LOCKED.store(false, Ordering::Relaxed); // explicit reset overrides the corrupt-row lock
    load_pick_history(&store.settings); // reset clears the mirror too
    // The prompt rows are untouched, but the store around them is rebuilt from defaults: start
    // the row bookkeeping from scratch.
    store.forget_written_rows();
    if !write(store) {
        store.settings = old;
        SETTINGS_LOCKED.store(was_locked, Ordering::Relaxed);
        load_pick_history(&store.settings);
        mirror_minimize_to_tray(&store.settings);
        return Err("save-failed".into());
    }
    Ok((old, ocr_was_on))
}

// Reset every SETTING to its default (theme, views/layouts, toggles, expert values)
// while keeping the stored prompts intact. The front-end reloads so each default
// re-applies cleanly; cleared views make normalizeLayout re-place the kept prompts.
#[tauri::command]
pub(crate) async fn reset_settings(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    let labels: Vec<String> = app
        .webview_windows()
        .keys()
        .filter(|l| l.starts_with("float-") || l.starts_with("folder-"))
        .cloned()
        .collect();
    for label in labels {
        if let Some(win) = app.get_webview_window(&label) {
            // Tracked, so a sync right after a failed reset waits for the teardown instead of
            // keeping the half-closed window.
            close_tracked(&label, &win);
        }
    }
    let reset = {
        let mut store = lock(&state);
        match reset_settings_core(&mut store, |st| save_settings(&app, &st.settings)) {
            Ok((_old, ocr_was_on)) => {
                // Defaults switch the picture-text index off: erase its text as well.
                if ocr_was_on && clear_ocr_index_text(&mut store) {
                    write_ocr_wipe(&app, &store, "reset");
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    };
    match reset {
        Ok(()) => {}
        Err(e) => {
            // The settings rolled back, so the windows closed above belong back; the store
            // lock is released here, as the window calls require.
            sync_float_windows(&app);
            return Err(e);
        }
    };
    // Another copy's Run entry (portable versus installed) is not this reset's to remove.
    if !autostart_removal_skipped() {
        let _ = apply_autostart(false, false);
    }
    // Defaults mean the screenshot switch is off: open windows go back to excluded.
    apply_capture_exclusion_all(&app, true);
    // The key overlay defaults off too: end a running capture.
    #[cfg(windows)]
    keylog::flag_changed(false);
    Ok(())
}

// The answer is kept per SOURCE FOLDER (never a single global flag), so a
// genuinely different old location is offered again even after an earlier one
// was settled. Stored in the same free-form map the expert menu's own text
// settings use, under a key namespace of its own.
pub(crate) const TAKEOVER_DECISION_PREFIX: &str = "takeoverDecision:";

pub(crate) fn takeover_decision_key(src: &std::path::Path) -> String {
    format!("{TAKEOVER_DECISION_PREFIX}{}", src.display())
}

// Whether this exact old location was already settled, whichever way. Pulled
// out of takeover_offer() (which needs an AppHandle) so the three answer paths
// (adopt, keep separate, never answered) are testable without one.
pub(crate) fn takeover_already_decided(ui_texts: &HashMap<String, String>, src: &std::path::Path) -> bool {
    ui_texts.contains_key(&takeover_decision_key(src))
}

// Records the answer only if asked to remember it. Skipping the record on
// `remember == false` is deliberate: closing the dialog without ticking that
// box must leave nothing behind, so the same folder is offered again next
// launch rather than being silently treated as settled.
pub(crate) fn takeover_record_decision(ui_texts: &mut HashMap<String, String>, src: &std::path::Path, adopt: bool, remember: bool) {
    if remember {
        ui_texts.insert(takeover_decision_key(src), if adopt { "adopt" } else { "skip" }.to_string());
    }
}

// The three things the offer dialog needs to say before anything is copied:
// where the old data is, how big it is, and roughly how much is in it.
#[derive(Serialize)]
pub(crate) struct TakeoverOffer {
    pub(crate) path: String,
    pub(crate) size: u64,
    // None when the database could not be opened (e.g. the other copy of the
    // app still has it open): the dialog falls back to naming it unknown
    // rather than blocking the offer on a count that is not essential to it.
    pub(crate) prompts: Option<usize>,
}

// Size and prompt count of a store folder, read without staging or copying it
//: unlike a backup, a live store is a plain uncompressed data.db, so a
// direct read-only COUNT(*) is enough and never touches restore's read rail.
pub(crate) fn takeover_offer_info(src: &std::path::Path) -> TakeoverOffer {
    let size = STORE_FILES.iter().filter_map(|f| fs::metadata(src.join(f)).ok()).map(|m| m.len()).sum();
    // Read-only: a plain open would create an empty data.db in a folder that only holds legacy files.
    let prompts = Connection::open_with_flags(src.join("data.db"), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .ok()
        .and_then(|c| c.query_row("SELECT COUNT(*) FROM prompts", [], |r| r.get::<_, i64>(0)).ok())
        .map(|n| n as usize);
    TakeoverOffer { path: src.display().to_string(), size, prompts }
}

// Whether the store has nothing an adoption offer would overwrite: no prompts at
// all, or only the ones seed_example_prompts put there itself. Kept separate from
// takeover_offer() (which needs an AppHandle) so the condition itself is testable.
pub(crate) fn store_has_no_real_prompts(prompts: &[Prompt], example_prompt_ids: &[String]) -> bool {
    prompts.iter().all(|p| example_prompt_ids.contains(&p.id))
}

// Portable run, own store still empty (or seeded-examples-only), an installed store
// present elsewhere → the UI offers to adopt it, naming its size and prompt count
// so the question is answerable without opening the app twice. Nothing here
// copies or moves anything: see takeover_apply for that, after an answer.
#[tauri::command]
pub(crate) fn takeover_offer(app: AppHandle, state: State<Db>) -> Option<TakeoverOffer> {
    // Adopting copies a database over the folder another machine has open: not offered at all.
    // Quiet, since an offer is not a refused action.
    if !is_portable() || data_dir_write_blocked_quiet() {
        return None;
    }
    {
        let store = lock(&state);
        if !store_has_no_real_prompts(&store.prompts, &store.settings.example_prompt_ids) {
            return None;
        }
    }
    let dir = data_dir(&app);
    let src = appdata_store_dir(&app, &dir)?;
    if takeover_already_decided(&lock(&state).settings.ui_texts, &src) {
        return None;
    }
    Some(takeover_offer_info(&src))
}

// Answer to that question. `adopt` copies the installed store beside the exe (the
// original stays untouched) and reloads it; either way, `remember` decides whether
// this exact old location is settled for future launches or asked about again --
// closing the dialog without ticking "do not ask again" must leave nothing decided.
#[tauri::command(async)]
pub(crate) fn takeover_apply(app: AppHandle, state: State<'_, Db>, adopt: bool, remember: bool) -> Result<(), String> {
    if data_dir_write_blocked(&app) {
        return Err("readonly".to_string());
    }
    let dir = data_dir(&app);
    let src = appdata_store_dir(&app, &dir);
    if adopt {
        let src = src.clone().ok_or("no-source")?;
        // One lock from the empty-library check to the swap: the offer was made on an empty store,
        // and a prompt added in between would be replaced by the adopted database and lost.
        let mut store = lock(&state);
        if !store_has_no_real_prompts(&store.prompts, &store.settings.example_prompt_ids) {
            return Err("this library already has prompts".to_string());
        }
        let mut landed = false;
        // The installed app may be running in the tray and writing to this store.
        let copied = copy_live_store_files(&src, &dir, &mut landed);
        // Whatever arrived is the store on disk now, while the one in memory (and its row
        // bookkeeping) still describes the empty one it replaced, so the next save would delete
        // every adopted row as gone. The source in %APPDATA% is untouched either way; reload from
        // what landed, also when the copy failed halfway. The freshly copied database supersedes
        // whatever the empty one wrote, so its write-ahead files must not survive into the new one.
        if copied.is_ok() || landed {
            let _ = fs::remove_file(dir.join("data.db-wal"));
            let _ = fs::remove_file(dir.join("data.db-shm"));
            // Loaded under the guard, so without the sweeps that lock the store again.
            let fresh = load_store_without_sweeps(&app);
            end_session_if_vault_changed(&store.settings, &fresh.settings);
            *store = fresh;
        }
        drop(store);
        copied?;
        // An adopted store must bring its backup history along too (as set_data_dir does), or the
        // Backup panel shows 0 snapshots.
        let src_backups = src.join("backups");
        if src_backups.is_dir() {
            if let Err(e) = copy_dir_all(&src_backups, &dir.join("backups")) {
                append_crash_log(&format!("takeover: could not copy backups/ ({e})"));
            }
        }
    }
    if remember {
        if let Some(src) = src {
            let mut store = lock(&state);
            takeover_record_decision(&mut store.settings.ui_texts, &src, adopt, remember);
            save_settings(&app, &store.settings);
        }
    }
    Ok(())
}

// UI language preference: "auto" or one of LANG_CODES.
#[tauri::command(async)]
pub(crate) fn set_language(app: AppHandle, state: State<Db>, lang: String) {
    let resolved = {
        let mut store = lock(&state);
        store.settings.language = lang;
        let resolved = resolve_lang(&store.settings.language);
        // Start page still has a default name -> translate it along.
        let home = home_name(resolved);
        for view in &mut store.settings.views {
            if HOME_NAMES.contains(&view.name.as_str()) {
                view.name = home.to_string();
            }
        }
        save_settings(&app, &store.settings);
        resolved
    };
    // Live updates without a restart: tray menu + floating pills.
    apply_tray_menu(&app);
    let _ = app.emit("language-changed", resolved);
}

// Font family + size for the saved prompt tiles only. size 0 = auto-fit.
#[tauri::command(async)]
pub(crate) fn set_tile_style(app: AppHandle, state: State<Db>, font: String, size: u32) {
    let mut store = lock(&state);
    store.settings.tile_font = font;
    store.settings.tile_size = if size == 0 { 0 } else { size.clamp(10, 40) };
    save_settings(&app, &store.settings);
}
