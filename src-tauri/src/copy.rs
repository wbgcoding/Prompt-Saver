use super::*;

// The single place that decides what text a copy puts on the clipboard. The
// private notes are deliberately not part of it: every copy path goes through
// here (or through copy_text with an already-resolved body), so a note can never
// leak into the clipboard.
pub(crate) fn copy_payload(p: &Prompt) -> &str {
    &p.text
}

// Absolute ceiling regardless of the expert value rotateMaxEntries (a UI-level
// soft cap on how many the editor lets you ADD): this is the hard backend floor
// against a corrupted or hand-edited settings/import file.
pub(crate) const MAX_ROTATE_VARIANTS: usize = 49; // + entry 0 (text) = 50 total, matching rotateMaxEntries' own max

// Trims, drops empty entries and enforces the hard ceiling; add_prompt and update_prompt
// both apply it.
pub(crate) fn sanitize_variants(list: Vec<String>) -> Vec<String> {
    list.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .take(MAX_ROTATE_VARIANTS)
        .collect()
}

// Only "random" is ever stored as anything other than "" (order); a prompt
// with no variants has nothing to rotate, so its mode is meaningless: keeping it
// empty avoids a stale mode surviving in storage past the point it can affect anything.
pub(crate) fn sanitize_rotate_mode(mode: String, variants: &[String]) -> String {
    if variants.is_empty() {
        String::new()
    } else if mode == "random" {
        "random".to_string()
    } else {
        String::new()
    }
}

// Same reasoning as sanitize_rotate_mode: without variants, auto-advance
// has nothing to advance through, so it can never survive as a stored true.
pub(crate) fn sanitize_rotate_auto(auto: bool, variants: &[String]) -> bool {
    auto && !variants.is_empty()
}

// Just the fields the rotation and reveal code reads (id, entries, mode, confidential flag).
// A tile with a picture carries up to a 1024 px data URL; the per-copy paths clone this
// instead of the whole prompt.
pub(crate) fn rotation_core(p: &Prompt) -> Prompt {
    Prompt {
        id: p.id.clone(),
        text: p.text.clone(),
        variants: p.variants.clone(),
        rotate_mode: p.rotate_mode.clone(),
        secret: p.secret,
        ..Prompt::default()
    }
}

// "text" is entry 0 (every non-rotating consumer stays correct unchanged),
// `variants` hold entries 1..n.
pub(crate) fn rotation_total(p: &Prompt) -> usize {
    1 + p.variants.len()
}

pub(crate) fn rotation_entry_text(p: &Prompt, idx: usize) -> String {
    if idx == 0 {
        copy_payload(p).to_string() // Keep the one place that defines "entry 0"
    } else {
        p.variants.get(idx - 1).cloned().unwrap_or_else(|| copy_payload(p).to_string())
    }
}

// Which entry the next copy would use, without touching state, so a tile's "next: n/total"
// badge and the actual copy always agree. With the global "rotatingPrompts" flag off, or no
// variants, it resolves to entry 0 (the list stays saved, just not served).
// A stored pointer/bag that no longer fits the entry count (edited since) is never
// trusted: `% total` keeps a stale `rotate_pos` in range, and a bag with an out-of-range
// index counts as absent (discarded on the next rotation_advance).
// Besides `idx` (the absolute entry position) it returns `pos`: a 1-based draw number
// within the current cycle, so a badge counts 1..total and restarts instead of jumping
// around in random mode. Order mode: `idx + 1`. Random mode: `total - bag.len() + 1`, since
// a fresh bag holds exactly `total` entries.
pub(crate) fn rotation_next(p: &Prompt, settings: &Settings) -> (usize, usize, String) {
    let total = rotation_total(p);
    let (idx, pos) = if p.variants.is_empty() || settings.ui_flags.get("rotatingPrompts") == Some(&false) {
        (0, 1)
    } else if p.rotate_mode == "random" {
        match settings.rotate_bag.get(&p.id) {
            Some(bag) if !bag.is_empty() && bag.iter().all(|&i| (i as usize) < total) => {
                (bag[0] as usize, total - bag.len() + 1)
            }
            _ => (0, 1),
        }
    } else {
        let idx = settings.rotate_pos.get(&p.id).copied().unwrap_or(0) as usize % total;
        (idx, idx + 1)
    };
    (idx, pos, rotation_entry_text(p, idx))
}

// Advances the pointer past the entry rotation_next() just reported; call it only once the
// copy actually reached the clipboard, so a failed copy never burns an entry. Random mode
// pops the bag's front and, when that empties it (or the bag was stale or absent),
// reshuffles every other index with the just-shown one placed last, so two showings of the
// same entry are always a full cycle apart, also across the reshuffle.
pub(crate) fn rotation_advance(p: &Prompt, settings: &mut Settings) {
    // Same gate as rotation_next(): while the feature is globally off the pointer stays
    // frozen, so turning it back on resumes where the user left off.
    if p.variants.is_empty() || settings.ui_flags.get("rotatingPrompts") == Some(&false) {
        return;
    }
    let total = rotation_total(p);
    if p.rotate_mode == "random" {
        let bag_valid = settings.rotate_bag.get(&p.id)
            .map(|b| !b.is_empty() && b.iter().all(|&i| (i as usize) < total))
            .unwrap_or(false);
        let (shown, mut rest) = if bag_valid {
            let mut bag = settings.rotate_bag.remove(&p.id).unwrap();
            (bag.remove(0), bag)
        } else {
            (0u32, Vec::new())
        };
        if rest.is_empty() {
            rest = (0..total as u32).filter(|&i| i != shown).collect();
            use rand::seq::SliceRandom;
            rest.shuffle(&mut rand::thread_rng());
            rest.push(shown); // never the first draw of the new cycle
        }
        settings.rotate_bag.insert(p.id.clone(), rest);
    } else {
        let pos = settings.rotate_pos.get(&p.id).copied().unwrap_or(0) as usize % total;
        settings.rotate_pos.insert(p.id.clone(), ((pos + 1) % total) as u32);
    }
}

// Drops both rotation entries once a prompt is gone for good: called from the
// same places that already purge per-prompt state (`purge_ids`), not from the
// soft "move to trash" step (a trashed-then-restored prompt keeps its position).
pub(crate) fn rotation_forget(settings: &mut Settings, ids: &[String]) {
    for id in ids {
        settings.rotate_pos.remove(id);
        settings.rotate_bag.remove(id);
    }
}

// The single place that decides whether a text copy goes to the clipboard
// normally or opens an auto-advance session (Windows only: delayed rendering is a
// Win32 mechanism, see mod clipwatch). `id` is None for a text copy that never
// carries rotation state (e.g. a resolved placeholder fill copied ad hoc).
#[cfg(windows)]
pub(crate) fn put_text(state: &State<Db>, id: Option<&str>, text: &str) -> bool {
    let mut secret = false;
    if let Some(id) = id {
        let auto;
        (auto, secret) = {
            let store = lock(state);
            store.prompts.iter().find(|p| p.id == id)
                .map(|p| (auto_eligible(p, &store.settings), p.secret))
                .unwrap_or((false, false))
        };
        if auto {
            clipwatch::arm(id, text, Vec::new(), secret);
            return true;
        }
    }
    note_own_copy(text);
    // A confidential prompt's plain text stays out of clipboard history and the cloud clipboard.
    if !clipwatch::private_formats(secret).is_empty() {
        return clipwatch::write_private_text(text);
    }
    arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string())).is_ok()
}
#[cfg(not(windows))]
pub(crate) fn put_text(_state: &State<Db>, _id: Option<&str>, text: &str) -> bool {
    note_own_copy(text);
    arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string())).is_ok()
}

// The most-called command (every tile/pill/hotkey copy), so it clones only the one field
// the chosen branch needs, not the whole Prompt (an image prompt's base64 blob included).
pub(crate) enum CopyPayload {
    Image(String),
    File(String),
    Text(String),
    Missing,
}

// Async: encoding + writing a large image to the clipboard stays off the UI thread.
#[tauri::command]
pub(crate) async fn copy_prompt(app: AppHandle, state: State<'_, Db>, id: String) -> Result<bool, String> {
    let payload = {
        let store = lock(&state);
        match store.prompts.iter().find(|p| p.id == id) {
            Some(p) if p.copy_image && !p.image.is_empty() => CopyPayload::Image(p.image.clone()),
            // Asked again at use: the stored path may predate the check on save.
            Some(p) if !p.file_path.is_empty() => {
                if attachment_path_refused(&data_dir(&app), &p.file_path) {
                    CopyPayload::Missing
                } else {
                    CopyPayload::File(p.file_path.clone())
                }
            }
            // rotation_next() itself falls back to p.text when there is nothing to rotate, so a plain
            // prompt takes this branch too. A confidential entry is decrypted with the session key, or
            // the copy is refused while the vault is locked: ciphertext must never reach the clipboard
            // as the real text.
            Some(p) => {
                let key = secret_session_key(secret_timeout_min(&store.settings));
                let plain = reveal_rotation(p, key)?;
                CopyPayload::Text(rotation_next(&plain, &store.settings).2)
            }
            None => CopyPayload::Missing,
        }
    };
    let is_text = matches!(&payload, CopyPayload::Text(_));
    let ok = match payload {
        CopyPayload::Image(img) => copy_image_to_clipboard(&img),
        CopyPayload::File(path) => set_clipboard_file(&path),
        CopyPayload::Text(text) => put_text(&state, Some(&id), &text),
        CopyPayload::Missing => false,
    };
    // Only a successful text copy may consume a rotation entry: a failed copy leaves the
    // pointer where it was, and an Image/File payload showed no entry at all.
    if ok && is_text {
        let mut store = lock(&state);
        if let Some(pos) = store.prompts.iter().position(|p| p.id == id) {
            let p = rotation_core(&store.prompts[pos]);
            rotation_advance(&p, &mut store.settings);
            // The pointer survives a restart. Collected rather than written per
            // copy: it is already correct in memory for the next copy.
            save_settings_soon();
            emit_rotation_changed(&app, &p, &store.settings);
        }
    }
    Ok(ok)
}

// The prompt with a confidential entry decrypted, so rotation_next() on it can never
// hand ciphertext out. None while the vault is locked (or the key is wrong).
pub(crate) fn revealed_prompt(p: &Prompt, key: Option<[u8; 32]>) -> Option<Prompt> {
    reveal_rotation(p, key).ok()
}

// The one decrypt-and-slim step: the rotation fields of p with the confidential entries in
// the clear, or the vault's refusal while it is locked.
fn reveal_rotation(p: &Prompt, key: Option<[u8; 32]>) -> Result<Prompt, String> {
    let (text, variants) = reveal_secret_text(p, key)?;
    let mut plain = rotation_core(p);
    plain.text = text;
    plain.variants = variants;
    Ok(plain)
}

// What rotation_peek() reports for a "next: n/total" badge or a copy path that needs the
// resolved text before variable resolution (chaining, drag-out, hotkeys, tray,
// quick-launch): they peek, resolve, copy_text, then commit, so a cancelled variable dialog
// never burns an entry.
#[derive(Serialize)]
pub(crate) struct RotationPeek {
    pub(crate) index: usize,
    // 1-based "draw number within the current cycle": see rotation_next.
    pub(crate) pos: usize,
    pub(crate) total: usize,
    pub(crate) text: String,
}

#[tauri::command(async)]
pub(crate) fn rotation_peek(state: State<Db>, id: String) -> Option<RotationPeek> {
    let store = lock(&state);
    let p = store.prompts.iter().find(|p| p.id == id)?;
    // A locked confidential entry has no peek at all: the "next" badge simply
    // does not show rather than reveal ciphertext or the wrong text.
    let key = secret_session_key(secret_timeout_min(&store.settings));
    let plain = revealed_prompt(p, key)?;
    let (index, pos, text) = rotation_next(&plain, &store.settings);
    Some(RotationPeek { index, pos, total: rotation_total(&plain), text })
}

// Every window's tile/pill/folder-member badge stays live off one event, fired wherever the
// pointer moves (copy_prompt's success path, this command): the backend decides, the
// frontend reflects.
#[derive(Serialize, Clone)]
pub(crate) struct RotationChanged {
    pub(crate) id: String,
    pub(crate) index: usize,
    // 1-based "draw number within the current cycle": see rotation_next.
    pub(crate) pos: usize,
    pub(crate) total: usize,
    pub(crate) text: String,
}

pub(crate) fn emit_rotation_changed(app: &AppHandle, p: &Prompt, settings: &Settings) {
    if p.variants.is_empty() {
        return;
    }
    // Same rule as rotation_peek: a locked confidential entry emits nothing
    // rather than send ciphertext through a window event.
    let key = secret_session_key(secret_timeout_min(settings));
    let Some(plain) = revealed_prompt(p, key) else {
        return;
    };
    let (index, pos, text) = rotation_next(&plain, settings);
    let _ = app.emit(
        "rotation-changed",
        RotationChanged { id: plain.id.clone(), index, pos, total: rotation_total(&plain), text },
    );
}

#[tauri::command(async)]
pub(crate) fn rotation_commit(app: AppHandle, state: State<Db>, id: String) {
    let mut store = lock(&state);
    if let Some(pos) = store.prompts.iter().position(|p| p.id == id) {
        let p = rotation_core(&store.prompts[pos]);
        rotation_advance(&p, &mut store.settings);
        // The hottest settings write in the app: this runs on every paste of an
        // auto-advancing prompt, so it is collected instead of hitting the disk.
        save_settings_soon();
        emit_rotation_changed(&app, &p, &store.settings);
    }
}

// May this prompt start an auto-session (advance on every paste, not just
// on copy)? Image/file prompts have no rotation_next text path at all, so they
// are excluded here rather than relying on callers to check separately.
pub(crate) fn auto_eligible(p: &Prompt, s: &Settings) -> bool {
    s.ui_flags.get("rotatingPrompts") != Some(&false)
        && !p.variants.is_empty()
        && p.rotate_auto
        && s.ui_flags.get("rotateAuto") != Some(&false)
        && p.image.is_empty()
        && p.file_path.is_empty()
}

// Editor's "From the start" button: back to entry 1, same as a freshly
// created rotating prompt, without touching the entries themselves.
#[tauri::command(async)]
pub(crate) fn rotation_reset(app: AppHandle, state: State<Db>, id: String) {
    let mut store = lock(&state);
    rotation_forget(&mut store.settings, std::slice::from_ref(&id));
    save_settings(&app, &store.settings);
    // The pointer moved back to entry 1: tell every window's "next: n/total" badge.
    if let Some(p) = store.prompts.iter().find(|p| p.id == id).map(rotation_core) {
        emit_rotation_changed(&app, &p, &store.settings);
    }
}

// The placeholder round's entry point. The frontend resolves every entry's placeholders
// once (one dialog for the union of variables) and hands back `resolved` in absolute entry
// order; this arms the entry rotation_next() points at and advances the pointer. It
// replaces what copy_prompt would do for this one copy, so never call both for the same
// click.
#[tauri::command(async)]
pub(crate) fn rotation_arm_session(app: AppHandle, state: State<Db>, id: String, resolved: Vec<String>) -> bool {
    let mut store = lock(&state);
    let Some(p) = store.prompts.iter().find(|p| p.id == id).cloned() else {
        return false;
    };
    if !auto_eligible(&p, &store.settings) {
        return false;
    }
    // A locked confidential entry has no session: its stored text is ciphertext.
    let Some(plain) = revealed_prompt(&p, secret_session_key(secret_timeout_min(&store.settings))) else {
        return false;
    };
    let (idx, _, raw) = rotation_next(&plain, &store.settings);
    let text = resolved.get(idx).cloned().unwrap_or(raw);
    rotation_advance(&p, &mut store.settings);
    save_settings_soon(); // pointer only: collected, see SETTINGS_QUIET_MS
    emit_rotation_changed(&app, &p, &store.settings);
    drop(store);
    #[cfg(windows)]
    clipwatch::arm(&id, &text, resolved, p.secret);
    #[cfg(not(windows))]
    {
        let _ = resolved;
        note_own_copy(&text);
        let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(text));
    }
    true
}

// The tile menu / editor Auto toggle switching off while its session is the
// one running: end it immediately instead of waiting for the next paste/copy.
// A no-op if `id` is not the active session (stale click, already ended itself).
#[tauri::command]
pub(crate) fn rotation_end_auto(id: String) {
    #[cfg(windows)]
    clipwatch::end_if(&id);
    #[cfg(not(windows))]
    let _ = id;
}

// Put arbitrary text on the clipboard: used after filling placeholders, chains, drag-out,
// hotkeys, tray and quick-launch. `id` is the source prompt when there is one, so an
// auto-eligible prompt still opens its session even though the copy went through variable
// resolution (a plain copy_prompt would have resolved the placeholders twice).
#[tauri::command]
pub(crate) async fn copy_text(state: State<'_, Db>, text: String, id: Option<String>) -> Result<bool, String> {
    Ok(put_text(&state, id.as_deref(), &text))
}

// The CF_HTML clipboard payload, built by hand rather than through clipboard-win's header
// writer so the byte math is a plain function a unit test can call. Reference:
// https://learn.microsoft.com/windows/win32/dataxchg/html-clipboard-format. Every offset is
// a byte position in the UTF-8 string (the format requires it, and a multi-byte character
// in the fragment is worth its own test).
pub(crate) const CF_HTML_BODY_HEAD: &str = "<html>\r\n<body>\r\n<!--StartFragment-->";
pub(crate) const CF_HTML_BODY_FOOT: &str = "<!--EndFragment-->\r\n</body>\r\n</html>";

pub(crate) fn cf_html_header(start_html: usize, end_html: usize, start_fragment: usize, end_fragment: usize) -> String {
    format!(
        "Version:0.9\r\nStartHTML:{start_html:0>10}\r\nEndHTML:{end_html:0>10}\r\nStartFragment:{start_fragment:0>10}\r\nEndFragment:{end_fragment:0>10}\r\n"
    )
}

pub(crate) fn build_cf_html(fragment: &str) -> String {
    // The header's own placeholders are fixed-width (10 digits each), so its
    // byte length is already known before any real offset is computed.
    let header_len = cf_html_header(0, 0, 0, 0).len();
    let start_html = header_len;
    let start_fragment = header_len + CF_HTML_BODY_HEAD.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + CF_HTML_BODY_FOOT.len();
    let header = cf_html_header(start_html, end_html, start_fragment, end_fragment);
    format!("{header}{CF_HTML_BODY_HEAD}{fragment}{CF_HTML_BODY_FOOT}")
}

// Puts both the plain Markdown text (CF_UNICODETEXT) and its rendered form (CF_HTML) on the
// clipboard, so an app reading only CF_HTML sees formatting and one reading plain text sees
// what a normal copy gives. `html` is already escaped and sanitised by the caller, so it is
// passed to Windows unchanged. set_string empties the clipboard and claims CF_UNICODETEXT;
// CF_HTML is then added under its registered format without clearing what was just set.
#[cfg(windows)]
pub(crate) fn put_formatted(text: &str, html: &str) -> bool {
    let Ok(_clip) = clipboard_win::Clipboard::new_attempts(10) else {
        return false;
    };
    if clipboard_win::raw::set_string(text).is_err() {
        return false;
    }
    let Some(html_fmt) = clipboard_win::formats::Html::new() else {
        return false;
    };
    let payload = build_cf_html(html);
    clipboard_win::raw::set_without_clear(html_fmt.code(), payload.as_bytes()).is_ok()
}
#[cfg(not(windows))]
pub(crate) fn put_formatted(_text: &str, _html: &str) -> bool {
    false
}

#[tauri::command]
pub(crate) async fn copy_formatted(text: String, html: String) -> Result<bool, String> {
    // Noted first: the clipboard-update message can arrive before put_formatted returns,
    // and an own write that is not yet noted would be stored as an outside copy.
    note_own_copy(&text);
    Ok(put_formatted(&text, &html))
}

// `{?@clipboard}`: the clipboard's current text for the placeholder resolver.
// Empty when the clipboard holds no text at all (an image or file payload).
#[tauri::command]
pub(crate) async fn clipboard_text() -> String {
    // Bracketed as our own read: it renders a pending auto-advance promise, and that
    // render must not be counted as a paste that advances the rotation.
    #[cfg(windows)]
    let text = clipwatch::read_text_self();
    #[cfg(not(windows))]
    let text = arboard::Clipboard::new().and_then(|mut c| c.get_text()).unwrap_or_default();
    text
}

// Safety ceiling for the copy-history length; the live limit is the expert value
// historyMax (default 100, up to this ceiling). The history lives in its own
// table with O(log n) append/trim, so even the ceiling stays fast on every copy.
pub(crate) const COPY_HISTORY_MAX: usize = 1_000_000;

// One copy-history entry: which prompt + when (unix seconds).
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct CopyEntry {
    pub(crate) id: String,
    pub(crate) ts: u64,
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// Retention window from the expert "history retention" setting (days). Default 30 days for
// a new install; 0 or negative = forever. Stores older than this default are pinned to 0 in
// migrate().
pub(crate) const HISTORY_DAYS_DEFAULT: f64 = 30.0;
pub(crate) const RECORDING_RETENTION_DAYS_DEFAULT: f64 = 90.0;
pub(crate) fn history_max_age(settings: &Settings) -> Option<u64> {
    let days = settings.ui_values.get("historyDays").copied().unwrap_or(HISTORY_DAYS_DEFAULT);
    if days <= 0.0 {
        None
    } else {
        Some((days as u64).saturating_mul(86_400))
    }
}

// History length is an expert value (default 100, ceiling COPY_HISTORY_MAX), paired
// with the retention-window cutoff: the two limits `db_trim_copy` always applies
// together. One place for both, so startup trimming and live recording can't drift.
pub(crate) fn history_cap_and_cutoff(settings: &Settings) -> (usize, Option<u64>) {
    let cap = settings
        .ui_values
        .get("historyMax")
        .copied()
        .unwrap_or(100.0)
        .clamp(0.0, COPY_HISTORY_MAX as f64) as usize;
    let cutoff = history_max_age(settings).map(|age| now_secs().saturating_sub(age));
    (cap, cutoff)
}

// Drop copy-history entries older than the retention window (auto-delete).
pub(crate) fn prune_history(settings: &mut Settings) {
    if let Some(max_age) = history_max_age(settings) {
        let now = now_secs();
        // ts 0 = stored without a timestamp (the privacy toggle): never aged out.
        settings.copy_log.retain(|e| e.ts == 0 || now.saturating_sub(e.ts) <= max_age);
    }
}

// The copy-history row's on-demand trigger. db_trim_copy runs after every insert, but only
// against the cap/cutoff live then; lowering historyMax or historyDays trims nothing until
// the next copy, so this applies the current settings now. Row count before/after is the
// honest "size" figure (copy_log holds only a prompt id and a timestamp).
#[tauri::command(async)]
pub(crate) fn trim_copy_history_now(app: AppHandle, state: State<'_, Db>) -> (u32, u64) {
    let Some(conn) = db_conn(&app) else { return (0, 0) };
    let (cap, cutoff) = { history_cap_and_cutoff(&lock(&state).settings) };
    let before: (i64, i64) = conn
        .query_row("SELECT COUNT(*), COALESCE(SUM(LENGTH(id)), 0) FROM copy_log", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap_or((0, 0));
    if db_trim_copy(&conn, cap, cutoff).is_err() {
        return (0, 0);
    }
    let after: (i64, i64) = conn
        .query_row("SELECT COUNT(*), COALESCE(SUM(LENGTH(id)), 0) FROM copy_log", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap_or(before);
    ((before.0 - after.0).max(0) as u32, (before.1 - after.1).max(0) as u64)
}

// Record a copy in the history + usage stats (called by the UI after any copy).
#[tauri::command]
pub(crate) async fn record_copy(app: AppHandle, state: State<'_, Db>, id: String) -> Result<(), String> {
    let (entry, cap, cutoff, dedup) = {
        let mut store = lock(&state);
        // Respect the privacy toggle (expert menu): off = don't track.
        if store.settings.ui_flags.get("copyHistory") == Some(&false) {
            return Ok(());
        }
        // Expert opt: skip logging a copy identical to the previous one.
        let dedup = store.settings.ui_flags.get("dedupCopyLog") == Some(&true);
        if !store.prompts.iter().any(|p| p.id == id) {
            return Ok(());
        }
        let usage = {
            let slot = store.settings.usage.entry(id.clone()).or_insert(0);
            *slot += 1;
            *slot
        };
        // Timestamp storage is a privacy toggle (default on).
        let ts = if store.settings.ui_flags.get("historyTimestamps") == Some(&false) {
            0
        } else {
            now_secs()
        };
        if ts > 0 {
            store.settings.last_used.insert(id.clone(), ts);
        }
        let (cap, cutoff) = history_cap_and_cutoff(&store.settings);
        // Usage count + "last used" stamp only: collected, see SETTINGS_QUIET_MS.
        // The history row itself still goes into its own table below, untouched.
        save_settings_soon();
        // The main window's settings.usage/last_used mirror is refreshed only by a few explicit
        // refetches; a copy recorded from a pill, the folder window or the tray would leave it
        // stale and freeze the "most used" / "recently used" sorts, so it is told here.
        let _ = app.emit_to(
            "main",
            "usage-changed",
            serde_json::json!({ "id": id, "usage": usage, "lastUsed": if ts > 0 { Some(ts) } else { None } }),
        );
        (CopyEntry { id, ts }, cap, cutoff, dedup)
    };
    // The history itself lives in its own table: appended outside the lock so a
    // huge log never blocks other DB work and never bloats the settings blob.
    // No connection (a data-folder switch or a blocked folder) means no journal row either: said
    // so, not reported as written. The callers fire this after the clipboard write and ignore
    // the answer, so the copy itself is never held back.
    let Some(conn) = db_conn(&app) else {
        append_crash_log("copy history append failed: no database connection");
        return Err("history-failed".into());
    };
    if dedup {
        let last: Option<String> = conn
            .query_row("SELECT id FROM copy_log ORDER BY rowid DESC LIMIT 1", [], |r| r.get(0))
            .ok();
        if last.as_deref() == Some(entry.id.as_str()) {
            return Ok(()); // identical consecutive copy: don't add another row
        }
    }
    // An insert failure is logged, not reported as success: otherwise the entry would be
    // silently missing from the journal.
    if let Err(e) = db_append_copy(&conn, &entry, cap, cutoff) {
        append_crash_log(&format!("copy history append failed: {e}"));
        return Err("history-failed".into());
    }
    rebuild_tray(&app); // The "recently used" section follows every copy
    Ok(())
}

// The counters go only once the log rows are gone: a delete that failed leaves
// both the rows and the counters as they were, so the two never disagree.
pub(crate) fn clear_history_core(delete_log: impl FnOnce() -> bool, settings: &mut Settings) -> bool {
    if !delete_log() {
        return false;
    }
    settings.usage.clear();
    settings.last_used.clear();
    true
}

// Wipe copy history + usage counters (privacy / journal "clear").
#[tauri::command]
pub(crate) async fn clear_copy_history(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    // A "clear" the user asked for is a privacy action: a DB error must not be reported as
    // success while the log rows are still on disk.
    let (cleared, saved) = {
        let mut store = lock(&state);
        let cleared = clear_history_core(
            || match db_conn(&app) {
                Some(conn) => conn.execute("DELETE FROM copy_log", []).is_ok(),
                None => false,
            },
            &mut store.settings,
        );
        (cleared, cleared && save_settings(&app, &store.settings))
    };
    rebuild_tray(&app); // A wiped history empties the "recently used" section
    if !cleared {
        append_crash_log("clear_copy_history: copy_log could not be cleared");
        return Err("delete-incomplete".into());
    }
    if !saved {
        return Err("save-failed".into());
    }
    Ok(())
}

// One row as sent back by the frontend's own pre-clear snapshot (recent_copies
// already returns the same shape, just with an extra rowid the frontend drops).
#[derive(Deserialize)]
pub(crate) struct CopyLogRow {
    pub(crate) ts: u64,
    pub(crate) id: String,
}

// The insert loop behind restore_copy_history, kept apart from the command so it
// can be exercised with a bare in-memory connection. One transaction: a row that
// fails leaves the table exactly as it was, never a half-restored history.
//
// `rows` is the snapshot as recent_copies lists it, newest first. The log orders by rowid, so
// the rows go in oldest first: the newest copy ends up with the highest rowid again.
pub(crate) fn db_restore_copy_log(conn: &Connection, rows: &[CopyLogRow]) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    for row in rows.iter().rev() {
        tx.execute("INSERT INTO copy_log(ts, id) VALUES(?1, ?2)", params![row.ts as i64, row.id])?;
    }
    tx.commit()
}

// Undo for "clear copy history": writes the row-level log and the two counter maps back.
// The frontend holds the snapshot (read via recent_copies before the clear), so this only
// replays what existed a moment ago, through the same insert clause a live copy uses, never
// a raw table swap. Counters are written after the rows commit, so a failed restore
// changes nothing.
#[tauri::command]
pub(crate) async fn restore_copy_history(
    app: AppHandle,
    state: State<'_, Db>,
    entries: Vec<CopyLogRow>,
    usage: HashMap<String, u32>,
    last_used: HashMap<String, u64>,
) -> Result<(), String> {
    let Some(conn) = db_conn(&app) else {
        return Err("history-failed".into());
    };
    if db_restore_copy_log(&conn, &entries).is_err() {
        append_crash_log("restore_copy_history: the rows could not be reinserted");
        return Err("history-failed".into());
    }
    let saved = {
        let mut store = lock(&state);
        store.settings.usage = usage;
        store.settings.last_used = last_used;
        save_settings(&app, &store.settings)
    };
    rebuild_tray(&app);
    if !saved {
        return Err("save-failed".into());
    }
    Ok(())
}

// Newest copy-history entries for the journal (own table; capped for the UI).
// Async: the grouped query scans the table, so keep it off the UI thread.
#[tauri::command]
pub(crate) async fn recent_copies(app: AppHandle, limit: usize, grouped: bool) -> Vec<JournalEntry> {
    db_conn(&app)
        .map(|c| db_recent_copies(&c, limit.min(COPY_HISTORY_MAX), grouped))
        .unwrap_or_default()
}

// Remove one journal entry by its exact row: never "all copies of this
// prompt", so it behaves the same in grouped and ungrouped view.
#[tauri::command]
pub(crate) async fn delete_copy_entry(app: AppHandle, rowid: i64) -> bool {
    db_conn(&app)
        .map(|c| c.execute("DELETE FROM copy_log WHERE rowid = ?1", params![rowid]).is_ok())
        .unwrap_or(false)
}

