use super::*;

// Paste the current clipboard into the last external window. The delay lets the
// target window finish taking focus before Ctrl+V; kept off the UI thread via tokio.
#[tauri::command]
pub(crate) async fn paste_into_previous(delay_ms: u64, enter: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let hwnd = autopaste::begin_paste()?;
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms.clamp(10, 1000))).await;
        autopaste::finish_paste(hwnd, enter)
    }
    #[cfg(not(windows))]
    {
        let _ = (delay_ms, enter);
        Err("unsupported".into())
    }
}

// Tunable knobs for the auto-advance clipboard session (the expert menu exposes these in the
// expert menu; defaults here are what ships until a user overrides them there).
#[cfg(windows)]
#[derive(Clone, Copy)]
pub(crate) struct AutoCfg {
    pub(crate) settle_ms: u32,
    pub(crate) cooldown_ms: u32,
    pub(crate) input_ms: u32,
    pub(crate) rearm_max: u32,
    pub(crate) timeout_min: u32,
}

// Fixed timings of the automatic cycling: how long the clipboard must be quiet
// before a paste counts, the window for typed input, and how often a session
// may re-arm itself.
#[cfg(windows)]
pub(crate) const ROTATE_AUTO_SETTLE_MS: u32 = 350;
// Pause after a counted paste before the next one counts, and how long a
// session lasts before it stops cycling (0 would mean no limit).
#[cfg(windows)]
pub(crate) const ROTATE_AUTO_COOLDOWN_MS: u32 = 250;
#[cfg(windows)]
pub(crate) const ROTATE_AUTO_TIMEOUT_MIN: u32 = 60;
#[cfg(windows)]
pub(crate) const ROTATE_AUTO_INPUT_MS: u32 = 3000;
#[cfg(windows)]
pub(crate) const ROTATE_AUTO_REARM_MAX: u32 = 12;

#[cfg(windows)]
pub(crate) fn auto_cfg() -> AutoCfg {
    AutoCfg {
        settle_ms: ROTATE_AUTO_SETTLE_MS,
        cooldown_ms: ROTATE_AUTO_COOLDOWN_MS,
        input_ms: ROTATE_AUTO_INPUT_MS,
        rearm_max: ROTATE_AUTO_REARM_MAX,
        timeout_min: ROTATE_AUTO_TIMEOUT_MIN,
    }
}

// "Did this WM_RENDERFORMAT answer a real Ctrl+V / right-click-Paste, or just a
// clipboard-history spy / our own read?" Pure and table-tested (cargo test --lib):
// the Win32 plumbing around it is only provable on a real VM run.
#[cfg(windows)]
pub(crate) fn counts_as_paste(now: u32, armed_at: u32, last_paste: u32, last_input: u32, self_read: bool, cfg: &AutoCfg) -> bool {
    if self_read {
        return false;
    }
    if now.wrapping_sub(armed_at) < cfg.settle_ms {
        return false;
    }
    if last_paste != 0 && now.wrapping_sub(last_paste) < cfg.cooldown_ms {
        return false;
    }
    if now.wrapping_sub(last_input) > cfg.input_ms {
        return false;
    }
    true
}

// A DWORD-valued clipboard opt-out format (CanIncludeInClipboardHistory,
// CanUploadToCloudClipboard) means "present and explicitly 0" = excluded, a nonzero
// value means the source allows it, and absence means the source never said either
// way. A value that could not be read at all (format announced but the memory block
// came back empty) is treated the same as an explicit 0 -- the same fail-closed
// choice as the secret heuristic below: a copy skipped by mistake costs nothing, a
// secret kept by mistake costs a lot. Pure so the value-reading rule has its own
// test without touching a real clipboard.
pub(crate) fn dword_format_excludes(available: bool, value: Option<u32>) -> bool {
    available && value.unwrap_or(0) == 0
}

// Nothing else this app's own UI ever copies is a single unspaced 8-64 character
// string mixing three of the four character classes below, so the "not a word from
// our own dictionary" carve-out only has to rule out the product name itself.
pub(crate) fn is_own_ui_word(candidate: &str) -> bool {
    candidate.trim().eq_ignore_ascii_case(brand::DISPLAY_NAME)
}

// Shortest copied text the clipboard collector may take for a password.
pub(crate) const CLIP_SECRET_MIN_LEN: usize = 8;

// A best-effort guess that freshly copied text is a password rather than a prompt, a
// sentence or an address: exactly one line, `min_len` to 64 characters, no
// whitespace, at least three of the four character classes, and not the one word the
// app's own UI could otherwise trip this on. Getting this wrong toward "skip it" only
// costs one entry that never reaches the inbox; getting it wrong the other way puts a
// password into a history file, so every ambiguous case below returns true.
pub(crate) fn looks_like_password(text: &str, min_len: usize) -> bool {
    if text.lines().count() != 1 {
        return false;
    }
    let t = text.trim();
    let len = t.chars().count();
    if len < min_len.max(1) || len > 64 {
        return false;
    }
    if t.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    let has_lower = t.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = t.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = t.chars().any(|c| c.is_ascii_digit());
    let has_symbol = t.chars().any(|c| !c.is_ascii_alphanumeric());
    let classes = [has_lower, has_upper, has_digit, has_symbol].iter().filter(|b| **b).count();
    if classes < 3 {
        return false;
    }
    if is_own_ui_word(t) {
        return false;
    }
    true
}

// No-op own-copy note on non-Windows.
#[cfg(windows)]
pub(crate) fn note_own_copy(text: &str) {
    clipwatch::note_own(text);
}
#[cfg(not(windows))]
pub(crate) fn note_own_copy(_text: &str) {}

// List / dismiss / clear the clipboard inbox.
#[tauri::command]
pub(crate) async fn clip_inbox_list(app: AppHandle) -> Result<Vec<ClipItem>, String> {
    Ok(db_conn(&app).map(|c| db_list_clips(&c)).unwrap_or_default())
}
#[tauri::command]
pub(crate) async fn clip_inbox_dismiss(app: AppHandle, id: i64) -> Result<(), String> {
    // Privacy-sensitive rows: a swallowed failure here used to always answer
    // Ok(()) while the entry stayed on disk.
    let ok = db_conn(&app).is_some_and(|c| c.execute("DELETE FROM clip_inbox WHERE id = ?1", params![id]).is_ok());
    if !ok {
        append_crash_log(&format!("clip_inbox_dismiss: row {id} could not be removed"));
        return Err("delete-incomplete".into());
    }
    Ok(())
}
#[tauri::command]
pub(crate) async fn clip_inbox_clear(app: AppHandle) -> Result<(), String> {
    let ok = db_conn(&app).is_some_and(|c| c.execute("DELETE FROM clip_inbox", []).is_ok());
    if !ok {
        append_crash_log("clip_inbox_clear: clip_inbox could not be cleared");
        return Err("delete-incomplete".into());
    }
    Ok(())
}

#[derive(Deserialize)]
pub(crate) struct ClipRestoreItem {
    pub(crate) ts: u64,
    pub(crate) text: String,
}

// Undo for "clear clipboard inbox": re-inserts the rows the frontend snapshotted
// (through the existing clip_inbox_list) before calling clip_inbox_clear, oldest
// first, through db_add_clip -- the same capped insert a live watcher hit uses,
// so a restore right after the cap changed still ends up with a valid inbox
// instead of one longer than the setting now allows.
#[tauri::command]
pub(crate) async fn restore_clip_inbox(app: AppHandle, state: State<'_, Db>, items: Vec<ClipRestoreItem>) -> Result<(), String> {
    let cap = lock(&state).settings.ui_values.get("clipInboxMax").copied().unwrap_or(50.0).clamp(10.0, 200.0) as usize;
    let Some(conn) = db_conn(&app) else {
        return Err("history-failed".into());
    };
    if db_restore_clips(&conn, &items, cap).is_err() {
        append_crash_log("restore_clip_inbox: the items could not be reinserted");
        return Err("history-failed".into());
    }
    Ok(())
}

// The insert loop behind restore_clip_inbox in one transaction: an item that
// fails leaves the inbox as it was instead of a partial restore.
pub(crate) fn db_restore_clips(conn: &Connection, items: &[ClipRestoreItem], cap: usize) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    for item in items {
        db_add_clip(&tx, item.ts, &item.text, cap)?;
    }
    tx.commit()
}
