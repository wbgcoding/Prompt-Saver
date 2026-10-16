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

// Latest release tag, installer asset URL and changelog body. None on any
// failure (offline, private repo, rate limit): checks never disturb the app.
pub(crate) fn fetch_latest() -> Option<(String, String, String)> {
    let body = ureq::get(&update_api())
        .set("User-Agent", brand::USER_AGENT)
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .ok()?
        .into_string()
        .ok()?;
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

#[tauri::command(async)]
pub(crate) fn set_auto_update(app: AppHandle, state: State<Db>, enabled: bool) {
    let mut store = lock(&state);
    store.settings.auto_update = enabled;
    store.settings.update_consent = if enabled { UpdateConsent::Yes } else { UpdateConsent::No };
    save_settings(&app, &store.settings);
}

// The first-launch question: remembers the answer and the switch together.
#[tauri::command(async)]
pub(crate) fn set_update_consent(app: AppHandle, state: State<Db>, enabled: bool) {
    let mut store = lock(&state);
    store.settings.auto_update = enabled;
    store.settings.update_consent = if enabled { UpdateConsent::Yes } else { UpdateConsent::No };
    save_settings(&app, &store.settings);
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

// Toggle an expert feature flag (enabled = feature on).
// Returns Err only where switching the feature ON has a hard precondition that
// failed (today: the safety backup prompt-sync takes before it first writes); the
// flag is then left OFF, so the UI and the store never disagree.
// Runs off the main thread: switching the receiver on or wider waits for its
// port to bind, and the activation backup takes as long as the library is large.
// The live effects of a switch changing that do not depend on the store lock: the
// window capture exclusion, the collected error list and a running auto-advance
// session. Shared by the settings window and the command line / receiver, so a
// `settings set` takes effect without a restart there too.
pub(crate) fn apply_flag_effects(app: &AppHandle, key: &str, enabled: bool) {
    // Capture exclusion applies live to every open window. Never hold the store
    // lock across window API calls (deadlock risk). The flag says "allow
    // screenshots", so the exclusion is its OPPOSITE: passing it straight through
    // is what made the switch work backwards.
    if key == ALLOW_SCREENSHOT_KEY {
        apply_capture_exclusion_all(app, !enabled);
    }
    // "Off" means the list is empty, not just paused -- clear whatever is
    // already collected the moment the switch goes off.
    if key == "errorLog" && !enabled {
        ERROR_LOG.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
    // Turning either gate off must not leave a session silently running.
    #[cfg(windows)]
    if !enabled && (key == "rotatingPrompts" || key == "rotateAuto") {
        clipwatch::end_any();
    }
    #[cfg(not(windows))]
    let _ = app;
}

#[tauri::command(async)]
pub(crate) fn set_ui_flag(app: AppHandle, state: State<Db>, key: String, enabled: bool) -> Result<(), String> {
    let is_clip = key == "clipWatcher";
    let is_hotkeys = key == "promptHotkeys";
    let is_tray = key == "trayRecent" || key == "trayFavs" || key == TRAY_CUSTOM_MENU_KEY;
    let is_folders = key == "folders";
    let is_sync_enable = key == "promptSync" && enabled; // Prompt-folders-as-Markdown
    // The receiver's own switch is durable-or-reversible (see remote.rs), so it
    // never takes the generic save-and-ignore path below.
    if key == "remoteApi" {
        let result = remote::set_remote_api_flag(&app, &state, enabled);
        notify_float_settings(&app);
        return result;
    }
    // The certificate lifetime switch changes the certificate with it, in one
    // write: only its own command may set it.
    if key == remote_tls::SHORT_LIVED_KEY {
        return Err("remote-tls-short-lived-has-own-command".to_string());
    }
    // Widening or narrowing the receiver's reach restarts it with the matching
    // protocol and is saved first, like its own switch.
    if key == "remoteNetworkFilter" {
        let result = remote::set_network_flag(&app, &state, enabled);
        notify_float_settings(&app);
        return result;
    }
    // The address scheme is a registry entry as well as a flag: changed together,
    // and an error leaves both as they were.
    if key == "urlScheme" {
        let result = set_url_scheme_flag(&app, &state, enabled);
        notify_float_settings(&app);
        return result;
    }
    // Turning this ON while demo mode is active is exactly the condition
    // enable_demo_mode/disable_demo_mode's own real-instance check exists to
    // prevent (second review, finding 4): a second process that starts once
    // this is allowed would keep resolving the redirect per write like the
    // first, so once either flips it, the other's ordinary writes land in
    // whichever store it does not belong in. Defense in depth alongside that
    // check, not a replacement for it -- this one is a settings read, not
    // proof of anything actually running.
    if key == "allowMultipleInstances" && enabled {
        let store = lock(&state);
        if store.settings.ui_flags.get("demoMode").copied().unwrap_or(false) {
            return Err("demo-mode-active".to_string());
        }
    }
    // The activation work below (a full backup, then one file per prompt) takes
    // as long as the library is large. It runs on a SNAPSHOT taken under a short
    // lock rather than with the lock held, the way spawn_sync_write_all already
    // does: otherwise every copy, every save and every other window waits for
    // the whole thing to finish.
    let clear_ocr_index = key == "ocrIndex" && !enabled;
    let snapshot = {
        let mut store = lock(&state);
        store.settings.ui_flags.insert(key.clone(), enabled);
        save_settings(&app, &store.settings);
        if clear_ocr_index {
            // A running "read existing pictures" pass would write the text straight back.
            if clear_ocr_index_text(&mut store) {
                save_prompts_and_trash(&app, &store);
            }
        }
        is_sync_enable.then(|| (store.prompts.clone(), store.settings.clone()))
    };
    if let Some((prompts, settings)) = snapshot {
        // Switching the feature back off has to happen under its own lock, and
        // must not clobber whatever else was written in the meantime.
        let abort = |app: &AppHandle, state: &State<Db>| {
            let mut store = lock(state);
            store.settings.ui_flags.insert("promptSync".to_string(), false);
            save_settings(app, &store.settings);
        };
        // Safety net before the very first sync run ever touches this library:
        // same shape a manual backup would take, so a bad first sync is always
        // recoverable. Without it there is nothing to fall back on, so a failure
        // here ABORTS the activation instead of syncing unprotected.
        let keep = settings.ui_values.get("backupKeep").copied().unwrap_or(10.0).max(1.0) as usize;
        let gfs = gfs_from_settings(&settings);
        if let Err(e) = do_backup(&app, keep, gfs) {
            append_crash_log(&format!("prompt-sync activation backup failed: {e}"));
            abort(&app, &state);
            return Err(format!("sync-backup-failed: {e}"));
        }
        // Synchronous and immediate on purpose (unlike spawn_sync_write_all
        // below) - this is a one-time, user-initiated activation, not a hot
        // save path, and the point is the folder really exists before this
        // command returns "on". If it does not, the activation failed and has
        // to say so rather than leave a switch on over an empty folder.
        let failed = sync_write_all_impl(&app, &prompts, &settings);
        if failed > 0 {
            abort(&app, &state);
            return Err(format!("sync-write-failed: {failed}"));
        }
        // The poll loop ends itself the moment it next finds the feature off, so
        // switching it back on needs a fresh one - the claim inside start_sync_poll
        // makes this safe even if one somehow were still winding down.
        start_sync_poll(app.clone());
    }
    apply_flag_effects(&app, &key, enabled);
    // The window itself now always runs (started at boot) since the
    // auto-advance session needs it independently of this toggle; on/off just
    // changes what on_clipboard_update does with each event, checked live.
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
    // Switching folders off puts every member back on screen as its own pill (the
    // folders themselves are kept, so switching it on again restores them). Opening
    // and closing windows inline from a command is the documented way to deadlock
    // WebView2, hence the hop onto the runtime.
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
    let restart_port_from = {
        let mut store = lock(&state);
        let old = store.settings.ui_values.get(&key).copied();
        if key == "remotePort" {
            let restart = remote::commit_port_value(&mut store.settings, value, remote::probe_port, |s| save_settings(&app, s))?;
            restart.then_some(old)
        } else {
            store.settings.ui_values.insert(key.clone(), value);
            save_settings(&app, &store.settings);
            None
        }
    };
    if let Some(old) = restart_port_from {
        remote::apply_port_change(&app, &state, old)?;
    }
    if is_tray {
        rebuild_tray(&app); // dragging a slider must not rebuild on every step
    }
    // append_crash_log has no DB access at most of its call sites, so its
    // rotation cap lives in this static instead, kept current from here.
    if is_error_log_cap {
        ERROR_LOG_KEEP_BYTES.store((value.max(1.0) * 1024.0) as u64, Ordering::Relaxed);
    }
    // A new update interval counts from the last check, so the running job picks it
    // up now instead of after its old wait. The job itself still does nothing while
    // Auto update is off.
    if key == "updateCheckH" {
        scheduler::retime("update-check", update_check_interval(&app));
    }
    notify_float_settings(&app);
    Ok(())
}

// Settings that name a folder the backend itself writes into or deletes from.
// A generic setter (this one) or a settings/backup import must never be able
// to point one of these at an arbitrary folder -- each is changed only
// through its own dedicated command (set_screenshot_dir, set_video_export_dir,
// set_recording_dir, set_sync_dir), so a subverted renderer calling the
// generic string setter, or a hostile imported settings file, cannot grant
// itself write or delete authority over a folder of its choosing.
pub(crate) const PROTECTED_FOLDER_KEYS: [&str; 4] = ["screenshotDir", "videoExportDir", "recordingDir", "syncDir"];

pub(crate) fn is_protected_folder_key(key: &str) -> bool {
    PROTECTED_FOLDER_KEYS.contains(&key)
}

// Text rows with their own validated writer (folders, receiver secrets and
// ranges, sealed backup password, vault salt and verifier): the generic string
// setter never writes them.
pub(crate) fn is_generic_text_write_denied(key: &str) -> bool {
    is_machine_text_key(key) || is_vault_key(key)
}

// Set an expert string option (e.g. the copy-feedback font key).
#[tauri::command(async)]
pub(crate) fn set_ui_text(app: AppHandle, state: State<Db>, key: String, value: String) {
    if is_generic_text_write_denied(&key) {
        return;
    }
    {
        let mut store = lock(&state);
        store.settings.ui_texts.insert(key, value);
        save_settings(&app, &store.settings);
    }
    notify_float_settings(&app);
}

// Dedicated setter for the screenshot save folder -- the expert menu's
// "Change" button calls pick_folder for the dialog, then this with its
// result. Kept out of the generic set_ui_text path; see PROTECTED_FOLDER_KEYS.
#[tauri::command]
pub(crate) fn set_screenshot_dir(app: AppHandle, state: State<Db>, dir: String) -> Result<(), String> {
    let value = accept_dir_setting(&dir, Path::new(&default_screenshot_dir()), "screenshot")?;
    let mut store = lock(&state);
    store.settings.ui_texts.insert("screenshotDir".to_string(), value);
    save_settings(&app, &store.settings);
    drop(store);
    notify_float_settings(&app);
    Ok(())
}

// Dedicated setter for the video editor's export folder -- same shape as
// set_screenshot_dir above, and for the same reason. There is no separate
// built-in default folder here (empty already means "next to the source
// file"), so accept_dir_setting's default check is a no-op past the empty
// case above it.
#[tauri::command]
pub(crate) fn set_video_export_dir(app: AppHandle, state: State<Db>, dir: String) -> Result<(), String> {
    let value = accept_dir_setting(&dir, Path::new(""), "videoExport")?;
    let mut store = lock(&state);
    store.settings.ui_texts.insert("videoExportDir".to_string(), value);
    save_settings(&app, &store.settings);
    drop(store);
    notify_float_settings(&app);
    Ok(())
}

// Dedicated setter for the screen-recording save folder -- same shape as
// set_screenshot_dir above, and for the same reason.
#[tauri::command]
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
// preferences stay: the backup password, the receiver's certificate values and
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
    let effects = {
        let mut store = lock(&state);
        let old = store.settings.clone();
        let ocr_was_on = ocr_index_flag_on(&store);
        reset_expert_settings(&mut store.settings);
        save_settings(&app, &store.settings);
        // The reset turned the picture-text index back off: its text goes with it.
        if ocr_was_on && clear_ocr_index_text(&mut store) {
            save_prompts_and_trash(&app, &store);
        }
        remote::reset_side_effects(&old, &store.settings)
    };
    remote::run_reset_side_effects(&app, &effects);
    ERROR_LOG_KEEP_BYTES.store(256 * 1024, Ordering::Relaxed);
    // Cleared flags put the screenshot permission back to its default (off), which
    // means the exclusion is on: re-assert it on every window so it is immediate.
    apply_capture_exclusion_all(&app, true);
    apply_tray_menu(&app); // The tray sections fall back to their defaults too
    notify_float_settings(&app);
}

// Add a version to the skip list: it will not be offered again.
#[tauri::command(async)]
pub(crate) fn skip_version(app: AppHandle, state: State<Db>, version: String) {
    let mut store = lock(&state);
    if !store.settings.skipped_versions.contains(&version) {
        store.settings.skipped_versions.push(version);
        save_settings(&app, &store.settings);
    }
}

// The shipped changelog, baked into the binary at compile time: the "what's
// new" window needs nothing from the network to show the current version.
pub(crate) const EMBEDDED_RELEASE_NOTES: &str = include_str!("../../docs/release-notes.md");
pub(crate) const RELEASE_NOTES_MAX_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Serialize, Clone)]
pub(crate) struct WhatsNewSection {
    pub(crate) version: String,
    // Raw Markdown, including its own "## <version>" heading. The frontend
    // renders it node by node, the same safe path the update-changelog dialog
    // already uses, never through innerHTML.
    pub(crate) markdown: String,
}

#[derive(Serialize, Clone)]
pub(crate) struct WhatsNewResult {
    pub(crate) sections: Vec<WhatsNewSection>,
    // The version boundary the sections were computed from, None for "fresh
    // install" or "current version only". Echoed back to whats_new_refresh so
    // a GitHub refresh reproduces the exact same range.
    pub(crate) since: Option<String>,
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

// The three cases the acceptance test names: unchanged version shows nothing,
// a version jump shows every section strictly after `since` up to and
// including `current`, and an unknown/absent `since` (fresh install, or a
// caller that only ever wants the current entry) shows the current version
// only.
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

// Startup check: what changed since the version this install last showed.
// Marks the current version as seen immediately, regardless of whether the
// caller ever displays the result, matching the toast, which never returns
// once its version is marked seen.
#[tauri::command(async)]
pub(crate) fn whats_new_check(app: AppHandle, state: State<Db>) -> WhatsNewResult {
    let all = parse_release_notes(EMBEDDED_RELEASE_NOTES);
    let mut store = lock(&state);
    let since = (!store.settings.last_seen_version.is_empty()).then(|| store.settings.last_seen_version.clone());
    let sections = whats_new_check_impl(&all, &store.settings.last_seen_version, APP_VERSION);
    store.settings.last_seen_version = APP_VERSION.to_string();
    save_settings(&app, &store.settings);
    WhatsNewResult { sections, since }
}

// About dialog: always just the current version's own section, from the
// shipped copy. Opening it never marks anything as seen.
#[tauri::command]
pub(crate) fn whats_new_current() -> Vec<WhatsNewSection> {
    let all = parse_release_notes(EMBEDDED_RELEASE_NOTES);
    sections_for_update(&all, None, APP_VERSION)
}

// "Update from GitHub": re-fetch release-notes.md and apply the same filter
// the open window used (`since`, exactly as passed to whats_new_check or
// None for the About-triggered view). Only the display changes, never the
// shipped file.
#[tauri::command]
pub(crate) async fn whats_new_refresh(since: Option<String>) -> Result<Vec<WhatsNewSection>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let url = format!("https://raw.githubusercontent.com/{}/main/docs/release-notes.md", brand::REPO);
        let resp = ureq::get(&url)
            .set("User-Agent", brand::USER_AGENT)
            .timeout(std::time::Duration::from_secs(10))
            .call()
            .map_err(|e| format!("download: {}", e))?;
        let mut bytes = Vec::new();
        use std::io::Read;
        resp.into_reader()
            .take(RELEASE_NOTES_MAX_BYTES)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("read: {}", e))?;
        if bytes.len() as u64 >= RELEASE_NOTES_MAX_BYTES {
            return Err("release notes too large / truncated".to_string());
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let all = parse_release_notes(&text);
        Ok(sections_for_update(&all, since.as_deref(), APP_VERSION))
    })
    .await
    .map_err(|e| e.to_string())?
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
    // The URL shape check above only proves the download stays inside our own repo's
    // release path: it says nothing about the version. Re-assert the release tag
    // embedded in that same pinned path is actually newer than what's installed, so
    // an older release reachable at a valid, pinned-repo URL (a stale link, a rolled-back
    // asset) can never be installed as an "update".
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

// The blocking body of `install_update` above, split out so it can run inside
// `spawn_blocking` without an `AppHandle` (not needed here) crossing the
// boundary.
pub(crate) fn install_update_blocking(url: String) -> Result<(), String> {
    let resp = download_agent().get(&url)
        .set("User-Agent", brand::USER_AGENT)
        .call()
        .map_err(|e| format!("download: {}", e))?;
    let mut bytes = Vec::new();
    use std::io::Read;
    resp.into_reader()
        .take(UPDATE_MAX_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("read: {}", e))?;
    // A file that exactly hit the cap was almost certainly truncated: never run a
    // partial installer.
    if bytes.len() as u64 >= UPDATE_MAX_BYTES {
        return Err("installer too large / truncated".to_string());
    }
    // Sanity-check the download is actually a Windows executable (PE "MZ" header)
    // before we ever run it: a corruption/wrong-format guard, not authentication
    // on its own. The real integrity check is the SHA-256 verification directly
    // below.
    if bytes.len() < 2 || &bytes[0..2] != b"MZ" {
        return Err("downloaded file is not a valid installer".to_string());
    }
    // Integrity: the SHA-256 published beside the installer in the release
    // (`<installer>.sha256`, same pinned path) must be present AND match. Fail closed:
    // an installer we cannot authenticate is never executed, so a hash file that is
    // missing, unreadable or malformed aborts the update just like a mismatch does.
    // Every release publishes the hash (see `release:publish` in package.json); if one
    // ever does not, the update is done by hand rather than unverified.
    let want = ureq::get(&format!("{}.sha256", url))
        .set("User-Agent", brand::USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .ok()
        .and_then(|r| r.into_string().ok())
        .map(|t| t.split_whitespace().next().unwrap_or("").to_ascii_lowercase())
        .filter(|w| w.len() == 64 && w.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| "no usable checksum published for this update, download rejected".to_string())?;
    {
        use sha2::{Digest, Sha256};
        if format!("{:x}", Sha256::digest(&bytes)) != want {
            return Err("update hash mismatch: download rejected".to_string());
        }
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

    // Helper script: silent install, relaunch the app, delete the installer + itself.
    let app_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let script = stage.join("update.cmd");
    let content = format!(
        "@echo off\r\n\"{}\" /S\r\nstart \"\" \"{}\"\r\ndel \"{}\"\r\ndel \"%~f0\"\r\n",
        installer.display(),
        app_exe.display(),
        installer.display()
    );
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
