use super::*;

// ---------- Updates (GitHub releases) ----------

pub(crate) const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

// The project's own GitHub repo: the only host any of this ever talks to.
pub(crate) fn repo_url() -> String {
    format!("https://github.com/{}", brand::REPO)
}
pub(crate) fn update_api() -> String {
    format!("https://api.github.com/repos/{}/releases/latest", brand::REPO)
}
pub(crate) const UPDATE_MAX_BYTES: u64 = 100 * 1024 * 1024;

// Spawn a child process without flashing a console window.
#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Serialize, Clone)]
pub(crate) struct UpdateInfo {
    pub(crate) available: bool,
    pub(crate) version: String,
    pub(crate) url: String,
    // Release changelog (GitHub release body); empty when none was published.
    pub(crate) notes: String,
    // True when this version is on the user's skip list (manual check only).
    pub(crate) skipped: bool,
}

// The response body as text, capped at 10 MiB; invalid UTF-8 is an error, not a repair.
pub(crate) fn body_text(resp: ureq::http::Response<ureq::Body>) -> Option<String> {
    resp.into_body()
        .into_with_config()
        .limit(10 * 1024 * 1024)
        .read_to_string()
        .ok()
}

// Latest release tag, installer asset URL and changelog body. None on any
// failure (offline, private repo, rate limit): checks never disturb the app.
pub(crate) fn fetch_latest() -> Option<(String, String, String)> {
    let body = body_text(
        crate::ffmpeg::request_agent().get(&update_api())
            .header("User-Agent", brand::USER_AGENT)
            .config()
            .timeout_global(Some(std::time::Duration::from_secs(10)))
            .build()
            .call()
            .ok()?,
    )?;
    let json: serde_json::Value = serde_json::from_str(&body).ok()?;
    let tag = json["tag_name"].as_str()?.trim_start_matches('v').to_string();
    let url = json["assets"].as_array()?.iter().find_map(|a| {
        let name = a["name"].as_str()?;
        if name.ends_with("-setup.exe") {
            a["browser_download_url"].as_str().map(String::from)
        } else {
            None
        }
    })?;
    let notes = json["body"].as_str().unwrap_or("").trim().to_string();
    Some((tag, url, notes))
}

pub(crate) fn version_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.').map(|p| p.parse().unwrap_or(0)).collect()
    };
    // Compare component-wise, zero-padding the shorter version, so "1.9" and
    // "1.9.0" rank equal instead of one being treated as newer.
    let (a, b) = (parse(latest), parse(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

pub(crate) fn updater_check() -> Option<UpdateInfo> {
    let (version, url, notes) = fetch_latest()?;
    version_newer(&version, APP_VERSION).then_some(UpdateInfo {
        available: true,
        version,
        url,
        notes,
        skipped: false,
    })
}

// Far enough past launch that the first check never competes with window build.
pub(crate) const UPDATE_CHECK_FIRST_DELAY_SECS: u64 = 2;

// Hours between automatic update checks, kept inside what the setting offers. Never
// means "off" here: that is the Auto update switch, not a short or zero interval.
pub(crate) fn clamp_update_hours(hours: f64) -> f64 {
    if hours.is_finite() {
        hours.clamp(1.0, 168.0)
    } else {
        24.0
    }
}

// Expert-tunable, clamped to the range the setting offers.
pub(crate) fn update_check_interval(app: &AppHandle) -> Duration {
    let hours = app
        .try_state::<Db>()
        .map(|s| {
            s.lock()
                .unwrap_or_else(|e| e.into_inner())
                .settings
                .ui_values
                .get("updateCheckH")
                .copied()
                .unwrap_or(24.0)
        })
        .unwrap_or(24.0);
    Duration::from_secs((clamp_update_hours(hours) * 3600.0) as u64)
}

// The automatic check goes online only with the switch on AND an explicit yes
// (or an existing install's migrated choice). An unanswered question counts as no.
pub(crate) fn update_check_allowed(s: &Settings) -> bool {
    s.auto_update && s.update_consent == UpdateConsent::Yes
}

// One round of the automatic update check.
pub(crate) fn update_check_tick(app: &AppHandle) {
    let enabled = app
        .try_state::<Db>()
        .map(|s| update_check_allowed(&s.lock().unwrap_or_else(|e| e.into_inner()).settings))
        .unwrap_or(false);
    if !enabled {
        return;
    }
    let Some(info) = updater_check() else { return };
    // Honour the skip list: a skipped version never pops up on its own, only on
    // a manual check.
    let skipped = app
        .try_state::<Db>()
        .map(|s| {
            s.lock()
                .unwrap_or_else(|e| e.into_inner())
                .settings
                .skipped_versions
                .contains(&info.version)
        })
        .unwrap_or(false);
    if !skipped {
        let _ = app.emit("update-available", info);
    }
}

#[tauri::command(async)]
pub(crate) fn set_bars(app: AppHandle, state: State<Db>, header: bool, composer: bool) {
    let mut store = lock(&state);
    store.settings.show_header = header;
    store.settings.show_composer = composer;
    save_settings(&app, &store.settings);
}

// Stores the update choice and saves it. A failed save puts the previous values back, so the
// background check never acts on a switch the user saw revert.
pub(crate) fn set_update_choice(
    settings: &mut Settings,
    enabled: bool,
    save: impl FnOnce(&Settings) -> bool,
) -> Result<(), String> {
    let previous = (settings.auto_update, settings.update_consent);
    settings.auto_update = enabled;
    settings.update_consent = if enabled { UpdateConsent::Yes } else { UpdateConsent::No };
    if !save(settings) {
        (settings.auto_update, settings.update_consent) = previous;
        return Err("save-failed".to_string());
    }
    Ok(())
}

#[tauri::command(async)]
pub(crate) fn set_auto_update(app: AppHandle, state: State<Db>, enabled: bool) -> Result<(), String> {
    let mut store = lock(&state);
    set_update_choice(&mut store.settings, enabled, |s| save_settings(&app, s))
}

// The first-launch question: remembers the answer and the switch together.
#[tauri::command(async)]
pub(crate) fn set_update_consent(app: AppHandle, state: State<Db>, enabled: bool) -> Result<(), String> {
    let mut store = lock(&state);
    set_update_choice(&mut store.settings, enabled, |s| save_settings(&app, s))
}

#[tauri::command]
pub(crate) fn app_version() -> String {
    APP_VERSION.to_string()
}

// Async: the release-info fetch runs on a blocking worker so it never
// occupies the async runtime; the settings-lock read stays after the await
// since it is short and purely in-memory.
#[tauri::command]
pub(crate) async fn check_update(state: State<'_, Db>) -> Result<UpdateInfo, String> {
    let latest = tauri::async_runtime::spawn_blocking(fetch_latest).await.map_err(|e| e.to_string())?;
    match latest {
        Some((version, url, notes)) => {
            let available = version_newer(&version, APP_VERSION);
            let skipped = available && lock(&state).settings.skipped_versions.contains(&version);
            Ok(UpdateInfo {
                available,
                version: if available { version } else { APP_VERSION.to_string() },
                url: if available { url } else { String::new() },
                notes: if available { notes } else { String::new() },
                skipped,
            })
        }
        None => Err("update check failed".to_string()),
    }
}

// Runs one automatic check right now, e.g. straight after the first-launch
// question was answered with yes, instead of waiting for the next launch. It
// still honours the stored consent and the switch like the scheduled round.
#[tauri::command(async)]
pub(crate) fn run_update_check(app: AppHandle) {
    update_check_tick(&app);
}

// The live effects of a flag switch that do not depend on the store lock: the window
// capture exclusion, the collected error list and a running auto-advance session. Shared by
// the settings window and the command line, so a `settings set` takes effect
// without a restart there too.
pub(crate) fn apply_flag_effects(app: &AppHandle, key: &str, enabled: bool) {
    // Capture exclusion applies live to every open window; never hold the store lock across
    // window API calls (deadlock risk). The flag says "allow screenshots", so the exclusion is
    // its opposite.
    if key == ALLOW_SCREENSHOT_KEY {
        apply_capture_exclusion_all(app, !enabled);
    }
    // "Off" means the list is empty, not just paused: clear whatever is
    // already collected the moment the switch goes off.
    if key == "errorLog" && !enabled {
        ERROR_LOG.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
    // A running key capture must not outlive the switch.
    #[cfg(windows)]
    if key == "keyOverlay" {
        keylog::flag_changed(enabled);
    }
    // Turning either gate off must not leave a session silently running.
    #[cfg(windows)]
    if !enabled && (key == "rotatingPrompts" || key == "rotateAuto") {
        clipwatch::end_any();
    }
}

// Toggle an expert feature flag. Err only where switching it ON has a hard precondition
// that failed (e.g. the safety backup prompt-sync takes before its first write); the flag
// is then left OFF so the UI and the store never disagree. Runs off the main thread: the
// activation backup takes as long as the library is large.
#[tauri::command(async)]
pub(crate) fn set_ui_flag(app: AppHandle, state: State<Db>, key: String, enabled: bool) -> Result<(), String> {
    let is_clip = key == "clipWatcher";
    let is_hotkeys = key == "promptHotkeys";
    let is_tray = key == "trayRecent" || key == "trayFavs" || key == TRAY_CUSTOM_MENU_KEY;
    let is_folders = key == "folders";
    let is_sync_enable = key == "promptSync" && enabled; // Prompt-folders-as-Markdown
    // Turning this on while demo mode is active is what enable_demo_mode/disable_demo_mode's
    // real-instance check exists to prevent: a second process started once this is allowed
    // would resolve the redirect per write like the first, so once either flips it, the other's
    // writes land in the wrong store. Defense in depth next to that check; this one is only a
    // settings read, not proof that an instance runs.
    if key == "allowMultipleInstances" && enabled {
        let store = lock(&state);
        if store.settings.ui_flags.get("demoMode").copied().unwrap_or(false) {
            return Err("demo-mode-active".to_string());
        }
    }
    // The activation work below (a full backup, then one file per prompt) takes as long as the
    // library is large, so it runs on a snapshot taken under a short lock, like
    // spawn_sync_write_all: otherwise every copy, save and other window would wait for it.
    let clear_ocr_index = key == "ocrIndex" && !enabled;
    // From before the flag exists: a save during the backup would otherwise queue a write of
    // this library into a folder that has not been read yet.
    let sync_hold = is_sync_enable.then(sync_hold_writes_for_activation);
    let snapshot = {
        let mut store = lock(&state);
        store.settings.ui_flags.insert(key.clone(), enabled);
        save_settings(&app, &store.settings);
        if clear_ocr_index {
            // A running "read existing pictures" pass would write the text straight back.
            if clear_ocr_index_text(&mut store) {
                write_ocr_wipe(&app, &store, "index switch-off");
            }
        }
        is_sync_enable.then(|| store.settings.clone())
    };
    if let Some(settings) = snapshot {
        // Switching the feature back off has to happen under its own lock, and
        // must not clobber whatever else was written in the meantime.
        let abort = |app: &AppHandle, state: &State<Db>| {
            let mut store = lock(state);
            store.settings.ui_flags.insert("promptSync".to_string(), false);
            save_settings(app, &store.settings);
        };
        // Safety net before the first sync run touches this library: the same backup a manual one
        // takes, so a bad first sync is recoverable. A failure here aborts the activation instead
        // of syncing unprotected.
        let keep = settings.ui_values.get("backupKeep").copied().unwrap_or(10.0).max(1.0) as usize;
        let gfs = gfs_from_settings(&settings);
        if let Err(e) = do_backup(&app, keep, gfs) {
            append_crash_log(&format!("prompt-sync activation backup failed: {e}"));
            abort(&app, &state);
            return Err(format!("sync-backup-failed: {e}"));
        }
        // Synchronous and immediate on purpose (unlike spawn_sync_write_all): this is a one-time,
        // user-initiated activation and the folder must really exist before the command returns
        // "on". If it does not, the activation fails visibly instead of leaving the switch on over
        // an empty folder.
        let failed = sync_activate_impl(&app);
        if failed > 0 {
            abort(&app, &state);
            return Err(format!("sync-write-failed: {failed}"));
        }
        // The poll loop ends itself when it next finds the feature off, so switching back on needs
        // a fresh one; the claim inside start_sync_poll makes this safe even if one is still
        // winding down. The hold ends first: the folder is learned and written by now, and
        // start_sync_poll raises its own for the first read.
        drop(sync_hold);
        start_sync_poll(app.clone());
    }
    apply_flag_effects(&app, &key, enabled);
    // The window itself always runs (started at boot) because the auto-advance session needs
    // it; the toggle only changes what on_clipboard_update does with each event, checked live.
    let _ = is_clip;
    // Switching the feature off unregisters every prompt accelerator but keeps
    // the stored ones, so switching it back on restores them all.
    if is_hotkeys {
        rearm_hotkeys(&app);
    }
    // Switching a section on or off redraws the tray menu.
    if is_tray {
        apply_tray_menu(&app);
    }
    // Switching folders off puts every member back on screen as its own pill (the folders are
    // kept, so switching on restores them). Opening and closing windows inline from a command
    // deadlocks WebView2, hence the hop onto the runtime.
    if is_folders {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move { sync_float_windows(&handle) });
    }
    notify_float_settings(&app);
    Ok(())
}

// The value a numeric setting is stored as: held to the range the setting offers
// (the same rule as when settings load), and a number that is not finite is refused.
pub(crate) fn checked_ui_value(key: &str, value: f64) -> Result<f64, String> {
    let value = if key == "updateCheckH" { clamp_update_hours(value) } else { value };
    clamp_ui_value(key, value).ok_or_else(|| "invalid number".to_string())
}

// Set an expert numeric value (CSS var / behaviour tweak).
#[tauri::command(async)]
pub(crate) fn set_ui_value(app: AppHandle, state: State<Db>, key: String, value: f64) -> Result<(), String> {
    let is_tray = key == "trayRecentN" || key == "trayFavN";
    let is_error_log_cap = key == "errorLogKeepKb";
    // A stored interval outside what the setting offers would show as one thing and
    // run as another.
    let value = checked_ui_value(&key, value)?;
    {
        let mut store = lock(&state);
        store.settings.ui_values.insert(key.clone(), value);
        save_settings(&app, &store.settings);
    }
    if is_tray {
        rebuild_tray(&app); // dragging a slider must not rebuild on every step
    }
    // append_crash_log has no DB access at most of its call sites, so its
    // rotation cap lives in this static instead, kept current from here.
    if is_error_log_cap {
        ERROR_LOG_KEEP_BYTES.store((value.max(1.0) * 1024.0) as u64, Ordering::Relaxed);
    }
    // A new update interval counts from the last check, so the running job picks it up now
    // instead of after its old wait. The job still does nothing while Auto update is off.
    if key == "updateCheckH" {
        scheduler::retime("update-check", update_check_interval(&app));
    }
    notify_float_settings(&app);
    Ok(())
}

// Settings that name a folder the backend itself writes into or deletes from. The generic
// setter and a settings/backup import must never point one at an arbitrary folder: each
// changes only through its own command (set_screenshot_dir, set_video_export_dir,
// set_recording_dir, set_sync_dir), so a subverted renderer or a hostile imported settings
// file cannot grant itself write or delete authority over a folder of its choosing.
pub(crate) const PROTECTED_FOLDER_KEYS: [&str; 4] = ["screenshotDir", "videoExportDir", "recordingDir", "syncDir"];

pub(crate) fn is_protected_folder_key(key: &str) -> bool {
    PROTECTED_FOLDER_KEYS.contains(&key)
}

// Text rows with their own validated writer (folders, sealed backup password, vault salt
// and verifier) or none at all (leftover receiver rows): the generic string setter never
// writes them.
pub(crate) fn is_generic_text_write_denied(key: &str) -> bool {
    is_machine_text_key(key) || is_vault_key(key)
}

// Text rows only the screenshot editor reads (the last size per tool and the last tool). No
// other window shows them, so writing one must not wake every float and folder window.
pub(crate) fn text_write_notifies(key: &str) -> bool {
    !matches!(key, "snipToolSizes" | "snipLastTool")
}

// Set an expert string option (e.g. the copy-feedback font key).
#[tauri::command(async)]
pub(crate) fn set_ui_text(app: AppHandle, state: State<Db>, key: String, value: String) {
    if is_generic_text_write_denied(&key) {
        return;
    }
    let notify = text_write_notifies(&key);
    {
        let mut store = lock(&state);
        store.settings.ui_texts.insert(key, value);
        save_settings(&app, &store.settings);
    }
    if notify {
        notify_float_settings(&app);
    }
}

// Dedicated setter for the screenshot save folder: the expert menu's "Change" button calls
// pick_folder, then this. Kept out of the generic set_ui_text path (see
// PROTECTED_FOLDER_KEYS).
#[tauri::command(async)]
pub(crate) fn set_screenshot_dir(app: AppHandle, state: State<Db>, dir: String) -> Result<(), String> {
    let value = accept_dir_setting(&dir, Path::new(&default_screenshot_dir()), "screenshot")?;
    let mut store = lock(&state);
    store.settings.ui_texts.insert("screenshotDir".to_string(), value);
    save_settings(&app, &store.settings);
    drop(store);
    notify_float_settings(&app);
    Ok(())
}

// Dedicated setter for the video editor's export folder, like set_screenshot_dir and for
// the same reason. There is no built-in default folder (empty means "next to the source
// file"), so accept_dir_setting's default check is a no-op past the empty case.
#[tauri::command(async)]
pub(crate) fn set_video_export_dir(app: AppHandle, state: State<Db>, dir: String) -> Result<(), String> {
    let value = accept_dir_setting(&dir, Path::new(""), "videoExport")?;
    let mut store = lock(&state);
    store.settings.ui_texts.insert("videoExportDir".to_string(), value);
    save_settings(&app, &store.settings);
    drop(store);
    notify_float_settings(&app);
    Ok(())
}

// Dedicated setter for the screen-recording save folder: same shape as
// set_screenshot_dir above, and for the same reason.
#[tauri::command(async)]
pub(crate) fn set_recording_dir(app: AppHandle, state: State<Db>, dir: String) -> Result<(), String> {
    let value = accept_dir_setting(&dir, Path::new(&default_recording_dir()), "recording")?;
    let mut store = lock(&state);
    store.settings.ui_texts.insert("recordingDir".to_string(), value);
    save_settings(&app, &store.settings);
    drop(store);
    notify_float_settings(&app);
    Ok(())
}

// Clears the expert flags, values and texts. The texts that are keys rather than
// preferences stay: the backup password and
// the vault's salt and verifier (the confidential prompts are sealed with it).
pub(crate) fn reset_expert_settings(settings: &mut Settings) {
    settings.ui_flags.clear();
    settings.ui_values.clear();
    let kept = settings_after_reset(settings).ui_texts;
    settings.ui_texts = kept;
}

// Clear all expert overrides back to the shipped defaults.
#[tauri::command(async)]
pub(crate) fn reset_expert(app: AppHandle, state: State<Db>) {
    {
        let mut store = lock(&state);
        let ocr_was_on = ocr_index_flag_on(&store);
        reset_expert_settings(&mut store.settings);
        save_settings(&app, &store.settings);
        // The reset turned the picture-text index back off: its text goes with it.
        if ocr_was_on && clear_ocr_index_text(&mut store) {
            write_ocr_wipe(&app, &store, "expert reset");
        }
    }
    ERROR_LOG_KEEP_BYTES.store(256 * 1024, Ordering::Relaxed);
    // Cleared flags put the screenshot permission back to its default (off), which
    // means the exclusion is on: re-assert it on every window so it is immediate.
    apply_capture_exclusion_all(&app, true);
    apply_tray_menu(&app); // The tray sections fall back to their defaults too
    notify_float_settings(&app);
    // The key overlay falls back to its default too: a running capture must not outlive it.
    #[cfg(windows)]
    {
        let on = keylog::enabled(&lock(&state).settings.ui_flags);
        keylog::flag_changed(on);
    }
}

// Add a version to the skip list: it will not be offered again.
#[tauri::command(async)]
pub(crate) fn skip_version(app: AppHandle, state: State<Db>, version: String) {
    let mut store = lock(&state);
    if add_skipped_version(&mut store.settings.skipped_versions, version) {
        save_settings(&app, &store.settings);
    }
}

const SKIPPED_VERSIONS_MAX: usize = 20;

// Adds a version-shaped string to the skip list and keeps only the newest entries. Shaped means
// at most 32 characters of [0-9A-Za-z.-], starting with a digit and not ending in a dash or dot,
// so a pre-release tag like 1.2.3-beta.1 counts. Returns whether the list changed.
pub(crate) fn add_skipped_version(list: &mut Vec<String>, version: String) -> bool {
    let shaped = version.len() <= 32
        && version.chars().next().is_some_and(|c| c.is_ascii_digit())
        && version.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !version.ends_with(['.', '-']);
    if !shaped || list.contains(&version) {
        return false;
    }
    list.push(version);
    if list.len() > SKIPPED_VERSIONS_MAX {
        let excess = list.len() - SKIPPED_VERSIONS_MAX;
        list.drain(..excess);
    }
    true
}

// The shipped changelog, baked into the binary at compile time: the "what's
// new" window needs nothing from the network to show the current version.
pub(crate) const EMBEDDED_RELEASE_NOTES: &str = include_str!("../../docs/release-notes.md");

#[derive(Serialize, Clone)]
pub(crate) struct WhatsNewSection {
    pub(crate) version: String,
    // Raw Markdown including its own "## <version>" heading. The frontend renders it node by
    // node (the safe path the update-changelog dialog uses), never through innerHTML.
    pub(crate) markdown: String,
}

#[derive(Serialize, Clone)]
pub(crate) struct WhatsNewResult {
    pub(crate) sections: Vec<WhatsNewSection>,
}

// Split a release-notes document on its "## " version headings. A heading's
// version is its last whitespace-separated token, so "## Prompt Saver 2.7.0"
// yields "2.7.0"; a heading without a trailing version-shaped token is
// skipped rather than guessed at.
pub(crate) fn parse_release_notes(text: &str) -> Vec<WhatsNewSection> {
    let mut sections = Vec::new();
    let mut current: Option<(String, Vec<&str>)> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            if let Some((version, body)) = current.take() {
                sections.push(WhatsNewSection { version, markdown: body.join("\n").trim().to_string() });
            }
            let version = rest.split_whitespace().last().unwrap_or("").to_string();
            if version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                current = Some((version, vec![line]));
            } else {
                current = None; // heading without a version-shaped token, not a release section
            }
        } else if let Some((_, body)) = current.as_mut() {
            body.push(line);
        }
    }
    if let Some((version, body)) = current {
        sections.push(WhatsNewSection { version, markdown: body.join("\n").trim().to_string() });
    }
    sections
}

// True when `a` and `b` name the same version, component-wise, like
// version_newer, so "1.9" and "1.9.0" compare equal.
pub(crate) fn version_eq(a: &str, b: &str) -> bool {
    !version_newer(a, b) && !version_newer(b, a)
}

// Unchanged version shows nothing; a version jump shows every section after `since` up to
// and including `current`; an unknown or absent `since` (fresh install, or a caller wanting
// only the current entry) shows the current version only.
pub(crate) fn sections_for_update(all: &[WhatsNewSection], since: Option<&str>, current: &str) -> Vec<WhatsNewSection> {
    match since {
        Some(v) if version_eq(v, current) => Vec::new(),
        Some(v) => all
            .iter()
            .filter(|s| version_newer(&s.version, v) && !version_newer(&s.version, current))
            .cloned()
            .collect(),
        None => all.iter().filter(|s| version_eq(&s.version, current)).cloned().collect(),
    }
}

// An empty stored version means "never shown" (fresh install), split out so
// this mapping is unit-testable without an AppHandle (mirrors version_newer).
pub(crate) fn whats_new_check_impl(all: &[WhatsNewSection], last_seen: &str, current: &str) -> Vec<WhatsNewSection> {
    let since = if last_seen.is_empty() { None } else { Some(last_seen) };
    sections_for_update(all, since, current)
}

// Startup check: what changed since the version this install last showed. Marks the
// current version as seen immediately, whether or not the caller displays the result, like
// the toast, which never returns once marked seen.
#[tauri::command(async)]
pub(crate) fn whats_new_check(app: AppHandle, state: State<Db>) -> WhatsNewResult {
    let all = parse_release_notes(EMBEDDED_RELEASE_NOTES);
    let mut store = lock(&state);
    let sections = whats_new_check_impl(&all, &store.settings.last_seen_version, APP_VERSION);
    store.settings.last_seen_version = APP_VERSION.to_string();
    save_settings(&app, &store.settings);
    WhatsNewResult { sections }
}

// About dialog: always just the current version's own section, from the
// shipped copy. Opening it never marks anything as seen.
#[tauri::command]
pub(crate) fn whats_new_current() -> Vec<WhatsNewSection> {
    let all = parse_release_notes(EMBEDDED_RELEASE_NOTES);
    sections_for_update(&all, None, APP_VERSION)
}

// Strictly pin an update download URL to `<repo>/releases/download/<tag>/<asset>.exe`.
// A bare `starts_with` prefix check is not enough: the URL crate (and Windows) treat
// `..` and `\` as path separators, so `.../releases/download/../../attacker/...` would
// pass a prefix check yet resolve to a different repo. Reject every escape vector and
// require exactly the two trailing path segments (tag / asset), no query or fragment.
pub(crate) fn valid_update_url(url: &str) -> bool {
    let prefix = format!("{}/releases/download/", repo_url());
    let rest = match url.strip_prefix(prefix.as_str()) {
        Some(r) => r,
        None => return false,
    };
    let low = rest.to_ascii_lowercase();
    if rest.contains("..") || rest.contains('\\') || rest.contains("//")
        || rest.contains(':') || rest.contains('@') || rest.contains('?')
        || rest.contains('#') || low.contains("%2e") || low.contains("%2f")
        || low.contains("%5c")
    {
        return false;
    }
    let mut segs = rest.splitn(3, '/');
    let tag = segs.next().unwrap_or("");
    let asset = segs.next().unwrap_or("");
    let extra = segs.next();
    !tag.is_empty() && !asset.is_empty() && extra.is_none() && low.ends_with(".exe")
}

// Re-derive the release tag from a pinned download URL (see `valid_update_url` above)
// and confirm it is actually newer than the installed version. Kept as its own pure
// function so `install_update` cannot install a version-shaped-but-old release, and so
// the downgrade rejection is unit-testable without needing a live AppHandle.
pub(crate) fn update_url_is_newer_release(url: &str) -> bool {
    let prefix = format!("{}/releases/download/", repo_url());
    let tag = url
        .strip_prefix(prefix.as_str())
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("")
        .trim_start_matches('v');
    !tag.is_empty() && version_newer(tag, APP_VERSION)
}

// The version read from the downloaded installer itself must be newer than this app; an
// installer without a readable version is refused.
pub(crate) fn installer_version_is_newer(installer_version: Option<&str>) -> bool {
    installer_version.is_some_and(|v| version_newer(v, APP_VERSION))
}

// The release page of the version a pinned download URL belongs to. A portable
// copy is not replaced by the installer, so it is sent there instead.
pub(crate) fn release_page_for(url: &str) -> Option<String> {
    if !valid_update_url(url) {
        return None;
    }
    let prefix = format!("{}/releases/download/", repo_url());
    let tag = url.strip_prefix(prefix.as_str())?.split('/').next()?;
    Some(format!("{}/releases/tag/{}", repo_url(), tag))
}

// The installer ends the app, which would cut a live recording short (an MP4
// with no index, unplayable) or abandon a running export, so it waits for them.
// A recording counts while it switches files too, when the state reads empty.
pub(crate) fn update_blocker(recording: bool, switching: bool, jobs: usize) -> Option<&'static str> {
    if recording || switching {
        Some("recording-active")
    } else if jobs > 0 {
        Some("jobs-running")
    } else {
        None
    }
}

// Download the installer to %TEMP%, run it fully silent (/S), restart the
// app afterwards and quit so the installer can replace the binaries.
#[tauri::command]
pub(crate) async fn install_update(app: AppHandle, url: String) -> Result<(), String> {
    // The setup would install a second, installed copy next to this portable
    // exe and restart the old one. The release page opens instead; the person
    // replaces the portable exe by hand.
    if is_portable() {
        let page = release_page_for(&url).ok_or_else(|| "invalid update source".to_string())?;
        std::process::Command::new("explorer").arg(page).spawn().map_err(|e| e.to_string())?;
        return Err("portable-update-manual".to_string());
    }
    if let Some(why) = update_blocker(recording_active(&app), REC_STARTING.load(Ordering::SeqCst), bg_ops_running_count()) {
        return Err(why.to_string());
    }
    // Only our own release assets under the exact pinned repo path: not any github.com URL.
    if !valid_update_url(&url) {
        return Err("invalid update source".to_string());
    }
    // The shape check above only proves the download stays inside our repo's release path, not
    // the version. Re-assert that the release tag in the pinned path is newer than what is
    // installed, so an older release at a valid URL (stale link, rolled-back asset) can never
    // be installed as an update.
    if !update_url_is_newer_release(&url) {
        return Err("this release is not newer than the installed version".to_string());
    }
    // Download, verify and stage the installer on a blocking worker so none of
    // it occupies the async runtime; only `url` (an owned String) moves in.
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> { install_update_blocking(url) })
        .await
        .map_err(|e| e.to_string())??;

    // Exit slightly delayed so this command's reply still reaches the UI.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(800));
        if let Some(state) = app.try_state::<Db>() {
            let store = state.lock().unwrap_or_else(|e| e.into_inner());
            save_settings(&app, &store.settings);
        }
        #[cfg(windows)]
        clipwatch::stop(); // lets DestroyWindow flush a pending auto-session for real
        app.exit(0);
    });
    Ok(())
}

// Public half of the key every release installer is signed with (`tools/sign_update.js`).
// The private half never leaves the publisher's machine.
pub(crate) const UPDATE_PUBLIC_KEY_B64: &str = "Z1ty4r3YnfqsEo+h3gee71jt+QcyINxbMoDCMkEXVZ8=";

// True when `sig_text` (base64, as published in `<installer>.sig`) is a valid Ed25519
// signature over the installer's SHA-256 digest under `public_key_b64`.
pub(crate) fn update_signature_valid(digest: &[u8], sig_text: &str, public_key_b64: &str) -> bool {
    // base64_decode answers malformed input with an empty Vec: an empty signature or key is
    // never valid, so a garbage file fails closed here instead of reaching the verifier.
    let (sig, key) = (base64_decode(sig_text.trim()), base64_decode(public_key_b64));
    if sig.is_empty() || key.is_empty() {
        return false;
    }
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
        .verify(digest, &sig)
        .is_ok()
}

/// Text of the helper script. A batch file expands `%NAME%` even inside quotes, so a percent
/// sign in either path is doubled; a quote or line break cannot be quoted safely and is refused.
pub(crate) fn update_script_content(installer: &Path, app_exe: &Path) -> Result<String, String> {
    fn quoted(p: &Path) -> Result<String, String> {
        let t = p.to_string_lossy();
        if t.contains(['"', '\r', '\n']) {
            return Err("invalid update source".to_string());
        }
        Ok(t.replace('%', "%%"))
    }
    let inst = quoted(installer)?;
    let exe = quoted(app_exe)?;
    // The stage folder goes last, on the line that also removes the script itself (the line is
    // already read, so it still runs); the working folder is moved away from it first.
    let stage = quoted(installer.parent().unwrap_or(installer))?;
    Ok(format!(
        "@echo off\r\n\"{inst}\" /S\r\nstart \"\" \"{exe}\"\r\ndel \"{inst}\"\r\ncd /d \"%TEMP%\"\r\ndel \"%~f0\" & rmdir \"{stage}\"\r\n"
    ))
}

// The blocking body of `install_update`, split out so it runs in spawn_blocking without an
// AppHandle crossing the boundary.
pub(crate) fn install_update_blocking(url: String) -> Result<(), String> {
    let resp = download_agent().get(&url)
        .header("User-Agent", brand::USER_AGENT)
        .call()
        .map_err(|e| format!("download: {}", e))?;
    let mut bytes = Vec::new();
    use std::io::Read;
    resp.into_body()
        .into_reader()
        .take(UPDATE_MAX_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("read: {}", e))?;
    // A file that exactly hit the cap was almost certainly truncated: never run a
    // partial installer.
    if bytes.len() as u64 >= UPDATE_MAX_BYTES {
        return Err("installer too large / truncated".to_string());
    }
    // Sanity-check the download is a Windows executable (PE "MZ" header): a corruption or
    // wrong-format guard, not authentication. The real integrity check is the SHA-256
    // verification below.
    if bytes.len() < 2 || &bytes[0..2] != b"MZ" {
        return Err("downloaded file is not a valid installer".to_string());
    }
    // Integrity: the SHA-256 published beside the installer (`<installer>.sha256`, same pinned
    // path) must be present and match. Fail closed: an installer we cannot authenticate is
    // never executed, so a missing, unreadable or malformed hash file aborts the update like a
    // mismatch. Every release publishes the hash (`release:publish` in package.json); if one
    // ever does not, the update is done by hand.
    let want = crate::ffmpeg::request_agent().get(&format!("{}.sha256", url))
        .header("User-Agent", brand::USER_AGENT)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .call()
        .ok()
        .and_then(body_text)
        .map(|t| t.split_whitespace().next().unwrap_or("").to_ascii_lowercase())
        .filter(|w| w.len() == 64 && w.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| "no usable checksum published for this update, download rejected".to_string())?;
    let digest = crate::ffmpeg::sha256(&bytes);
    if crate::ffmpeg::hex_lower(&digest) != want {
        return Err("update hash mismatch: download rejected".to_string());
    }
    // Origin: the hash proves the download is intact, the signature that the publisher made
    // it. Someone who could replace both release files still has no signing key.
    let sig = crate::ffmpeg::request_agent().get(&format!("{}.sig", url))
        .header("User-Agent", brand::USER_AGENT)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .call()
        .ok()
        .and_then(body_text)
        .unwrap_or_default();
    if !update_signature_valid(&digest, &sig, UPDATE_PUBLIC_KEY_B64) {
        return Err("update signature invalid: download rejected".to_string());
    }
    // Stage into a fresh, uniquely-named temp subdir so a local attacker can't pre-plant
    // the installer/script at a predictable path (TOCTOU).
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let stage = std::env::temp_dir().join(format!("{}-update-{}-{}", brand::SLUG, std::process::id(), stamp));
    fs::create_dir_all(&stage).map_err(|e| format!("stage dir: {}", e))?;
    let installer = stage.join("setup.exe");
    fs::write(&installer, &bytes).map_err(|e| format!("save installer: {}", e))?;
    // The signature covers the installer bytes, so the version inside them is the signed one;
    // the tag in the URL is not. An older genuine installer re-published under a new tag must
    // not install as an update.
    #[cfg(windows)]
    if !installer_version_is_newer(crate::win_windows::file_version_of(&installer.to_string_lossy()).as_deref()) {
        let _ = fs::remove_dir_all(&stage);
        return Err("this release is not newer than the installed version".to_string());
    }

    // Helper script: silent install, relaunch the app, delete the installer + itself.
    let app_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let script = stage.join("update.cmd");
    let content = update_script_content(&installer, &app_exe)?;
    fs::write(&script, content).map_err(|e| format!("save script: {}", e))?;

    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/C", &script.to_string_lossy()]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn().map_err(|e| format!("start installer: {}", e))?;
    Ok(())
}
