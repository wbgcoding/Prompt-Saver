use super::*;

// ---- Automatic backups & restore ----
// Name + size/kind, so the expanded backup list can show a real per-prompt row. `kind` reuses
// prompt_stat_type()'s categories (text/image/video/pdf/file) as one shared classifier.
#[derive(Serialize, Deserialize)]
pub(crate) struct BackupPrompt {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) chars: usize,
    #[serde(default = "default_backup_kind")]
    pub(crate) kind: String,
}
pub(crate) fn default_backup_kind() -> String {
    "text".into()
}

// One entry in a snapshot's full content list: the fuller sibling of BackupPrompt, read on
// demand through the staged and validated file a restore uses rather than the small cached
// preview.
#[derive(Serialize)]
pub(crate) struct BackupContentItem {
    pub(crate) id: String,
    pub(crate) name: String,
    // Its most recent edit, from prompt_versions; None for a prompt only ever created (no
    // version row is written for that).
    pub(crate) date: Option<i64>,
    pub(crate) chars: usize,
    pub(crate) kind: String,
}

// The full picture of one snapshot, read on open rather than kept around: unlike
// BackupInfo's cached header numbers, it is fetched only while the content viewer for that
// row is showing.
#[derive(Serialize)]
pub(crate) struct BackupContents {
    pub(crate) entries: usize,
    pub(crate) views: usize,
    pub(crate) tags: usize,
    // The newest prompt_versions timestamp in the snapshot; None when nothing in it was ever
    // edited (promptHistory off the whole time, or untouched prompts).
    pub(crate) last_change: Option<i64>,
    // None for a snapshot made before this note existed.
    pub(crate) program_version: Option<String>,
    // Whether every row decrypted through the normal DPAPI seal. False only on the rare machine
    // where Windows refused to encrypt while the snapshot was made (see enc_str's plaintext
    // fallback); never about a password, since a local snapshot has none.
    pub(crate) encrypted: bool,
    pub(crate) items: Vec<BackupContentItem>,
}

#[derive(Serialize)]
pub(crate) struct BackupInfo {
    pub(crate) name: String,
    pub(crate) size: u64,
    // How many prompts the snapshot holds. Written next to the snapshot when it is made (exact
    // and free to read back); a backup from an older build has no note, so its count is read from
    // the snapshot once.
    pub(crate) prompts: Option<usize>,
    // The prompt names, with the same "written once, cheap to read back" treatment as `prompts`.
    // The snapshot beside it has every row DPAPI-sealed, so this cache is sealed too: plaintext
    // would hand anyone copying the data folder to another Windows account the full list of
    // prompt names while the snapshot stayed unreadable.
    pub(crate) items: Vec<BackupPrompt>,
}

pub(crate) const BACKUP_COUNT_FILE: &str = "prompts.count";
pub(crate) const BACKUP_NAMES_FILE: &str = "prompts.names.json";
// The app version that made a snapshot; missing for one made before this note existed, read
// back as "unknown" rather than guessed from the running build.
pub(crate) const BACKUP_VERSION_FILE: &str = "version.txt";

// Count the prompts in a snapshot folder: the note if there is one, otherwise the snapshot is
// unpacked to a temp file and counted (and the note written). None only when the folder
// holds no readable snapshot.
pub(crate) fn backup_prompt_count(folder: &std::path::Path) -> Option<usize> {
    if let Ok(text) = fs::read_to_string(folder.join(BACKUP_COUNT_FILE)) {
        if let Ok(n) = text.trim().parse::<usize>() {
            return Some(n);
        }
    }
    let db = backup_db_file(folder)?;
    let tmp;
    let path = if db.extension().map(|e| e == "gz").unwrap_or(false) {
        tmp = folder.join("count.tmp");
        gunzip_to(&db, &tmp).ok()?;
        tmp
    } else {
        db
    };
    let n = Connection::open(&path)
        .ok()
        .and_then(|c| c.query_row("SELECT COUNT(*) FROM prompts", [], |r| r.get::<_, i64>(0)).ok())
        .map(|n| n.max(0) as usize);
    let _ = fs::remove_file(folder.join("count.tmp"));
    if let Some(n) = n {
        let _ = fs::write(folder.join(BACKUP_COUNT_FILE), n.to_string());
    }
    n
}

// Total characters across a rotating prompt's entry 1 (`text`) and every variant: what the
// version history diff and the auto-advance treat as "the whole prompt".
pub(crate) fn prompt_char_count(p: &Prompt) -> usize {
    p.text.chars().count() + p.variants.iter().map(|v| v.chars().count()).sum::<usize>()
}

// Decrypt each row (the DPAPI seal `db_load` uses) and pull out name/size/kind. Best-effort:
// a row that fails to decrypt or parse is skipped rather than failing the list.
pub(crate) fn read_prompt_names(path: &std::path::Path) -> Vec<BackupPrompt> {
    let Ok(conn) = Connection::open(path) else { return Vec::new() };
    let Ok(mut stmt) = conn.prepare("SELECT data FROM prompts ORDER BY ord") else { return Vec::new() };
    let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) else { return Vec::new() };
    rows.flatten()
        .filter_map(|data| serde_json::from_str::<Prompt>(&dec_str(&data)).ok())
        .map(|p| BackupPrompt { name: p.name.clone(), chars: prompt_char_count(&p), kind: prompt_stat_type(&p).to_string() })
        .collect()
}

// The names cache carries real prompt titles, so it is DPAPI-sealed like every row in the
// snapshot beside it. `dec_str` passes an un-prefixed value through, so a plaintext cache
// from an earlier build still reads and is re-sealed on the next write.
pub(crate) fn write_prompt_names(cache: &std::path::Path, items: &[BackupPrompt]) {
    if let Ok(json) = serde_json::to_string(items) {
        let _ = fs::write(cache, enc_str(&json));
    }
}
// An older build's cache is a plain Vec<String>: try the current shape first, then fall back
// to strings with zero chars. The bool says whether the fallback fired, so the caller
// upgrades the cache file once instead of rewriting it on every read.
pub(crate) fn read_prompt_names_cache(cache: &std::path::Path) -> (Vec<BackupPrompt>, bool) {
    let Some(text) = fs::read_to_string(cache).ok().map(|raw| dec_str(&raw)) else { return (Vec::new(), false) };
    if let Ok(items) = serde_json::from_str::<Vec<BackupPrompt>>(&text) {
        return (items, false);
    }
    let items = serde_json::from_str::<Vec<String>>(&text)
        .unwrap_or_default()
        .into_iter()
        .map(|name| BackupPrompt { name, chars: 0, kind: default_backup_kind() })
        .collect();
    (items, true)
}

// Same cached-note treatment as `backup_prompt_count`: computed once (here, or at creation
// time in `do_backup`) and read back after.
pub(crate) fn backup_prompt_names(folder: &std::path::Path) -> Vec<BackupPrompt> {
    let cache = folder.join(BACKUP_NAMES_FILE);
    if cache.exists() {
        let (items, legacy) = read_prompt_names_cache(&cache);
        if legacy {
            write_prompt_names(&cache, &items);
        }
        return items;
    }
    let Some(db) = backup_db_file(folder) else { return Vec::new() };
    let tmp;
    let path = if db.extension().map(|e| e == "gz").unwrap_or(false) {
        tmp = folder.join("names.tmp");
        if gunzip_to(&db, &tmp).is_err() {
            return Vec::new();
        }
        tmp
    } else {
        db
    };
    let items = read_prompt_names(&path);
    let _ = fs::remove_file(folder.join("names.tmp"));
    // An empty list is cached only when the snapshot really holds no prompt: a failed read looks
    // the same and must not be pinned.
    if !items.is_empty() || backup_prompt_count(folder) == Some(0) {
        write_prompt_names(&cache, &items);
    }
    items
}
pub(crate) fn backups_dir(app: &AppHandle) -> std::path::PathBuf {
    data_dir(app).join("backups")
}
// A backup folder name is a local_stamp() (digits, '-', '_'): reject path separators or
// traversal so a crafted name can never escape the backups dir.
pub(crate) fn valid_backup_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && name.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '_')
}
pub(crate) fn list_backups_impl(app: &AppHandle) -> Vec<BackupInfo> {
    list_backups_impl_at(&backups_dir(app))
}

// Split out of list_backups_impl() for the command line's headless path, which has a folder
// to list but no AppHandle.
pub(crate) fn list_backups_impl_at(dir: &std::path::Path) -> Vec<BackupInfo> {
    let mut out: Vec<BackupInfo> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir() && backup_db_file(&e.path()).is_some())
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let size = backup_db_file(&e.path())
                .and_then(|p| fs::metadata(p).ok())
                .map(|m| m.len())
                .unwrap_or(0);
            let prompts = backup_prompt_count(&e.path());
            let items = backup_prompt_names(&e.path());
            BackupInfo { name, size, prompts, items }
        })
        .collect();
    out.sort_by(|a, b| b.name.cmp(&a.name)); // newest first (stamp sorts chronologically)
    out
}
// Grandfather-father-son retention: keep the newest N backups per time tier.
pub(crate) struct GfsConfig {
    pub(crate) daily: usize,
    pub(crate) weekly: usize,
    pub(crate) monthly: usize,
    pub(crate) yearly: usize,
}
pub(crate) fn gfs_from_settings(s: &Settings) -> Option<GfsConfig> {
    if s.ui_flags.get("backupGfs") != Some(&true) {
        return None;
    }
    let g = |k: &str, d: f64| s.ui_values.get(k).copied().unwrap_or(d).max(0.0) as usize;
    Some(GfsConfig {
        daily: g("backupDaily", 3.0),
        weekly: g("backupWeekly", 4.0),
        monthly: g("backupMonthly", 6.0),
        yearly: g("backupYearly", 1.0),
    })
}
// Days since 1970-01-01 for a proleptic Gregorian date (Hinnant's algorithm).
pub(crate) fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
// (day, week, month, year) tier keys from a backup folder name "YYYY-MM-DD_HH-MM-SS".
pub(crate) fn backup_date_keys(name: &str) -> Option<(i64, i64, i64, i64)> {
    let date = name.split('_').next()?;
    let mut it = date.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    let days = days_from_civil(y, m, d);
    Some((days, days.div_euclid(7), y * 12 + m, y))
}
// Which of `names` (sorted oldest first) rotation throws away. Pure, so the retention rules
// are testable without a disk full of folders.
pub(crate) fn backups_to_drop(names: &[String], keep: usize, gfs: Option<&GfsConfig>) -> Vec<String> {
    use std::collections::HashSet;
    let keep = keep.max(1); // a caller passing 0 must never mean "delete everything"
    let Some(g) = gfs else {
        let drop = names.len().saturating_sub(keep);
        return names[..drop].to_vec();
    };
    // Newest-first walk; for each tier keep the newest backup per distinct
    // day / week / month / year up to that tier's count.
    let pick = |count: usize, idx: usize| -> Vec<String> {
        let mut seen: HashSet<i64> = HashSet::new();
        let mut out = Vec::new();
        if count == 0 {
            return out;
        }
        for n in names.iter().rev() {
            if let Some(keys) = backup_date_keys(n) {
                let k = [keys.0, keys.1, keys.2, keys.3][idx];
                if seen.insert(k) {
                    out.push(n.clone());
                    if seen.len() >= count {
                        break;
                    }
                }
            }
        }
        out
    };
    // The newest `keep` snapshots survive whatever the tiers say. A tier keeps one backup per
    // distinct day, so without this a second backup made the same day would delete the first.
    let mut keep_set: HashSet<String> = names.iter().rev().take(keep).cloned().collect();
    for (count, idx) in [(g.daily, 0), (g.weekly, 1), (g.monthly, 2), (g.yearly, 3)] {
        for n in pick(count, idx) {
            keep_set.insert(n);
        }
    }
    // Every tier at zero would wipe the lot: keep the newest one instead.
    if keep_set.is_empty() {
        if let Some(newest) = names.last() {
            keep_set.insert(newest.clone());
        }
    }
    names.iter().filter(|n| !keep_set.contains(*n)).cloned().collect()
}
// Split out of do_backup() so the command line's headless path can drop old snapshots the
// same way from an already-resolved folder. A snapshot named in `protect` survives whatever
// the retention says: a restore takes its safety snapshot before it reads the chosen one,
// and without this the chosen (oldest) snapshot could be the folder rotation drops.
pub(crate) fn rotate_backups_at(dir: &std::path::Path, keep: usize, gfs: Option<GfsConfig>, protect: Option<&str>) {
    // Only folders holding a snapshot count: one still being packed (or left half-made by a
    // quit) must neither use up a slot nor push a good one out.
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir() && backup_db_file(&e.path()).is_some())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    names.sort(); // oldest first (names are timestamps → chronological)
    for name in backups_to_drop(&names, keep, gfs.as_ref()) {
        if protect == Some(name.as_str()) {
            continue;
        }
        let _ = fs::remove_dir_all(dir.join(name));
    }
}
// A snapshot is stored gzipped: a vacuumed store is mostly JSON text, so this shrinks it to a
// fraction, and ten are kept by default. An uncompressed `data.db` is still accepted so older
// backups keep working.
pub(crate) const BACKUP_DB_GZ: &str = "data.db.gz";
// How often the auto-backup job looks at the clock. The shortest interval the UI
// offers is 6 h, so a quarter of an hour is precise enough and costs nothing.
pub(crate) const BACKUP_CHECK_SECS: u64 = 900;

// No backup on disk at all is always due; otherwise due once the newest is at least a full
// interval old. Pulled out of auto_backup_tick so both cases are unit-testable without an
// AppHandle. The second case also keeps two app starts shortly after each other from both
// taking a backup: the later one sees the first as fresh and skips.
pub(crate) fn backup_is_due(newest_age_secs: Option<u64>, interval_h: u64) -> bool {
    newest_age_secs.map_or(true, |age| age >= interval_h.saturating_mul(3600))
}

// One round of the auto-backup job. Due-ness comes from the age of the newest backup on
// disk, never a counter this process keeps, so a restart neither skips a due backup nor
// takes an extra one.
pub(crate) fn auto_backup_tick(app: &AppHandle) {
    // The lock is held across the point-in-time copy, like the manual "backup now" command, so no
    // write races with the VACUUM read.
    let state = app.state::<Db>();
    let store = lock(&state);
    let on = store.settings.ui_flags.get("autoBackup") != Some(&false);
    let keep = store.settings.ui_values.get("backupKeep").copied().unwrap_or(10.0).clamp(1.0, 50.0) as usize;
    let interval_h = store
        .settings
        .ui_values
        .get("backupIntervalH")
        .copied()
        .filter(|v| *v >= 1.0)
        .unwrap_or(24.0) as u64;
    let due = backup_is_due(newest_backup_age(app), interval_h);
    if on && due {
        // A snapshot copies data.db, so anything still collected has to be in it.
        flush_settings(app, &store.settings);
        // The manual "backup now" toast fires from the frontend's own success path; the automatic
        // one has no such call site, so it needs its own signal here.
        let gfs = gfs_from_settings(&store.settings);
        // Point-in-time copy under the lock, packing after it is released.
        match backup_released(store, &data_dir(app), &backups_dir(app), keep, gfs, || {}) {
            Ok(_) => {
                let _ = app.emit_to("main", "backup-done", ());
            }
            // Failing silently would let automatic protection stay broken (disk full, permissions, a
            // locked folder) with nothing telling the user, so log it for the settings/backup screen.
            Err(e) => {
                append_crash_log(&format!("automatic backup failed: {e}"));
                let _ = app.emit_to("main", "backup-failed", e);
            }
        }
    }
}

// Start-up sweep of what a quit or crash mid-pack leaves behind: the raw copy, the
// half-written archive, and folders that never got a snapshot. Anything newer than `min_age`
// may belong to a running backup and is left alone, and a folder that holds a snapshot is
// never removed (only its stray temp files).
pub(crate) fn cleanup_incomplete_backups(root: &std::path::Path, min_age: Duration) {
    let old_enough = |p: &std::path::Path| {
        fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age >= min_age)
    };
    for entry in fs::read_dir(root).into_iter().flatten().flatten() {
        let folder = entry.path();
        if !folder.is_dir() || !valid_backup_name(&entry.file_name().to_string_lossy()) {
            continue;
        }
        if backup_db_file(&folder).is_some() {
            for stray in ["data.db.tmp", "data.db.gz.part"] {
                let f = folder.join(stray);
                if f.exists() && old_enough(&f) {
                    let _ = fs::remove_file(&f);
                }
            }
            continue;
        }
        let newest_inside_is_old = fs::read_dir(&folder).into_iter().flatten().flatten().all(|e| old_enough(&e.path()));
        if old_enough(&folder) && newest_inside_is_old {
            let _ = fs::remove_dir_all(&folder);
        }
    }
}

pub(crate) fn backup_db_file(folder: &std::path::Path) -> Option<PathBuf> {
    let gz = folder.join(BACKUP_DB_GZ);
    if gz.exists() {
        return Some(gz);
    }
    let plain = folder.join("data.db");
    plain.exists().then_some(plain)
}

pub(crate) fn gzip_to(src: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    use flate2::{write::GzEncoder, Compression};
    // Temp + rename: a snapshot half-written when the machine goes down would still be picked up
    // by backup_db_file and only fail at restore time.
    let tmp = dest.with_extension("gz.part");
    // Streamed file to file: the library is never held in memory, raw or packed.
    let packed = (|| -> std::io::Result<()> {
        let mut input = fs::File::open(src)?;
        let mut enc = GzEncoder::new(fs::File::create(&tmp)?, Compression::default());
        std::io::copy(&mut input, &mut enc)?;
        // finish() writes the gzip trailer; sync so a reported success is on disk.
        enc.finish()?.sync_all()
    })();
    if let Err(e) = packed {
        let _ = fs::remove_file(&tmp);
        return Err(e.to_string());
    }
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e.to_string()
    })
}

// VACUUM src into tmp, then reject the result if it is past the ceiling restore_backup could
// inflate. The path-based core of do_backup's size gate, split out so it is testable without
// an AppHandle. VACUUM INTO folds in every committed row, data.db-wal included, so tmp's size
// is the true figure restore_backup would inflate; checking data.db alone (do_backup's cheap
// early exit) under-reports when committed data sits in an uncheckpointed WAL file.
pub(crate) fn vacuum_and_check_size(src: &std::path::Path, tmp: &std::path::Path) -> Result<(), String> {
    let conn = Connection::open(src).map_err(|e| e.to_string())?;
    conn.execute("VACUUM INTO ?1", params![tmp.to_string_lossy()]).map_err(|e| e.to_string())?;
    if let Ok(meta) = fs::metadata(tmp) {
        if too_large_to_restore(meta.len()) {
            let _ = fs::remove_file(tmp);
            return Err("store-too-large-for-backup".into());
        }
    }
    Ok(())
}

// The one error for a snapshot past MAX_INFLATED, whether a gzip that inflates that far or a
// plain file that already is that big.
const BACKUP_TOO_LARGE_MSG: &str = "backup expands to more than 512 MB";

pub(crate) fn gunzip_to(src: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    gunzip_to_capped(src, dest, MAX_INFLATED)
}

// Streams the snapshot through the decoder straight into dest, never holding it in memory.
// Capped like every decompression here: a snapshot file sits on disk where anything could
// have replaced it, and an unbounded inflate is a free crash. Any failure (truncated or
// corrupt stream, cap exceeded, full disk) removes the partial output, so a half-inflated
// file is never mistaken for a database.
pub(crate) fn gunzip_to_capped(src: &std::path::Path, dest: &std::path::Path, cap: u64) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let input = fs::File::open(src).map_err(|e| e.to_string())?;
    let out = fs::File::create(dest).map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> {
        let mut out = out;
        // cap + 1 byte: reaching the extra byte is what proves the stream was too big.
        let n = std::io::copy(&mut GzDecoder::new(input).take(cap.saturating_add(1)), &mut out)
            .map_err(|_| "corrupt backup".to_string())?;
        if n > cap {
            return Err(BACKUP_TOO_LARGE_MSG.to_string());
        }
        out.sync_all().map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(dest);
    }
    result
}

pub(crate) fn do_backup(app: &AppHandle, keep: usize, gfs: Option<GfsConfig>) -> Result<String, String> {
    do_backup_at(&data_dir(app), &backups_dir(app), keep, gfs)
}

// The restore's safety snapshot: rotation never drops `protect`, the snapshot about to be
// restored.
pub(crate) fn do_backup_protecting(app: &AppHandle, keep: usize, gfs: Option<GfsConfig>, protect: &str) -> Result<String, String> {
    do_backup_protecting_at(&data_dir(app), &backups_dir(app), keep, gfs, protect)
}

pub(crate) fn do_backup_protecting_at(dir: &std::path::Path, root: &std::path::Path, keep: usize, gfs: Option<GfsConfig>, protect: &str) -> Result<String, String> {
    let pending = backup_snapshot(dir, root)?;
    backup_pack_protecting(pending, root, keep, gfs, Some(protect))
}

// Split out of do_backup() for the command line's headless path: the same snapshot, naming
// and rotation logic from two already-resolved folders instead of an AppHandle.
pub(crate) fn do_backup_at(dir: &std::path::Path, root: &std::path::Path, keep: usize, gfs: Option<GfsConfig>) -> Result<String, String> {
    let pending = backup_snapshot(dir, root)?;
    backup_pack(pending, root, keep, gfs)
}

// A claimed snapshot folder holding the vacuumed, still unpacked copy.
pub(crate) struct PendingBackup {
    pub(crate) name: String,
    pub(crate) folder: PathBuf,
    pub(crate) tmp: PathBuf,
}

// Phase one: claim the folder and take the point-in-time copy (VACUUM INTO reads one
// consistent state). Callers hold the store lock across this so no write lands mid-copy;
// everything after (compression, names, rotation) touches only the copy and runs without it.
pub(crate) fn backup_snapshot(dir: &std::path::Path, root: &std::path::Path) -> Result<PendingBackup, String> {
    let src = dir.join("data.db");
    if !src.exists() {
        return Err("no-store".into());
    }
    // restore_backup refuses to inflate a snapshot past MAX_INFLATED (a defense against a hostile
    // crafted .gz). Without this check a store that grew past that ceiling would back up fine and
    // only reveal it can never be restored the day it is needed. Checked on the uncompressed size
    // (gzip only shrinks it). A cheap early exit only: data.db alone can under-report content in
    // an uncheckpointed data.db-wal, so the post-VACUUM check below is what gates the backup.
    if let Ok(meta) = fs::metadata(&src) {
        if too_large_to_restore(meta.len()) {
            return Err("store-too-large-for-backup".into());
        }
    }
    // Two snapshots inside the same second (two quick manual backups, or the hour a DST fallback
    // repeats) must not share a folder: create_dir_all would reuse it and silently overwrite the
    // earlier snapshot. create_dir fails on an existing directory, so claiming the name is the
    // check, with no race. stamp_secs() reads only the first three time fields, so the suffix
    // leaves the snapshot's age intact for rotation and the GFS tiers.
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let base = local_stamp();
    let mut name = base.clone();
    let mut folder = root.join(&name);
    let mut n = 2;
    loop {
        match fs::create_dir(&folder) {
            Ok(()) => break,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if n > 99 {
                    return Err("backup-name-taken".into());
                }
                name = format!("{base}-{n}");
                folder = root.join(&name);
                n += 1;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    let tmp = folder.join("data.db.tmp");
    let _ = fs::remove_file(&tmp);
    // As with the packed-copy failure below: a folder was already claimed, so any failure before
    // a real snapshot exists must remove it, or the list offers a restore from one never written.
    if let Err(e) = vacuum_and_check_size(&src, &tmp) {
        let _ = fs::remove_dir_all(&folder);
        return Err(e);
    }
    Ok(PendingBackup { name, folder, tmp })
}

// Phase two: compress the copy, record count and names, rotate. Works on files only, so it
// needs no store lock. On any failure the claimed folder is removed.
pub(crate) fn backup_pack(pending: PendingBackup, root: &std::path::Path, keep: usize, gfs: Option<GfsConfig>) -> Result<String, String> {
    backup_pack_protecting(pending, root, keep, gfs, None)
}

pub(crate) fn backup_pack_protecting(pending: PendingBackup, root: &std::path::Path, keep: usize, gfs: Option<GfsConfig>, protect: Option<&str>) -> Result<String, String> {
    let PendingBackup { name, folder, tmp } = pending;
    let dest = folder.join(BACKUP_DB_GZ);
    // Compress the vacuumed copy, then drop the raw one: never leave both behind.
    let packed = gzip_to(&tmp, &dest);
    // The count and names are taken from the vacuumed copy while it is unpacked, so the list
    // never has to decompress a snapshot for either.
    let mut counted = None;
    if let Ok(conn) = Connection::open(&tmp) {
        if let Ok(n) = conn.query_row("SELECT COUNT(*) FROM prompts", [], |r| r.get::<_, i64>(0)) {
            let _ = fs::write(folder.join(BACKUP_COUNT_FILE), n.max(0).to_string());
            counted = Some(n);
        }
    }
    let items = read_prompt_names(&tmp);
    // An empty list is cached only for a snapshot that really holds no prompt: a failed read
    // looks the same and must not be pinned.
    if !items.is_empty() || counted == Some(0) {
        write_prompt_names(&folder.join(BACKUP_NAMES_FILE), &items);
    }
    // The app version that made this snapshot, so the content viewer can show it without
    // guessing from the running build. Plain text like the count file: a version is not sensitive.
    let _ = fs::write(folder.join(BACKUP_VERSION_FILE), APP_VERSION);
    let _ = fs::remove_file(&tmp);
    if let Err(e) = packed {
        // No snapshot is better than a folder that looks like one: the list reads folders, and an
        // empty one would offer a restore that cannot work.
        let _ = fs::remove_dir_all(&folder);
        return Err(e);
    }
    rotate_backups_at(root, keep.clamp(1, 50), gfs, protect);
    Ok(name)
}

// Snapshot under the caller's store lock, then release it before the slow part. `between`
// runs right after the release (tests use it to prove the lock is free).
pub(crate) fn backup_released<G>(
    guard: G,
    dir: &std::path::Path,
    root: &std::path::Path,
    keep: usize,
    gfs: Option<GfsConfig>,
    between: impl FnOnce(),
) -> Result<String, String> {
    let pending = backup_snapshot(dir, root);
    drop(guard);
    let pending = pending?;
    between();
    backup_pack(pending, root, keep, gfs)
}
// A folder name "YYYY-MM-DD_HH-MM-SS" as seconds on a local civil timeline. Two of these
// subtract cleanly (the zone offset cancels), which is all the schedule needs, and it avoids
// a modification time that any sync or antivirus tool may reset.
pub(crate) fn stamp_secs(name: &str) -> Option<i64> {
    let (date, time) = name.split_once('_')?;
    let mut d = date.split('-');
    let y: i64 = d.next()?.parse().ok()?;
    let mo: i64 = d.next()?.parse().ok()?;
    let da: i64 = d.next()?.parse().ok()?;
    let mut t = time.split('-');
    let h: i64 = t.next()?.parse().ok()?;
    let mi: i64 = t.next()?.parse().ok()?;
    let s: i64 = t.next().unwrap_or("0").parse().ok()?;
    Some(days_from_civil(y, mo, da) * 86_400 + h * 3_600 + mi * 60 + s)
}

// Seconds since the newest snapshot, or None when there is none yet.
pub(crate) fn newest_backup_age(app: &AppHandle) -> Option<u64> {
    let now = stamp_secs(&local_stamp())?;
    let newest = fs::read_dir(backups_dir(app))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir() && backup_db_file(&e.path()).is_some())
        .filter_map(|e| stamp_secs(&e.file_name().to_string_lossy()))
        .max()?;
    Some((now - newest).max(0) as u64)
}

#[tauri::command]
pub(crate) async fn list_backups(app: AppHandle) -> Result<Vec<BackupInfo>, String> {
    Ok(list_backups_impl(&app))
}

// Hands the user to the snapshot folder. The path is always the app's own backups_dir(),
// never taken from the frontend, so a crafted argument cannot redirect it; `explorer` gets
// exactly one argument, no shell involved.
#[tauri::command]
pub(crate) fn open_backups_folder(app: AppHandle) -> Result<(), String> {
    let dir = backups_dir(&app);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn create_backup(app: AppHandle, keep: usize) -> Result<String, String> {
    // Off the async runtime: the lock wait, the VACUUM and the compression all block. The store
    // lock is held only across the point-in-time copy.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Db>();
        let store = lock(&state); // no writes race with the VACUUM read
        let gfs = gfs_from_settings(&store.settings);
        // The snapshot copies data.db: a collected counter write belongs in it.
        flush_settings(&app, &store.settings);
        backup_released(store, &data_dir(&app), &backups_dir(&app), keep, gfs, || {})
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn delete_backup(app: AppHandle, name: String) -> Result<(), String> {
    if !valid_backup_name(&name) {
        return Err("bad-name".into());
    }
    fs::remove_dir_all(backups_dir(&app).join(&name)).map_err(|e| e.to_string())
}

pub(crate) const STAGED_RESTORE_PREFIX: &str = "data.db.restoring";

// Start-up sweep of staged backup copies a quit or crash left beside the live store (the
// copy, its -wal/-shm and the older fixed name). Anything younger than `min_age` is left.
pub(crate) fn remove_stale_staged_restores(dir: &std::path::Path, min_age: Duration) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        if !entry.file_name().to_string_lossy().starts_with(STAGED_RESTORE_PREFIX) {
            continue;
        }
        let old_enough = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age >= min_age);
        if old_enough && entry.path().is_file() {
            let _ = fs::remove_file(entry.path());
        }
    }
}

// Unpack (gunzip if needed) a backup file beside the live store, then prove it is an openable
// SQLite file. Shared by every restore scope: a snapshot failing either step must never
// touch the live database.
pub(crate) fn stage_and_validate_backup(backup_db: &std::path::Path, dir: &std::path::Path) -> Result<PathBuf, String> {
    // A name of its own per call: two "view contents" clicks, or a view still unpacking while a
    // restore runs, must not share (and overwrite) one file.
    let staged = dir.join(format!("{STAGED_RESTORE_PREFIX}-{}", gen_id()));
    let _ = fs::remove_file(&staged);
    let unpack = if backup_db.extension().is_some_and(|e| e == "gz") {
        gunzip_to(backup_db, &staged)
    } else {
        // Pre-2.6 snapshot: copied as is, so it gets the same ceiling an inflated one does, checked
        // before a byte is copied.
        match fs::metadata(backup_db) {
            Ok(meta) if meta.len() > MAX_INFLATED => Err(BACKUP_TOO_LARGE_MSG.to_string()),
            Ok(_) => fs::copy(backup_db, &staged).map(|_| ()).map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        }
    };
    if let Err(e) = unpack {
        let _ = fs::remove_file(&staged);
        return Err(e);
    }
    // fs::copy keeps the source's modified time, which would make a fresh copy look old to
    // another instance's stale-file sweep: stamp it with now.
    if let Ok(f) = fs::OpenOptions::new().write(true).open(&staged) {
        let _ = f.set_modified(SystemTime::now());
    }
    // A readable SQLite file is not necessarily one of this app's backups: any valid but
    // unrelated .db (empty, wrong schema, a stray file the user picked) would pass a bare "can I
    // query it" check and, for scope "all", be renamed onto the live data.db, where
    // db_conn_init_schema's CREATE TABLE IF NOT EXISTS would recreate the missing tables empty,
    // a restore that "succeeds" into an empty library. So the two tables every real backup has
    // (prompts, meta) are required with the exact columns every read and write assumes (a table
    // that is merely called "prompts" but has another shape would fail confusingly mid-restore),
    // plus a quick SQLite integrity pass, before the live database is touched.
    let has_expected_schema = Connection::open(&staged).is_ok_and(|c| {
        let has_columns = |table: &str, cols: &[&str]| {
            cols.iter().all(|col| {
                c.query_row(
                    "SELECT count(*) FROM pragma_table_info(?1) WHERE name = ?2",
                    params![table, col],
                    |r| r.get::<_, i64>(0),
                ) == Ok(1)
            })
        };
        has_columns("prompts", &["id", "ord", "data"])
            && has_columns("meta", &["key", "value"])
            && c.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0)).as_deref() == Ok("ok")
    });
    if !has_expected_schema {
        let _ = fs::remove_file(&staged);
        return Err("backup-schema-mismatch".to_string());
    }
    Ok(staged)
}

// The column names of one table, in the live database ("main") or in the attached backup.
// Empty means the table is not there.
pub(crate) fn table_columns(tx: &Transaction, schema: &str, table: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = tx.prepare("SELECT name FROM pragma_table_info(?1, ?2)")?;
    let names = stmt.query_map(params![table, schema], |r| r.get::<_, String>(0))?;
    names.collect()
}

// The three scopes a restore or an import may run in. Both commands parse the string through
// here so an unrecognised value cannot mean "everything" to one and "nothing, but report
// success" to the other.
pub(crate) fn valid_scope(scope: &str) -> Result<&str, String> {
    match scope {
        "all" | "data" | "settings" => Ok(scope),
        _ => Err(format!("unknown scope {scope:?}, expected all, data or settings")),
    }
}

// The live settings row, or None when absent or unreadable (an unreadable row has no vault
// keys to carry).
pub(crate) fn read_settings_row(conn: &Connection, schema: &str) -> Option<Settings> {
    let schema = if schema == "backup" { "backup" } else { "main" };
    let raw: String = conn.query_row(&format!("SELECT value FROM {schema}.meta WHERE key = 'settings'"), [], |r| r.get(0)).ok()?;
    serde_json::from_str(&dec_str(&raw)).ok()
}

// Before a whole snapshot replaces the database: the settings row in the staged copy gets
// this machine's network, credential and registration values back, so an old snapshot cannot
// bring back an old receiver password or switch. The vault keys stay with the snapshot's
// prompts.
pub(crate) fn keep_machine_in_staged(target: &std::path::Path, staged: &std::path::Path) -> Result<(), String> {
    let local = match Connection::open(target) {
        Ok(conn) => read_settings_row(&conn, "main"),
        Err(_) => None,
    };
    // No settings on this machine yet: the snapshot still must not bring the denied switches or
    // its autostart registration along, so it gets a fresh install's values.
    let local = local.unwrap_or_default();
    let conn = Connection::open(staged).map_err(|e| e.to_string())?;
    if let Some(mut restored) = read_settings_row(&conn, "main") {
        keep_machine_settings(&local, &mut restored);
        db_write_settings(&conn, &restored).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// Merge only the tables `scope` asks for from `staged` into the live `target` database,
// inside one transaction: any failure leaves `target` untouched. "data" mirrors the tables
// delete_all_data treats as data; "settings" touches only the meta['settings'] row, never
// the meta['copy_log_migrated'] marker. A table missing from an older backup (e.g. "trash")
// is skipped rather than failing the restore: its live rows stay put.
pub(crate) fn restore_selective(target: &std::path::Path, staged: &std::path::Path, scope: &str) -> Result<(), String> {
    let mut conn = Connection::open(target).map_err(|e| e.to_string())?;
    conn.execute("ATTACH DATABASE ?1 AS backup", [staged.to_string_lossy().to_string()])
        .map_err(|e| e.to_string())?;
    let result: rusqlite::Result<()> = (|| {
        let tx = conn.transaction()?;
        if scope == "data" {
            for table in ["prompts", "trash", "prompt_versions", "copy_log", "clip_inbox"] {
                // A snapshot's table need not have today's columns: one written before a column was added
                // (prompt_versions and `variants`) has one fewer. `SELECT *` makes the column count part of
                // the deal and would fail the restore outright, taking every other table in the transaction
                // down. Copy the columns both sides share and let the rest take their defaults. The list is
                // built from the live table's own column names, so nothing read out of the backup file ends
                // up in the SQL text.
                let backup_cols = table_columns(&tx, "backup", table)?;
                let shared: Vec<String> = table_columns(&tx, "main", table)?
                    .into_iter()
                    .filter(|c| backup_cols.iter().any(|b| b.eq_ignore_ascii_case(c)))
                    .collect();
                // Nothing in common, including a table missing from an older backup entirely (e.g. "trash"):
                // leave the live rows alone rather than emptying them.
                if shared.is_empty() {
                    continue;
                }
                let cols = shared.join(", ");
                tx.execute(&format!("DELETE FROM {table}"), [])?;
                tx.execute(
                    &format!("INSERT INTO {table}({cols}) SELECT {cols} FROM backup.{table}"),
                    [],
                )?;
            }
            // The vault keys follow the prompts: confidential prompts are sealed with the key derived
            // from the salt of the settings they came with.
            if let (Some(mut local), Some(theirs)) = (read_settings_row(&tx, "main"), read_settings_row(&tx, "backup")) {
                if has_vault_keys(&theirs) {
                    carry_vault_keys(&theirs, &mut local);
                    db_write_settings(&tx, &local)?;
                }
            }
        } else if scope == "settings" {
            // The prompts stay, so the vault keys they are sealed with stay too, and so do this machine's
            // network, credential and registration values.
            let local = read_settings_row(&tx, "main");
            tx.execute(
                "INSERT INTO meta(key, value) SELECT key, value FROM backup.meta WHERE key = 'settings'
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [],
            )?;
            if let Some(mut restored) = read_settings_row(&tx, "main") {
                match local {
                    Some(local) => {
                        keep_machine_settings(&local, &mut restored);
                        clear_vault_keys(&mut restored);
                        carry_vault_keys(&local, &mut restored);
                    }
                    // No settings on this machine yet: the denied rows, the autostart registration and the vault
                    // keys get a fresh install's values.
                    None => {
                        keep_machine_settings(&Settings::default(), &mut restored);
                        clear_vault_keys(&mut restored);
                    }
                }
                db_write_settings(&tx, &restored)?;
            }
        }
        tx.commit()
    })();
    let _ = conn.execute("DETACH DATABASE backup", []);
    if result.is_ok() {
        // Another connection to data.db may still be open when this one closes; its committed pages
        // must reach the main file, because the caller never touches the WAL of a selective restore.
        let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    }
    result.map_err(|e| e.to_string())
}

// Pure core of backup_contents: everything a snapshot holds, read from an open connection on
// the staged file. Split out so it is testable against a plain fixture database, without an
// AppHandle or a real .gz (as apply_import from import_all, and vacuum_and_check_size from
// do_backup).
pub(crate) fn backup_contents_from(conn: &Connection, folder: &std::path::Path) -> BackupContents {
    let mut written = WrittenRows::new();
    let (prompts, settings, plaintext, _failures, _settings_unreadable) = db_load(conn, &mut written);
    // Each prompt's most recent edit, from the table the library's "last edited" sort reads,
    // grouped per prompt rather than a single MAX across everything.
    let mut dates: HashMap<String, i64> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT prompt_id, MAX(ts) FROM prompt_versions GROUP BY prompt_id") {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))) {
            for row in rows.flatten() {
                dates.insert(row.0, row.1);
            }
        }
    }
    let last_change = conn
        .query_row("SELECT MAX(ts) FROM prompt_versions", [], |r| r.get::<_, Option<i64>>(0))
        .unwrap_or(None);
    let mut tag_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    for p in &prompts {
        for tag in &p.tags {
            tag_keys.insert(tag_key(tag));
        }
    }
    let items = prompts
        .iter()
        .map(|p| BackupContentItem {
            id: p.id.clone(),
            name: p.name.clone(),
            date: dates.get(&p.id).copied(),
            chars: prompt_char_count(p),
            kind: prompt_stat_type(p).to_string(),
        })
        .collect();
    BackupContents {
        entries: prompts.len(),
        views: settings.as_ref().map(|s| s.views.len()).unwrap_or(0),
        tags: tag_keys.len(),
        last_change,
        program_version: fs::read_to_string(folder.join(BACKUP_VERSION_FILE))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        encrypted: !plaintext,
        items,
    }
}

// Everything a snapshot holds, read through the same staged and schema-checked file
// restore_backup/restore_selective open, never a second path onto the raw .gz. Nothing here
// writes: the staged copy is removed once the query is done, success or not.
#[tauri::command]
pub(crate) async fn backup_contents(app: AppHandle, name: String) -> Result<BackupContents, String> {
    if !valid_backup_name(&name) {
        return Err("bad-name".into());
    }
    let folder = backups_dir(&app).join(&name);
    let Some(backup_db) = backup_db_file(&folder) else {
        return Err("missing".into());
    };
    let dir = data_dir(&app);
    let staged = stage_and_validate_backup(&backup_db, &dir)?;
    let result = Connection::open(&staged)
        .map(|conn| backup_contents_from(&conn, &folder))
        .map_err(|e| e.to_string());
    let _ = fs::remove_file(&staged);
    result
}

// Restore: snapshot the current store first (safety), then either swap the whole data.db
// (scope "all") or merge just the requested tables in place (scope "data"/"settings"). Either
// way the app relaunches after, so nothing can half-apply.
#[tauri::command]
pub(crate) async fn restore_backup(app: AppHandle, state: State<'_, Db>, name: String, keep: usize, scope: String) -> Result<(), String> {
    restore_backup_core(app.clone(), state, name, keep, scope)?;
    app.restart();
}

// Everything a restore does except the relaunch. The command line and the receiver call this,
// answer their caller, and only then restart: a restart inside the call drops the connection
// before any answer is written, and a script would read a working restore as a failure.
pub(crate) fn restore_backup_core(app: AppHandle, state: State<'_, Db>, name: String, keep: usize, scope: String) -> Result<(), String> {
    if !valid_backup_name(&name) {
        return Err("bad-name".into());
    }
    valid_scope(&scope)?;
    // The relaunch that follows a restore would cut a running take short.
    if update_blocker(recording_active(&app), REC_STARTING.load(Ordering::SeqCst), 0).is_some() {
        return Err("restore-recording-active".into());
    }
    // A store another machine holds locked is not ours to overwrite.
    if data_dir_write_blocked(&app) {
        return Err("readonly".into());
    }
    let Some(backup_db) = backup_db_file(&backups_dir(&app).join(&name)) else {
        return Err("missing".into());
    };
    {
        let store = lock(&state);
        let ocr_was_on = ocr_index_flag_on(&store);
        let gfs = gfs_from_settings(&store.settings);
        // The safety snapshot is the fallback if the restore goes wrong, so it must carry the
        // collected counters too.
        flush_settings(&app, &store.settings);
        // A restore without its own undo point is one-way: if the snapshot could not be written (disk
        // full, permissions, locked file), stop rather than overwrite the live database with nothing
        // to fall back to.
        do_backup_protecting(&app, keep, gfs, &name).map_err(|e| format!("safety-snapshot-failed: {e}"))?;
        let dir = data_dir(&app);
        let target = dir.join("data.db");
        let staged = stage_and_validate_backup(&backup_db, &dir)?;
        if scope == "all" {
            // Writing straight onto data.db truncates it first, so a disk that fills up halfway would
            // leave neither the old nor the restored database: swap via rename instead.
            if let Err(e) = keep_machine_in_staged(&target, &staged) {
                let _ = fs::remove_file(&staged);
                return Err(e);
            }
            if let Err(e) = fs::rename(&staged, &target) {
                let _ = fs::remove_file(&staged);
                return Err(e.to_string());
            }
            // The renamed-in file must not pair with the old database's journal.
            let _ = fs::remove_file(dir.join("data.db-wal"));
            let _ = fs::remove_file(dir.join("data.db-shm"));
        } else {
            let result = restore_selective(&target, &staged, &scope);
            let _ = fs::remove_file(&staged);
            result?;
        }
        // Every prompt row on disk now comes from the backup. The relaunch rebuilds the store anyway,
        // but a write slipping in before it must not trust any of the old hashes.
        store.forget_written_rows();
        // A restore that brings settings with it may have switched the picture-text
        // index off while the prompts that stay still carry its text.
        if ocr_was_on && scope != "data" {
            let mut fresh = load_store(&app);
            if clear_ocr_text_if_switched_off(true, &mut fresh) {
                write_ocr_wipe(&app, &fresh, "restore");
            }
        }
        // The store still holds the pre-restore prompts and settings until the relaunch every caller
        // does right after Ok. Writing them back (the exit flush, a hotkey counter, any prompt save)
        // would undo the restore, so every write is refused from here on, with the switch the
        // demo-mode flip uses. Set before the store lock is released.
        DEMO_TRANSITION_LOCKED.store(true, Ordering::Relaxed);
    }
    Ok(())
}
