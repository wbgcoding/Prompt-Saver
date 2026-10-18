use super::*;

// Unfinished screenshot/video editor work, remembered so closing the window by accident
// does not lose it. One row per (kind, target): kind is "snip" or "video", target the source
// path. `ops` is the editor's own JSON, never parsed here (its embedded version can change
// without a migration). size/mtime are the file's stat at save time; load matches on the
// exact triple, so a changed source file misses instead of restoring onto other content.

// The (size, mtime) pair save_edit_state/load_edit_state key on: a plain stat, cheap enough
// to call on every open.
#[tauri::command]
pub(crate) async fn stat_file(path: String) -> Result<(i64, i64), String> {
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok((meta.len() as i64, mtime))
}

// Rows are keyed by a hash of the file path and carry the path and ops sealed, so the table
// does not list recording paths or text overlay contents in clear. A row in the legacy form
// (plain "kind:path" id, plain columns) is still found through its legacy id.
pub(crate) fn edit_state_id(kind: &str, target: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{kind}:{:x}", Sha256::digest(target.as_bytes()))
}

pub(crate) fn edit_state_legacy_id(kind: &str, target: &str) -> String {
    format!("{kind}:{target}")
}

pub(crate) fn edit_state_write(conn: &Connection, kind: &str, target: &str, size: i64, mtime: i64, ops: &str, updated_at: i64) -> Result<(), String> {
    conn.execute(
        "INSERT INTO edit_state(id, kind, target, size, mtime, ops, updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(id) DO UPDATE SET target=excluded.target, size=excluded.size, mtime=excluded.mtime, ops=excluded.ops, updated_at=excluded.updated_at",
        params![edit_state_id(kind, target), kind, enc_str(target), size, mtime, enc_str(ops), updated_at],
    )
    .map_err(|e| e.to_string())?;
    // Drop the legacy plain-path row for the same state.
    let _ = conn.execute("DELETE FROM edit_state WHERE id = ?1", params![edit_state_legacy_id(kind, target)]);
    Ok(())
}

// Rewrites every row whose target or ops are still in clear through the sealing
// writer, keeping each row's own timestamp. Returns how many rows it rewrote; a
// second run finds nothing left to do.
pub(crate) fn edit_state_seal_all(conn: &Connection) -> usize {
    type Row = (String, String, i64, i64, String, String, i64);
    let rows: Vec<Row> = {
        let Ok(mut st) = conn.prepare("SELECT kind, target, size, mtime, ops, id, updated_at FROM edit_state WHERE target NOT LIKE 'enc:v1:%' OR ops NOT LIKE 'enc:v1:%'") else {
            return 0;
        };
        let Ok(it) = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))) else {
            return 0;
        };
        it.flatten().collect()
    };
    let mut done = 0;
    for (kind, target, size, mtime, ops, old_id, updated_at) in rows {
        let plain_target = dec_str(&target);
        let plain_ops = dec_str(&ops);
        if edit_state_write(conn, &kind, &plain_target, size, mtime, &plain_ops, updated_at).is_ok() {
            // A row that sat under some other id (legacy form) is dropped once the hashed one exists.
            if old_id != edit_state_id(&kind, &plain_target) {
                let _ = conn.execute("DELETE FROM edit_state WHERE id = ?1", params![old_id]);
            }
            done += 1;
        }
    }
    done
}

// Points every saved editor state whose source file lay below the old data folder at the same
// file in the new one. A row is keyed by a hash of its path, so it is rewritten under the new
// key and the old one dropped, keeping its timestamp. Returns how many rows moved.
pub(crate) fn rebase_edit_state(conn: &Connection, old: &Path, new: &Path) -> Result<usize, String> {
    type Row = (String, String, i64, i64, String, String, i64);
    let rows: Vec<Row> = {
        let mut st = conn
            .prepare("SELECT kind, target, size, mtime, ops, id, updated_at FROM edit_state")
            .map_err(|e| e.to_string())?;
        let it = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))
            .map_err(|e| e.to_string())?;
        it.flatten().collect()
    };
    let mut moved = 0;
    for (kind, target, size, mtime, ops, old_id, updated_at) in rows {
        let Some(new_target) = rebase_media_path(&dec_str(&target), old, new) else { continue };
        edit_state_write(conn, &kind, &new_target, size, mtime, &dec_str(&ops), updated_at)?;
        if old_id != edit_state_id(&kind, &new_target) {
            conn.execute("DELETE FROM edit_state WHERE id = ?1", params![old_id]).map_err(|e| e.to_string())?;
        }
        moved += 1;
    }
    Ok(moved)
}

// (row id, ops as stored, decrypted) for the exact (size, mtime) triple, hashed id first.
pub(crate) fn edit_state_read(conn: &Connection, kind: &str, target: &str, size: i64, mtime: i64) -> Option<(String, String)> {
    for id in [edit_state_id(kind, target), edit_state_legacy_id(kind, target)] {
        let row = conn
            .query_row(
                "SELECT ops, target, updated_at FROM edit_state WHERE id = ?1 AND size = ?2 AND mtime = ?3",
                params![id, size, mtime],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)),
            )
            .ok();
        if let Some((ops, stored_target, updated_at)) = row {
            let plain_ops = dec_str(&ops);
            // A row stored in clear is sealed the first time it is read: rewritten under the hashed id
            // (the legacy row is dropped), keeping its timestamp so the retention clock does not restart.
            if !ops.starts_with(ENC_PREFIX) || !stored_target.starts_with(ENC_PREFIX) {
                let _ = edit_state_write(conn, kind, target, size, mtime, &plain_ops, updated_at);
                let hashed = edit_state_id(kind, target);
                return Some((hashed, plain_ops));
            }
            return Some((id, plain_ops));
        }
    }
    None
}

pub(crate) fn edit_state_discard(conn: &Connection, kind: &str, target: &str) -> Result<(), String> {
    for id in [edit_state_id(kind, target), edit_state_legacy_id(kind, target)] {
        conn.execute("DELETE FROM edit_state WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn save_edit_state(app: AppHandle, kind: String, target: String, size: i64, mtime: i64, ops: String) -> Result<(), String> {
    let conn = db_conn(&app).ok_or_else(|| "db unavailable".to_string())?;
    let updated_at = (now_ms() / 1000) as i64;
    edit_state_write(&conn, &kind, &target, size, mtime, &ops, updated_at)
}
#[tauri::command]
pub(crate) async fn load_edit_state(app: AppHandle, kind: String, target: String, size: i64, mtime: i64) -> Result<Option<String>, String> {
    let conn = db_conn(&app).ok_or_else(|| "db unavailable".to_string())?;
    let Some((id, ops)) = edit_state_read(&conn, &kind, &target, size, mtime) else { return Ok(None) };
    // Malformed JSON or an unknown "v" must never reach the editor, which would try to replay
    // operations it cannot understand. Decided here so "usable" is defined in one place; the
    // caller gets an empty editor, and the discard is logged so real corruption stays visible.
    let is_known_version = edit_state_ops_version(&ops) == Some(1);
    if !is_known_version {
        // The file name only: the full path would put the recording location back into
        // the log that sealing the row keeps out of the database.
        let shown = std::path::Path::new(&target).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        append_crash_log(&format!("load_edit_state: discarding unreadable saved state for {shown}"));
        let _ = conn.execute("DELETE FROM edit_state WHERE id = ?1", params![id]);
        return Ok(None);
    }
    Ok(Some(ops))
}
// A minimal sniff, not a full parse of every op inside: this table's whole point
// is staying opaque to the individual op shapes JS invents, it only ever needs to
// know "can any of this be understood at all".
pub(crate) fn edit_state_ops_version(ops: &str) -> Option<i64> {
    serde_json::from_str::<serde_json::Value>(ops)
        .ok()?
        .get("v")?
        .as_i64()
}
#[tauri::command]
pub(crate) async fn discard_edit_state(app: AppHandle, kind: String, target: String) -> Result<(), String> {
    let conn = db_conn(&app).ok_or_else(|| "db unavailable".to_string())?;
    edit_state_discard(&conn, &kind, &target)
}

// ---- Editor draft autosave ----
// A single row (id = "current"): only one prompt editor modal is open at a time. prompt_id
// is the edited entry's id, or "neu" for a still-unsaved new entry. Only the text side
// (name/text/variants) is drafted; an attached image stays with the prompt itself.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EditorDraft {
    pub(crate) prompt_id: String,
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) variants: String,
    pub(crate) updated_at: i64,
}

// The text side is sealed like a prompt is: a draft holds whatever the user
// was typing, and a plain row would leak it into data.db and every backup.
// Old unsealed rows still read back (dec_str passes plaintext through).
pub(crate) fn editor_draft_write(conn: &Connection, prompt_id: &str, name: &str, text: &str, variants: &str, updated_at: i64) -> Result<(), String> {
    conn.execute(
        "INSERT INTO editor_drafts(id, prompt_id, name, text, variants, updated_at) VALUES('current',?1,?2,?3,?4,?5)
         ON CONFLICT(id) DO UPDATE SET prompt_id=excluded.prompt_id, name=excluded.name, text=excluded.text, variants=excluded.variants, updated_at=excluded.updated_at",
        params![prompt_id, enc_str(name), enc_str(text), enc_str(variants), updated_at],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

pub(crate) fn editor_draft_read(conn: &Connection) -> Option<EditorDraft> {
    let (prompt_id, raw_name, raw_text, raw_variants, updated_at) = conn
        .query_row(
            "SELECT prompt_id, name, text, variants, updated_at FROM editor_drafts WHERE id = 'current'",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?)),
        )
        .ok()?;
    let (name, text, variants) = (dec_str(&raw_name), dec_str(&raw_text), dec_str(&raw_variants));
    // A sealed draft that no longer decrypts (data.db restored on another account or
    // machine) reads back empty: that is no draft, never an empty one to "restore".
    let sealed = [&raw_name, &raw_text, &raw_variants].iter().any(|v| v.starts_with(ENC_PREFIX));
    if sealed && name.is_empty() && text.is_empty() && variants.is_empty() {
        let _ = conn.execute("DELETE FROM editor_drafts WHERE id = 'current'", []);
        return None;
    }
    // An old draft written in clear is sealed on this first read.
    if ![&raw_name, &raw_text, &raw_variants].iter().all(|v| v.is_empty() || v.starts_with(ENC_PREFIX)) {
        let _ = editor_draft_write(conn, &prompt_id, &name, &text, &variants, updated_at);
    }
    Some(EditorDraft { prompt_id, name, text, variants, updated_at })
}

#[tauri::command]
pub(crate) async fn save_editor_draft(app: AppHandle, prompt_id: String, name: String, text: String, variants: String) -> Result<(), String> {
    let conn = db_conn(&app).ok_or_else(|| "db unavailable".to_string())?;
    let updated_at = (now_ms() / 1000) as i64;
    editor_draft_write(&conn, &prompt_id, &name, &text, &variants, updated_at)
}

#[tauri::command]
pub(crate) async fn load_editor_draft(app: AppHandle) -> Result<Option<EditorDraft>, String> {
    let conn = db_conn(&app).ok_or_else(|| "db unavailable".to_string())?;
    Ok(editor_draft_read(&conn))
}

#[tauri::command]
pub(crate) async fn discard_editor_draft(app: AppHandle) -> Result<(), String> {
    let conn = db_conn(&app).ok_or_else(|| "db unavailable".to_string())?;
    conn.execute("DELETE FROM editor_drafts WHERE id = 'current'", []).map_err(|e| e.to_string())?;
    Ok(())
}

// Shared by delete_prompt and batch_prompts's "delete" branch: a draft outlives the modal
// that wrote it, so it must not outlive its entry. `matches` decides on the draft's own prompt_id.
pub(crate) fn discard_editor_draft_if(conn: &Connection, matches: impl Fn(&str) -> bool) {
    let target: Option<String> = conn
        .query_row("SELECT prompt_id FROM editor_drafts WHERE id = 'current'", [], |r| r.get(0))
        .ok();
    if let Some(target) = target {
        if matches(&target) {
            let _ = conn.execute("DELETE FROM editor_drafts WHERE id = 'current'", []);
        }
    }
}

// Periodic cleanup for edit_state, run through the job scheduler. A row disappears once its
// source file is gone (deleted from the library, swept from the recordings folder, or aged
// out of the "verworfen" folder) without hooks into those call sites. keep_days = 0 means
// "never expire by age" (like trashRetentionDays); a missing file is removed on the next tick
// regardless. `exists` is a parameter so a test can use paths that are not real files.
pub(crate) fn sweep_edit_state_conn(conn: &Connection, exists: impl Fn(&str) -> bool, keep_days: f64) -> usize {
    let cutoff = (now_ms() / 1000) as i64 - (keep_days.max(0.0) * 86400.0) as i64;
    let mut stmt = match conn.prepare("SELECT id, target, updated_at FROM edit_state") {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let rows: Vec<(String, String, i64)> = match stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))) {
        Ok(it) => it.filter_map(|r| r.ok()).collect(),
        Err(_) => return 0,
    };
    drop(stmt);
    let mut removed = 0usize;
    for (id, target, updated_at) in rows {
        let target = dec_str(&target);
        let gone = !exists(&target);
        let expired = keep_days > 0.0 && updated_at < cutoff;
        if (gone || expired) && conn.execute("DELETE FROM edit_state WHERE id = ?1", params![id]).is_ok() {
            removed += 1;
        }
    }
    removed
}

// Re-read every round, same as recording_sweep_interval, so a changed value takes
// effect without a restart.
pub(crate) fn edit_state_keep_days(app: &AppHandle) -> f64 {
    app.try_state::<Db>()
        .map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).settings.ui_values.get("editStateKeepDays").copied().unwrap_or(30.0))
        .unwrap_or(30.0)
        .max(0.0)
}

pub(crate) fn sweep_edit_state_tick(app: &AppHandle) {
    let Some(conn) = db_conn(app) else { return };
    sweep_edit_state_conn(&conn, |p| std::path::Path::new(p).exists(), edit_state_keep_days(app));
}

// The edit-states row's on-demand trigger: runs the same rule as the periodic tick, so a
// click removes exactly what the next tick would have. It returns a row count only, since
// sweep_edit_state_conn reports nothing more.
#[tauri::command(async)]
pub(crate) fn sweep_edit_states_now(app: AppHandle) -> u32 {
    let Some(conn) = db_conn(&app) else { return 0 };
    sweep_edit_state_conn(&conn, |p| std::path::Path::new(p).exists(), edit_state_keep_days(&app)) as u32
}

// Emergency purge: something sensitive (a password) in the clipboard inbox may also sit in
// the rotating backup snapshots, DPAPI-sealed but on disk. This walks every backup folder,
// opens its snapshot, deletes clip_inbox rows if any, and VACUUMs so the bytes are really
// overwritten (a bare DELETE leaves them in SQLite's free pages). Returns (purged, failed):
// a snapshot that could not be opened, or a scrub not confirmed written back, must never be
// folded into "nothing to purge", so the caller can tell which backups may still hold the data.
#[tauri::command]
pub(crate) async fn purge_clip_inbox_from_backups(app: AppHandle) -> Result<(usize, usize), String> {
    Ok(purge_clip_inbox_from_backups_impl(&backups_dir(&app)))
}

// Empties clip_inbox in one snapshot and repacks it so the freed pages are
// overwritten. Some(rows removed) only when both steps worked: a delete whose
// repack failed leaves the old bytes in the free pages, so it is no scrub.
pub(crate) fn scrub_clip_inbox(conn: &Connection) -> Option<i64> {
    let removed = conn.execute("DELETE FROM clip_inbox", []).ok()? as i64;
    if removed > 0 && conn.execute("VACUUM", []).is_err() {
        return None;
    }
    Some(removed)
}

pub(crate) fn purge_clip_inbox_from_backups_impl(dir: &std::path::Path) -> (usize, usize) {
    let mut purged = 0usize;
    let mut failed = 0usize;
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let folder = entry.path();
        if !folder.is_dir() {
            continue;
        }
        let Some(db) = backup_db_file(&folder) else { continue };
        let is_gz = db.extension().is_some_and(|e| e == "gz");
        let staged_tmp = folder.join("purge.tmp");
        let staged = if is_gz {
            match gunzip_to(&db, &staged_tmp) {
                Ok(()) => staged_tmp.clone(),
                // Could not even look inside: count as a failure, since it might hold exactly what this scrubs.
                Err(_) => {
                    failed += 1;
                    continue;
                }
            }
        } else {
            db.clone()
        };
        let mut removed = 0i64;
        let mut inspected = false;
        if let Ok(conn) = Connection::open(&staged) {
            let has_table: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='clip_inbox'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if has_table > 0 {
                // None here means "could not confirm the rows are actually gone"
                // (the delete or the repack failed): inspected stays false,
                // counted as a failure below.
                if let Some(n) = scrub_clip_inbox(&conn) {
                    inspected = true;
                    removed = n;
                }
            } else {
                inspected = true; // genuinely nothing to remove, not a failure
            }
        }
        if !inspected {
            failed += 1;
        } else if removed > 0 {
            let repacked = if is_gz { gzip_to(&staged, &db) } else { Ok(()) };
            if repacked.is_ok() {
                purged += 1;
            } else {
                // Scrubbed in the decompressed copy, but it never made it back
                // into the actual backup file: the original rows are untouched.
                failed += 1;
            }
        }
        if is_gz {
            let _ = fs::remove_file(&staged_tmp);
        }
    }
    (purged, failed)
}

// Set a floating window's position AND size together (logical px). Used by the
// edge/corner resize so the grabbed edge tracks the cursor 1:1.
#[tauri::command]
pub(crate) async fn set_float_bounds(
    app: AppHandle,
    id: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    clamp: Option<bool>,
) -> Result<(), String> {
    if [x, y, width, height].iter().all(|v| v.is_finite()) {
        if let Some(win) = app.get_webview_window(&flabel(&id)) {
            let w = width.clamp(48.0, 8000.0);
            let h = height.clamp(48.0, 8000.0);
            // OPT-IN, and only the pill menu asks for it: that placement picks a side that fits and
            // falls back to "below" when none does. Every other caller has already worked out its exact
            // box (a live edge-resize computes the origin from the size it is about to set), and clamping
            // would move the window out from under the cursor near a screen edge.
            let (x, y) = if clamp.unwrap_or(false) {
                let dpi = win.scale_factor().unwrap_or(1.0);
                let work = work_rect_at(&app, x * dpi, y * dpi);
                let (cx, cy) = clamp_to_work(work, w * dpi, h * dpi, x * dpi, y * dpi);
                (cx / dpi, cy / dpi)
            } else {
                (x, y)
            };
            #[cfg(windows)]
            if set_float_bounds_native(&win, x, y, w, h) {
                return Ok(());
            }
            let _ = win.set_position(tauri::LogicalPosition::new(x, y));
            let _ = win.set_size(tauri::LogicalSize::new(w, h));
        }
    }
    Ok(())
}
