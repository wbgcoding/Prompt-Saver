use super::*;

// ---- Usage statistics ----
#[derive(Serialize)]
pub(crate) struct StatBar {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) count: u32,
}
// One day's copy count. `ts` is that day's UTC midnight (unix seconds); the frontend
// formats dates with Intl, so no date crate is needed here.
#[derive(Serialize)]
pub(crate) struct DayCount {
    pub(crate) ts: u64,
    pub(crate) count: usize,
}
// What the statistics dialog shows; the full numbers live in the export.
#[derive(Serialize)]
pub(crate) struct UsageStats {
    pub(crate) total_prompts: usize,
    pub(crate) copies7: u64,
    pub(crate) top: Vec<StatBar>,
    pub(crate) never_used: Vec<StatBar>,
    pub(crate) history_on: bool,
}

// ---- Most-used tools ----
// The functions this counts, in the same usage/last_used maps a prompt copy uses (see
// record_tool_usage), under a key namespace no real prompt id can collide with (ids come
// from gen_id(), never the fixed "tool:" prefix). It is also the single list a test checks
// counted keys against, so a new tool call site cannot go uncounted unnoticed.
pub(crate) const TOOL_KEYS: [&str; 13] = [
    "screenshot",
    "screenshot_editor",
    "ocr",
    "recording_start",
    "video_editor",
    "video_export",
    "dictation",
    "color_pick",
    "chain",
    "rotation_advance",
    "folder_create",
    "duplicate_finder",
    "backup_create",
];

pub(crate) fn tool_usage_key(tool: &str) -> String {
    format!("tool:{tool}")
}

// Same "must be explicitly turned on" reading as the frontend's own shipsOn:
// an opt-in flag defaults to off, so only an explicit `true` counts as on.
pub(crate) fn tool_usage_enabled(settings: &Settings) -> bool {
    settings.ui_flags.get("usageStats") == Some(&true)
}

// The increment itself, apart from the command below so it is testable without a mock
// Tauri runtime (State<'_, Db> cannot be built in a unit test).
pub(crate) fn apply_tool_usage(settings: &mut Settings, tool: &str) {
    let key = tool_usage_key(tool);
    let slot = settings.usage.entry(key.clone()).or_insert(0);
    *slot += 1;
    settings.last_used.insert(key, now_secs());
}

// Record one use of a tool, called by the UI right after the action succeeds. Does nothing
// while usage statistics are off: unlike record_copy's usage/last_used (always collected),
// this counter set exists only for the dashboard, so nothing of it should survive the
// switch being off.
#[tauri::command]
pub(crate) async fn record_tool_usage(state: State<'_, Db>, tool: String) -> Result<(), String> {
    if !TOOL_KEYS.contains(&tool.as_str()) {
        return Err("unknown-tool".to_string());
    }
    let mut store = lock(&state);
    if !tool_usage_enabled(&store.settings) {
        return Ok(());
    }
    apply_tool_usage(&mut store.settings, &tool);
    save_settings_soon();
    Ok(())
}

// Coarse type of a prompt for the statistics breakdown.
pub(crate) fn prompt_stat_type(p: &Prompt) -> &'static str {
    if !p.file_path.is_empty() {
        let lc = p.file_path.to_lowercase();
        if [".mp4", ".webm", ".mkv", ".mov", ".avi", ".m4v"].iter().any(|e| lc.ends_with(e)) {
            "video"
        } else if lc.ends_with(".pdf") {
            "pdf"
        } else if [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".tiff", ".tga", ".ico"].iter().any(|e| lc.ends_with(e)) {
            "image"
        } else {
            "file"
        }
    } else if !p.image.is_empty() {
        "image"
    } else {
        "text"
    }
}

// Aggregate everything in Rust so a 1000-prompt library returns one compact struct.
#[tauri::command]
pub(crate) async fn usage_stats(app: AppHandle, state: State<'_, Db>, top_n: usize) -> Result<UsageStats, String> {
    // The copy journal lives in SQLite and does not need the in-memory store, so it is read
    // before the store is locked; running the query under that lock made every copy and save
    // wait for the statistics dialog to open.
    let now = now_secs();
    let copies7 = db_conn(&app)
        .map(|c| {
            c.query_row("SELECT COUNT(*) FROM copy_log WHERE ts >= ?1", params![now.saturating_sub(7 * 86400) as i64], |r| r.get::<_, i64>(0))
                .unwrap_or(0) as u64
        })
        .unwrap_or(0);

    let store = lock(&state);
    let usage = &store.settings.usage;
    let name_of = stats_display_name;
    let total_prompts = store.prompts.len();
    let mut used: Vec<&Prompt> = store
        .prompts
        .iter()
        .filter(|p| usage.get(&p.id).copied().unwrap_or(0) > 0)
        .collect();
    used.sort_by(|a, b| {
        let ua = usage.get(&a.id).copied().unwrap_or(0);
        let ub = usage.get(&b.id).copied().unwrap_or(0);
        ub.cmp(&ua).then_with(|| name_of(a).to_lowercase().cmp(&name_of(b).to_lowercase()))
    });
    let top: Vec<StatBar> = used
        .iter()
        .take(top_n)
        .map(|p| StatBar { id: p.id.clone(), name: name_of(p), count: usage.get(&p.id).copied().unwrap_or(0) })
        .collect();
    // An archived-and-unused prompt is not flagged here too -- the user
    // already set it aside on purpose, so it does not need a second nudge.
    let archive_tag_key = store.settings.ui_texts.get("archiveTag").map(|s| tag_key(s));
    let mut never_used: Vec<StatBar> = store
        .prompts
        .iter()
        // Never copied: not "not lately", so the list means exactly what it says.
        .filter(|p| usage.get(&p.id).copied().unwrap_or(0) == 0)
        .filter(|p| !is_archived(p, archive_tag_key.as_deref()))
        .map(|p| StatBar { id: p.id.clone(), name: name_of(p), count: usage.get(&p.id).copied().unwrap_or(0) })
        .collect();
    never_used.truncate(top_n.max(10) * 2);
    let history_on = store.settings.ui_flags.get("copyHistory") != Some(&false);
    Ok(UsageStats { total_prompts, copies7, top, never_used, history_on })
}

// ---- Usage statistics export ----
// One row per prompt (not just the dashboard's top-N) plus the daily copy counts of the
// last 14 days and every tool counter, for a spreadsheet someone can sort and filter.
// `last_used`/`ts` stay raw unix seconds: the dashboard formats them through Intl on the JS
// side, and a calendar calculation here would be a second place for leap-year bugs.
#[derive(Serialize)]
pub(crate) struct StatsExportEntry {
    pub(crate) name: String,
    pub(crate) copies: u32,
    pub(crate) last_used: u64,
}

// The label a statistics row shows. A confidential prompt's text can be sealed
// ciphertext, so it never stands in for a missing name; the id does.
pub(crate) fn stats_display_name(p: &Prompt) -> String {
    if !p.name.is_empty() {
        p.name.clone()
    } else if !p.text.is_empty() && !p.secret {
        p.text.chars().take(40).collect()
    } else {
        p.id.clone()
    }
}

// One export row per prompt in the store.
pub(crate) fn stats_export_entries(store: &Store) -> Vec<StatsExportEntry> {
    let usage = &store.settings.usage;
    let last_used = &store.settings.last_used;
    let name_of = stats_display_name;
    let mut entries: Vec<StatsExportEntry> = store
        .prompts
        .iter()
        .map(|p| StatsExportEntry {
            name: name_of(p),
            copies: usage.get(&p.id).copied().unwrap_or(0),
            last_used: last_used.get(&p.id).copied().unwrap_or(0),
        })
        .collect();
    entries.sort_by(|a, b| b.copies.cmp(&a.copies).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    entries
}

pub(crate) fn stats_to_csv(entries: &[StatsExportEntry], daily: &[DayCount], tools: &[StatsExportEntry]) -> String {
    let mut rows = vec![csv_row(&["name", "copies", "last_used"])];
    for e in entries {
        rows.push(csv_row(&[&e.name, &e.copies.to_string(), &e.last_used.to_string()]));
    }
    rows.push(String::new());
    rows.push(csv_row(&["date_ts", "copies"]));
    for d in daily {
        rows.push(csv_row(&[&d.ts.to_string(), &d.count.to_string()]));
    }
    if !tools.is_empty() {
        rows.push(String::new());
        rows.push(csv_row(&["tool", "uses", "last_used"]));
        for t in tools {
            rows.push(csv_row(&[&t.name, &t.copies.to_string(), &t.last_used.to_string()]));
        }
    }
    rows.join("\r\n")
}

pub(crate) fn stats_to_txt(entries: &[StatsExportEntry], daily: &[DayCount], tools: &[StatsExportEntry]) -> String {
    use std::fmt::Write;
    let mut out = String::from("Prompts\n=======\n");
    for e in entries {
        let _ = writeln!(out, "{}: {} copies, last used {}", e.name, e.copies, e.last_used);
    }
    out.push_str("\nDaily copies\n============\n");
    for d in daily {
        let _ = writeln!(out, "{}: {}", d.ts, d.count);
    }
    if !tools.is_empty() {
        out.push_str("\nTool usage\n==========\n");
        for t in tools {
            let _ = writeln!(out, "{}: {} uses, last used {}", t.name, t.copies, t.last_used);
        }
    }
    out
}

pub(crate) fn stats_to_json(entries: &[StatsExportEntry], daily: &[DayCount], tools: &[StatsExportEntry]) -> String {
    #[derive(Serialize)]
    struct StatsExport<'a> {
        version: u32,
        entries: &'a [StatsExportEntry],
        daily: &'a [DayCount],
        tools: &'a [StatsExportEntry],
    }
    serde_json::to_string_pretty(&StatsExport { version: 1, entries, daily, tools }).unwrap_or_default()
}

// Same shape as export_prompts (format select, save dialog), fed from the statistics
// dashboard's own button so the export action sits next to the data on screen.
#[tauri::command]
pub(crate) async fn export_stats(app: AppHandle, state: State<'_, Db>, format: String) -> Result<usize, String> {
    let daily = db_conn(&app).map(|c| db_daily_copies(&c, 14)).unwrap_or_default();
    let (entries, tools) = {
        let store = lock(&state);
        let entries = stats_export_entries(&store);
        // Every tool, not just the dashboard's top-N: the export holds everything, the dashboard
        // only the top slice.
        let tools: Vec<StatsExportEntry> = TOOL_KEYS
            .iter()
            .map(|k| StatsExportEntry {
                name: (*k).to_string(),
                copies: store.settings.usage.get(&tool_usage_key(k)).copied().unwrap_or(0),
                last_used: store.settings.last_used.get(&tool_usage_key(k)).copied().unwrap_or(0),
            })
            .collect();
        (entries, tools)
    };
    let count = entries.len();
    let content = match format.as_str() {
        "csv" => stats_to_csv(&entries, &daily, &tools),
        "txt" => stats_to_txt(&entries, &daily, &tools),
        "json" => stats_to_json(&entries, &daily, &tools),
        _ => return Err("unsupported-export-format".to_string()),
    };
    let file = file_dialog(&app)
        .set_file_name(format!("usage-stats.{format}"))
        .add_filter(format.to_uppercase(), &[format.as_str()])
        .save_file();
    match file {
        Some(path) => {
            write_bytes_atomic(&path, content.as_bytes()).map_err(|e| e.to_string())?;
            Ok(count)
        }
        None => Err("canceled".to_string()),
    }
}

// ---- Storage breakdown (statistics/backups window, storage section) ----
// One row per storage kind the app writes to disk. `key` is a stable id the frontend
// switches on; `path` is what an "open folder" button reveals, empty for a row with no
// single folder.
#[derive(Serialize, Clone)]
pub(crate) struct StorageItem {
    pub(crate) key: String,
    pub(crate) path: String,
    pub(crate) bytes: u64,
    pub(crate) count: u64,
    // The scan stopped at the entry cap, so the numbers are a lower bound. Kept apart from
    // `denied` so the frontend can word the two cases differently.
    pub(crate) capped: bool,
    // The folder exists but could not be listed (a permissions problem): reported as such
    // instead of as an empty 0 B row.
    pub(crate) denied: bool,
}

#[derive(Serialize, Clone)]
pub(crate) struct StorageBreakdown {
    pub(crate) items: Vec<StorageItem>,
    pub(crate) total_bytes: u64,
    pub(crate) measured_at: u64,
}

pub(crate) fn file_size_or_zero(path: &std::path::Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

// (count, bytes) of every distinct, still-existing attachment path: a file shared by two
// prompts (a rotating entry's variants, say) counts once, and a path that no longer
// resolves adds nothing instead of failing the readout.
pub(crate) fn attachments_bytes(file_paths: &[String]) -> (u64, u64) {
    let mut seen = std::collections::HashSet::new();
    let mut bytes = 0u64;
    let mut count = 0u64;
    for path in file_paths {
        if path.is_empty() || !seen.insert(path.as_str()) {
            continue;
        }
        if let Ok(meta) = fs::metadata(path) {
            bytes += meta.len();
            count += 1;
        }
    }
    (count, bytes)
}

// (count, bytes, capped, denied) for one folder, one level deep (never descending into
// subfolders, like video_folder_stats_in). The scan stops after `cap` files so a folder
// with millions of small files cannot hang the window. `read_dir` is injected so a
// permissions failure can be tested without changing real ACLs.
pub(crate) fn scan_dir_stats_with(
    dir: &std::path::Path,
    cap: usize,
    read_dir: impl Fn(&std::path::Path) -> std::io::Result<std::fs::ReadDir>,
) -> (u64, u64, bool, bool) {
    let entries = match read_dir(dir) {
        Ok(e) => e,
        // A folder nothing has written to yet (the recordings folder before a first capture) is
        // simply empty, not an error.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (0, 0, false, false),
        Err(e) => return (0, 0, false, e.kind() == std::io::ErrorKind::PermissionDenied),
    };
    let mut count = 0u64;
    let mut bytes = 0u64;
    let mut capped = false;
    for entry in entries.flatten() {
        if count as usize >= cap {
            capped = true;
            break;
        }
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                count += 1;
                bytes += meta.len();
            }
        }
    }
    (count, bytes, capped, false)
}

pub(crate) fn scan_dir_stats(dir: &std::path::Path, cap: usize) -> (u64, u64, bool, bool) {
    scan_dir_stats_with(dir, cap, |p| std::fs::read_dir(p))
}

// (count, bytes, capped, denied) for the backups folder: each snapshot lives in its own
// subfolder, so the sizes are summed the way the backup list finds them (one snapshot file
// per folder). Counts snapshots; stops after `cap` of them.
pub(crate) fn backup_dir_stats(dir: &std::path::Path, cap: usize) -> (u64, u64, bool, bool) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (0, 0, false, false),
        Err(e) => return (0, 0, false, e.kind() == std::io::ErrorKind::PermissionDenied),
    };
    let (mut count, mut bytes) = (0u64, 0u64);
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(file) = backup_db_file(&path) else {
            continue;
        };
        if count as usize >= cap {
            return (count, bytes, true, false);
        }
        count += 1;
        bytes += std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    }
    (count, bytes, false, false)
}

// A runaway folder (a sync target pointed at something huge, say) stops
// counting here rather than freezing the window open while it walks on.
pub(crate) const STORAGE_SCAN_CAP: usize = 20_000;

#[tauri::command]
pub(crate) async fn storage_breakdown(app: AppHandle, state: State<'_, Db>, force: bool) -> Result<StorageBreakdown, String> {
    if !force {
        if let Some(cached) = STORAGE_CACHE.lock().unwrap().clone() {
            return Ok(cached);
        }
    }
    // The edit-state byte count lives in SQLite, so it is read before the
    // store is locked, same reasoning as usage_stats' copy_log reads.
    let (edit_state_count, edit_state_bytes) = db_conn(&app)
        .map(|c| {
            let count = c.query_row("SELECT COUNT(*) FROM edit_state", [], |r| r.get::<_, i64>(0)).unwrap_or(0) as u64;
            let bytes = c
                .query_row("SELECT COALESCE(SUM(LENGTH(ops)), 0) FROM edit_state", [], |r| r.get::<_, i64>(0))
                .unwrap_or(0) as u64;
            (count, bytes)
        })
        .unwrap_or((0, 0));
    let (file_paths, screenshot_custom, recording_custom) = {
        let store = lock(&state);
        (
            store.prompts.iter().map(|p| p.file_path.clone()).collect::<Vec<_>>(),
            store.settings.ui_texts.get("screenshotDir").cloned().unwrap_or_default(),
            store.settings.ui_texts.get("recordingDir").cloned().unwrap_or_default(),
        )
    };
    let dd = data_dir(&app);
    let mut items = Vec::new();

    // Database + its write-ahead log (and shared-memory file, when SQLite has
    // one open right now).
    {
        let db = dd.join("data.db");
        let wal = dd.join("data.db-wal");
        let shm = dd.join("data.db-shm");
        let bytes = file_size_or_zero(&db) + file_size_or_zero(&wal) + file_size_or_zero(&shm);
        let count = [&db, &wal, &shm].iter().filter(|p| p.exists()).count() as u64;
        items.push(StorageItem { key: "database".into(), path: db.to_string_lossy().into_owned(), bytes, count, capped: false, denied: false });
    }

    // Attachments: a prompt's file keeps only its original path, so there is no app folder to
    // scan; this sums whatever those paths currently point to.
    {
        let (count, bytes) = attachments_bytes(&file_paths);
        items.push(StorageItem { key: "attachments".into(), path: String::new(), bytes, count, capped: false, denied: false });
    }

    // Screenshots
    {
        let target = screenshot_dir(&screenshot_custom);
        let (count, bytes, capped, denied) = scan_dir_stats(&target, STORAGE_SCAN_CAP);
        items.push(StorageItem { key: "screenshots".into(), path: target.to_string_lossy().into_owned(), bytes, count, capped, denied });
    }

    // Recordings, plus the discard trash as a separate row: the same folder the expert menu's
    // recording-folder readout reads.
    {
        let target = recording_dir(&recording_custom);
        let (count, bytes) = video_folder_stats_in(&target);
        items.push(StorageItem { key: "recordings".into(), path: target.to_string_lossy().into_owned(), bytes, count, capped: false, denied: false });
        let discard = target.join("verworfen");
        let (dcount, dbytes) = video_folder_stats_in(&discard);
        items.push(StorageItem { key: "discarded_recordings".into(), path: discard.to_string_lossy().into_owned(), bytes: dbytes, count: dcount, capped: false, denied: false });
    }

    // Backups
    {
        let target = backups_dir(&app);
        let (count, bytes, capped, denied) = backup_dir_stats(&target, STORAGE_SCAN_CAP);
        items.push(StorageItem { key: "backups".into(), path: target.to_string_lossy().into_owned(), bytes, count, capped, denied });
    }

    // Temporary files: the scratch folder OCR, the language-pack install and
    // the update stager all write into.
    {
        let target = std::env::temp_dir().join(brand::TEMP_DIR);
        let (count, bytes, capped, denied) = scan_dir_stats(&target, STORAGE_SCAN_CAP);
        items.push(StorageItem { key: "temp_files".into(), path: target.to_string_lossy().into_owned(), bytes, count, capped, denied });
    }

    // Edit states: `ops` is the JSON blob actually held in the database; the
    // row's own `size` column is the edited file's size, not storage cost.
    items.push(StorageItem {
        key: "edit_states".into(),
        path: String::new(),
        bytes: edit_state_bytes,
        count: edit_state_count,
        capped: false,
        denied: false,
    });

    // Error log
    {
        let path = crash_log_path();
        let bytes = file_size_or_zero(&path);
        items.push(StorageItem {
            key: "crash_log".into(),
            path: path.to_string_lossy().into_owned(),
            bytes,
            count: u64::from(path.exists()),
            capped: false,
            denied: false,
        });
    }

    let total_bytes = items.iter().map(|i| i.bytes).sum();
    let fresh = StorageBreakdown { items, total_bytes, measured_at: now_secs() };
    *STORAGE_CACHE.lock().unwrap() = Some(fresh.clone());
    Ok(fresh)
}

// Cached across calls -- reopening the statistics/backups window should not
// re-walk every folder every time; the "measure again" button passes force=true.
pub(crate) static STORAGE_CACHE: Mutex<Option<StorageBreakdown>> = Mutex::new(None);

// Opens a storage-breakdown row's own folder -- the path travels from
// storage_breakdown's own computation, never from anything the user typed.
#[tauri::command(async)]
pub(crate) fn open_storage_folder(path: String) -> Result<(), String> {
    open_explorer_at(&path)
}

// One row per kept-data category (trash, copy history, clipboard inbox, recordings,
// discarded recordings, temporary exports, edit states) for the statistics/backups
// window's overview. Distinct from StorageBreakdown above, which is disk usage by folder:
// this is retention grouped by what a setting governs. It reuses each category's existing
// counting (video_folder_stats_in for the recording rows, the SQL other commands here run
// against copy_log/clip_inbox/edit_state) instead of a second way to measure the same thing.
#[derive(Serialize, Clone)]
pub(crate) struct RetentionItem {
    pub(crate) key: String,
    pub(crate) count: u64,
    pub(crate) bytes: u64,
}

#[derive(Serialize, Clone)]
pub(crate) struct RetentionOverview {
    pub(crate) items: Vec<RetentionItem>,
    pub(crate) measured_at: u64,
}

pub(crate) static RETENTION_CACHE: Mutex<Option<RetentionOverview>> = Mutex::new(None);

#[tauri::command]
pub(crate) async fn retention_overview(app: AppHandle, state: State<'_, Db>, force: bool) -> Result<RetentionOverview, String> {
    if !force {
        if let Some(cached) = RETENTION_CACHE.lock().unwrap().clone() {
            return Ok(cached);
        }
    }
    // Only the paths are collected under the lock; measuring them touches the disk.
    let (trash_count, trash_paths, recording_custom) = {
        let store = lock(&state);
        let trash_paths: Vec<String> = store.trash.iter().map(|p| p.file_path.clone()).collect();
        (store.trash.len() as u64, trash_paths, store.settings.ui_texts.get("recordingDir").cloned().unwrap_or_default())
    };
    let (_, trash_bytes) = attachments_bytes(&trash_paths);
    let (copy_count, copy_bytes) = db_conn(&app)
        .map(|c| {
            c.query_row("SELECT COUNT(*), COALESCE(SUM(LENGTH(id)), 0) FROM copy_log", [], |r| {
                Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64))
            })
            .unwrap_or((0, 0))
        })
        .unwrap_or((0, 0));
    let (clip_count, clip_bytes) = db_conn(&app)
        .map(|c| {
            c.query_row("SELECT COUNT(*), COALESCE(SUM(LENGTH(text)), 0) FROM clip_inbox", [], |r| {
                Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64))
            })
            .unwrap_or((0, 0))
        })
        .unwrap_or((0, 0));
    let rec_dir = recording_dir(&recording_custom);
    let (rec_count, rec_bytes) = video_folder_stats_in(&rec_dir);
    let (discarded_count, discarded_bytes) = video_folder_stats_in(&rec_dir.join("verworfen"));
    let (temp_count, temp_bytes) = clip_export_temp_stats();
    let (edit_count, edit_bytes) = db_conn(&app)
        .map(|c| {
            let count = c.query_row("SELECT COUNT(*) FROM edit_state", [], |r| r.get::<_, i64>(0)).unwrap_or(0) as u64;
            let bytes = c
                .query_row("SELECT COALESCE(SUM(LENGTH(ops)), 0) FROM edit_state", [], |r| r.get::<_, i64>(0))
                .unwrap_or(0) as u64;
            (count, bytes)
        })
        .unwrap_or((0, 0));
    let items = vec![
        RetentionItem { key: "trash".into(), count: trash_count, bytes: trash_bytes },
        RetentionItem { key: "copyHistory".into(), count: copy_count, bytes: copy_bytes },
        RetentionItem { key: "clipInbox".into(), count: clip_count, bytes: clip_bytes },
        RetentionItem { key: "recordings".into(), count: rec_count, bytes: rec_bytes },
        RetentionItem { key: "discardedRecordings".into(), count: discarded_count, bytes: discarded_bytes },
        RetentionItem { key: "tempExports".into(), count: temp_count, bytes: temp_bytes },
        RetentionItem { key: "editStates".into(), count: edit_count, bytes: edit_bytes },
    ];
    let fresh = RetentionOverview { items, measured_at: now_secs() };
    *RETENTION_CACHE.lock().unwrap() = Some(fresh.clone());
    Ok(fresh)
}

// ---- Duplicate finder ----
#[derive(Serialize)]
pub(crate) struct DupMember {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) usage: u32,
    pub(crate) views: Vec<String>,
}
#[derive(Serialize)]
pub(crate) struct DupGroup {
    pub(crate) score: f64,
    pub(crate) members: Vec<DupMember>,
}

pub(crate) fn normalize_dup(s: &str) -> String {
    s.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}
pub(crate) fn dup_trigrams(s: &str) -> std::collections::HashSet<[char; 3]> {
    let chars: Vec<char> = s.chars().collect();
    let mut set = std::collections::HashSet::new();
    for w in chars.windows(3) {
        set.insert([w[0], w[1], w[2]]);
    }
    set
}
pub(crate) fn dup_dice(a: &std::collections::HashSet<[char; 3]>, b: &std::collections::HashSet<[char; 3]>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.iter().filter(|t| b.contains(*t)).count();
    2.0 * inter as f64 / (a.len() + b.len()) as f64
}
pub(crate) fn uf_find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

// True if some previously-ignored group contains BOTH ids - a pair only
// counts as ignored when some single ignore_dups call covered them together,
// not just because each independently belongs to some (possibly different)
// ignored group.
pub(crate) fn dup_pair_is_ignored(groups: &[Vec<String>], a: &str, b: &str) -> bool {
    groups.iter().any(|g| g.iter().any(|id| id == a) && g.iter().any(|id| id == b))
}

// The few fields the duplicate scan reads. Cloning whole prompts under the store
// lock would copy every stored image (hundreds of kilobytes each) for nothing.
// A confidential entry is left out here: its stored text is ciphertext.
pub(crate) struct DupSource {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) text: String,
}

pub(crate) fn dup_sources(prompts: &[Prompt]) -> Vec<DupSource> {
    prompts
        .iter()
        .filter(|p| !p.secret)
        .map(|p| DupSource { id: p.id.clone(), name: p.name.clone(), text: p.text.clone() })
        .collect()
}

// Prompt id -> names of the views it belongs to. Membership is `View.members`
// (a prompt in a grid folder or without a cell on the current grid size still
// belongs); the per-size layouts are history and say nothing about it.
pub(crate) fn view_names_by_prompt(views: &[View]) -> std::collections::HashMap<&str, Vec<String>> {
    let mut out: std::collections::HashMap<&str, Vec<String>> = std::collections::HashMap::new();
    for v in views {
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for id in &v.members {
            if seen.insert(id.as_str()) {
                out.entry(id.as_str()).or_default().push(v.name.clone());
            }
        }
    }
    out
}

// Find near-duplicate prompts by trigram/Dice similarity. Rayon-parallel over the
// upper-triangle of pairs; 1000 prompts finish well under a second.
#[tauri::command]
pub(crate) async fn find_duplicates(state: State<'_, Db>, threshold: f64) -> Result<Vec<DupGroup>, String> {
    use rayon::prelude::*;
    use std::collections::{HashMap as Map, HashSet as Set};
    // Clone the few fields this scan needs and drop the lock before the O(n^2) comparison,
    // which would otherwise block every command needing the store mutex for as long as
    // comparing a few hundred prompts takes. The result is a snapshot: a prompt changed
    // mid-scan may be slightly stale in this one report, an acceptable trade for a read-only
    // finder.
    let (prompts, dup_ignored, views, usage) = {
        let store = lock(&state);
        (
            dup_sources(&store.prompts),
            store.settings.dup_ignored.clone(),
            store.settings.views.clone(),
            store.settings.usage.clone(),
        )
    };
    // Floor, not just a 0..1 clamp: at a threshold of 0 every pair passes and the
    // edge list below becomes the full n^2 triangle. The setting itself never goes
    // below 70 %, so this only bounds what a malformed call could ask for.
    let thr = (threshold / 100.0).clamp(0.5, 1.0);
    // A rotating prompt is compared on entry 1 (p.text) only: two prompts that share their
    // first entry but differ in the rest are not near duplicates, and comparing every entry
    // pair would turn this O(n^2) scan into O(n^2 * entries).
    let items: Vec<(&DupSource, String)> = prompts
        .iter()
        // Confidential entries were already left out of the snapshot (their stored
        // text is ciphertext, never a real body to compare).
        .map(|p| {
            let base = if !p.text.is_empty() { p.text.as_str() } else { p.name.as_str() };
            (p, normalize_dup(base))
        })
        .filter(|(_, n)| n.chars().count() >= 3)
        .collect();
    let n = items.len();
    if n < 2 {
        return Ok(Vec::new());
    }
    let grams: Vec<Set<[char; 3]>> = items.par_iter().map(|(_, s)| dup_trigrams(s)).collect();
    let edges: Vec<(usize, usize, f64)> = (0..n)
        .into_par_iter()
        .flat_map(|i| {
            let gi = &grams[i];
            let mut local = Vec::new();
            for j in (i + 1)..n {
                let d = dup_dice(gi, &grams[j]);
                if d >= thr && !dup_pair_is_ignored(&dup_ignored, &items[i].0.id, &items[j].0.id) {
                    local.push((i, j, d));
                }
            }
            local
        })
        .collect();
    // Union-find into connected components; track the strongest edge per group.
    let mut parent: Vec<usize> = (0..n).collect();
    let mut best: Map<usize, f64> = Map::new();
    for &(i, j, _) in &edges {
        let (ri, rj) = (uf_find(&mut parent, i), uf_find(&mut parent, j));
        if ri != rj {
            parent[ri] = rj;
        }
    }
    for &(i, _, d) in &edges {
        let r = uf_find(&mut parent, i);
        let e = best.entry(r).or_insert(0.0);
        if d > *e {
            *e = d;
        }
    }
    let mut groups: Map<usize, Vec<usize>> = Map::new();
    for i in 0..n {
        let r = uf_find(&mut parent, i);
        if best.contains_key(&r) {
            groups.entry(r).or_default().push(i);
        }
    }
    // Precompute view membership: prompt id -> view names containing it.
    let in_views = view_names_by_prompt(&views);
    let mut out: Vec<DupGroup> = groups
        .into_iter()
        .filter(|(_, m)| m.len() >= 2)
        .map(|(root, idxs)| {
            let members = idxs
                .iter()
                .map(|&i| {
                    let p = items[i].0;
                    DupMember {
                        id: p.id.clone(),
                        name: if p.name.is_empty() { p.text.chars().take(40).collect() } else { p.name.clone() },
                        text: p.text.clone(),
                        usage: usage.get(&p.id).copied().unwrap_or(0),
                        views: in_views.get(p.id.as_str()).cloned().unwrap_or_default(),
                    }
                })
                .collect();
            DupGroup { score: best.get(&root).copied().unwrap_or(0.0), members }
        })
        .collect();
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

// ---- Similar-on-save hint ----
// How long the after-save similarity scan may run before it gives up and reports nothing
// rather than be felt as a stall. find_duplicates needs no such budget: it is an explicit
// "scan now" action the user chose to wait for, while this runs after every ordinary save,
// unasked.
pub(crate) const SIMILAR_HINT_BUDGET: std::time::Duration = std::time::Duration::from_millis(40);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SimilarHit {
    pub(crate) id: String,
    pub(crate) name: String,
}

// Whether two normalized lengths could clear `thr` under trigram Dice similarity. Dice
// never exceeds 2*min/(min+max), even with the shorter fully inside the longer, so a pair
// whose lengths differ enough is provably impossible and skipped before paying for
// dup_trigrams on it.
pub(crate) fn dup_length_could_match(a: usize, b: usize, thr: f64) -> bool {
    if a == 0 || b == 0 {
        return false;
    }
    let (hi, lo) = if a > b { (a, b) } else { (b, a) };
    2.0 * lo as f64 / (hi + lo) as f64 >= thr
}

// The same trigram/Dice calculation find_duplicates uses, run against just the one prompt
// that was saved instead of every pair. Pure and synchronous, so a unit test can call it
// without a Db/AppHandle; the command below adds the store lookup. Time-boxed
// (SIMILAR_HINT_BUDGET) and length-filtered, so a 10,000-entry store does not turn an
// ordinary save into a stall; it runs after the save returned, so reaching the budget only
// delays the hint.
pub(crate) fn find_similar_prompt(target: &Prompt, others: &[&Prompt], threshold: f64) -> Option<SimilarHit> {
    let start = std::time::Instant::now();
    let thr = (threshold / 100.0).clamp(0.5, 1.0);
    let base = if !target.text.is_empty() { target.text.as_str() } else { target.name.as_str() };
    let target_norm = normalize_dup(base);
    let target_len = target_norm.chars().count();
    if target_len < 3 {
        return None;
    }
    let target_grams = dup_trigrams(&target_norm);
    let mut best: Option<(SimilarHit, f64)> = None;
    for p in others {
        if start.elapsed() > SIMILAR_HINT_BUDGET {
            break;
        }
        let base = if !p.text.is_empty() { p.text.as_str() } else { p.name.as_str() };
        let norm = normalize_dup(base);
        let len = norm.chars().count();
        if len < 3 || !dup_length_could_match(target_len, len, thr) {
            continue;
        }
        let d = dup_dice(&target_grams, &dup_trigrams(&norm));
        if d >= thr && best.as_ref().map_or(true, |(_, b)| d > *b) {
            best = Some((
                SimilarHit { id: p.id.clone(), name: if p.name.is_empty() { p.text.chars().take(40).collect() } else { p.name.clone() } },
                d,
            ));
        }
    }
    best.map(|(hit, _)| hit)
}

// The target and the prompts it may be compared with. A confidential prompt is
// neither compared nor offered as a match: its stored text can be ciphertext and
// its name must not surface as a hint. None when the target is missing or secret.
pub(crate) fn similar_inputs(prompts: &[Prompt], id: &str) -> Option<(Prompt, Vec<Prompt>)> {
    let target = prompts.iter().find(|p| p.id == id).filter(|p| !p.secret)?.clone();
    let others = prompts.iter().filter(|p| p.id != id && !p.secret).cloned().collect();
    Some((target, others))
}

// Called by the frontend after add_prompt/update_prompt already returned, never awaited as
// part of saving, so a slow scan can only delay the hint toast. A missing id (deleted in
// the instant between save and this call) is "nothing to report", not an error.
#[tauri::command]
pub(crate) async fn similar_to_saved(state: State<'_, Db>, id: String, threshold: f64) -> Result<Option<SimilarHit>, String> {
    // Snapshot and drop the lock before the scan, as find_duplicates does: the bounded but real
    // comparison must not block every other command.
    let Some((target, others)) = similar_inputs(&lock(&state).prompts, &id) else {
        return Ok(None);
    };
    let others_ref: Vec<&Prompt> = others.iter().collect();
    Ok(find_similar_prompt(&target, &others_ref, threshold))
}

// Mark this whole group of prompt ids as "not duplicates" (of each other).
// One entry for the whole group instead of every pairwise combination - see the
// dup_ignored field comment for why.
#[tauri::command]
pub(crate) async fn ignore_dups(app: AppHandle, state: State<'_, Db>, ids: Vec<String>) -> Result<(), String> {
    if ids.len() < 2 {
        return Ok(()); // nothing to ignore a duplicate relationship between
    }
    let mut store = lock(&state);
    let mut sorted = ids;
    sorted.sort();
    sorted.dedup();
    // Skip an exact repeat of an already-stored group (e.g. a double-click) so this
    // cannot grow unbounded from the same action fired twice.
    if !store.settings.dup_ignored.iter().any(|g| {
        let mut existing = g.clone();
        existing.sort();
        existing == sorted
    }) {
        store.settings.dup_ignored.push(sorted);
    }
    save_settings(&app, &store.settings);
    Ok(())
}
