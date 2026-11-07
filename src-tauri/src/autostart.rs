use super::*;

// ---------- Background / autostart ----------

// Task Manager keeps each entry's on/off state here, next to (not inside) the Run key.
#[cfg(windows)]
const STARTUP_APPROVED_RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

// What Task Manager reads as "enabled": flag 0x02, no change time.
#[cfg(windows)]
const STARTUP_APPROVED_ENABLED: [u8; 12] = [0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

// Task Manager's stored state for our entry. A missing key or value is None.
#[cfg(windows)]
pub(crate) fn startup_approved_bytes() -> Result<Option<Vec<u8>>, String> {
    use crate::registry::{Hive, Key};
    let key = match Key::open_read(Hive::CurrentUser, STARTUP_APPROVED_RUN) {
        Ok(k) => k,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    match key.get_binary(AUTOSTART_KEY) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

// True when Task Manager's switch for our entry is off. Unreadable counts as on.
#[cfg(windows)]
pub(crate) fn task_manager_disabled() -> bool {
    startup_approved_disabled(startup_approved_bytes().ok().flatten().as_deref())
}

// Removes a value from Task Manager's state key; a missing key or value is fine.
#[cfg(windows)]
fn delete_startup_approved(name: &str) -> Result<(), String> {
    use crate::registry::{Hive, Key};
    match Key::open_write(Hive::CurrentUser, STARTUP_APPROVED_RUN) {
        Ok(key) => match key.delete_value(name) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// Add/remove the app in the per-user Windows autostart registry key. `approve` also marks the
// entry enabled for Task Manager (switching on in the app re-enables one it had disabled);
// changing only the start-minimized flag passes false and leaves that choice alone.
#[cfg(windows)]
pub(crate) fn apply_autostart(enabled: bool, minimized: bool, approve: bool) -> Result<(), String> {
    use crate::registry::{Hive, Key};
    let run = Key::open_write(Hive::CurrentUser, r"Software\Microsoft\Windows\CurrentVersion\Run")
        .map_err(|e| e.to_string())?;
    // A value an earlier name of this product registered would keep launching the
    // app twice; drop it whichever way the setting goes.
    if !brand::LEGACY_AUTOSTART_KEY.is_empty() {
        let _ = run.delete_value(brand::LEGACY_AUTOSTART_KEY);
        let _ = delete_startup_approved(brand::LEGACY_AUTOSTART_KEY);
    }
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let flag = if minimized { " --minimized" } else { "" };
        run.set_string(AUTOSTART_KEY, &format!("\"{}\"{}", exe.display(), flag))
            .map_err(|e| e.to_string())?;
        if approve {
            Key::create_write(Hive::CurrentUser, STARTUP_APPROVED_RUN)
                .and_then(|key| key.set_binary(AUTOSTART_KEY, &STARTUP_APPROVED_ENABLED))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    } else {
        match run.delete_value(AUTOSTART_KEY) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        delete_startup_approved(AUTOSTART_KEY)
    }
}


#[tauri::command(async)]
pub(crate) fn set_minimize_on_close(app: AppHandle, state: State<Db>, enabled: bool) {
    let mut store = lock(&state);
    store.settings.minimize_to_tray = enabled;
    save_settings(&app, &store.settings);
}

#[tauri::command(async)]
pub(crate) fn set_always_on_top(app: AppHandle, state: State<Db>, enabled: bool) {
    {
        let mut store = lock(&state);
        store.settings.always_on_top = enabled;
        save_settings(&app, &store.settings);
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.set_always_on_top(enabled);
    }
}

// The run entry as it stands right now (None = no entry), so a failed change can
// put back exactly what was there.
#[cfg(windows)]
pub(crate) fn autostart_command() -> Result<Option<String>, String> {
    use crate::registry::{Hive, Key};
    let run = Key::open_read(Hive::CurrentUser, r"Software\Microsoft\Windows\CurrentVersion\Run")
        .map_err(|e| e.to_string())?;
    match run.get_string(AUTOSTART_KEY) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}


#[cfg(windows)]
pub(crate) fn autostart_restore(prior: Option<String>) -> Result<(), String> {
    use crate::registry::{Hive, Key};
    let run = Key::open_write(Hive::CurrentUser, r"Software\Microsoft\Windows\CurrentVersion\Run")
        .map_err(|e| e.to_string())?;
    match prior {
        Some(cmd) => run.set_string(AUTOSTART_KEY, &cmd).map_err(|e| e.to_string()),
        None => match run.delete_value(AUTOSTART_KEY) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        },
    }
}


// One stored setting and one piece of Windows state that must agree. The setting is saved
// first, the Windows side changed only once that landed. If either step fails, whichever side
// changed is put back and the caller reports an error (the control then reverts).
// `restore_setting` gets `false` when nothing was written, so it only undoes the in-memory change.
pub(crate) fn two_state_change<C, P>(
    ctx: &mut C,
    prior_os: P,
    set_setting: impl FnOnce(&mut C) -> bool,
    restore_setting: impl FnOnce(&mut C, bool) -> bool,
    apply_os: impl FnOnce() -> Result<(), String>,
    restore_os: impl FnOnce(P) -> Result<(), String>,
) -> Result<(), String> {
    if !set_setting(ctx) {
        restore_setting(ctx, false);
        return Err("save-failed".to_string());
    }
    if let Err(e) = apply_os() {
        if let Err(r) = restore_os(prior_os) {
            append_crash_log(&format!("windows state could not be put back after a failed change: {r}"));
        }
        if !restore_setting(ctx, true) {
            append_crash_log("the previous setting could not be saved again after a failed change");
        }
        return Err(e);
    }
    Ok(())
}

// Whose registration an entry is. Only our own (or one whose program is gone)
// is ever rewritten or removed at start; another copy's entry is left alone.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum RegistryOwner {
    Missing,
    ThisExe,
    Gone,
    OtherExe,
}

// The program a registry command line starts: quoted path, or up to the first space.
pub(crate) fn command_program(command: &str) -> Option<String> {
    let c = command.trim();
    let exe = match c.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?,
        None => c.split(' ').next()?,
    };
    (!exe.is_empty()).then(|| exe.to_string())
}

pub(crate) fn same_program_path(a: &str, b: &str) -> bool {
    fold_path(a) == fold_path(b)
}

pub(crate) fn registry_owner(command: Option<&str>, this_exe: &str, exists: impl Fn(&str) -> bool) -> RegistryOwner {
    let Some(program) = command.and_then(command_program) else {
        return RegistryOwner::Missing;
    };
    if same_program_path(&program, this_exe) {
        RegistryOwner::ThisExe
    } else if !exists(&program) {
        RegistryOwner::Gone
    } else {
        RegistryOwner::OtherExe
    }
}

// What start-up does about the autostart entry.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum BootAutostart {
    // Refresh the Run value.
    Write,
    // Task Manager disabled the entry: the stored setting turns off, the Run value stays.
    TurnSettingOff,
    Nothing,
}

// Start-up only ever writes or refreshes an entry whose setting is on, and only
// when it is missing or points at this exe or one that no longer exists, and Task Manager
// has not disabled it (then the setting turns off and the entry stays). It never
// removes anything: a restart into default settings (demo mode, a missing or
// unreadable settings row) must not delete the user's real registrations, and
// removal belongs to the switches. Demo mode skips the whole pass.
pub(crate) fn boot_reconcile_writes(setting_on: bool, demo: bool, owner: RegistryOwner) -> bool {
    setting_on && !demo && owner != RegistryOwner::OtherExe
}

pub(crate) fn boot_reconcile_action(setting_on: bool, demo: bool, owner: RegistryOwner, tm_disabled: bool) -> BootAutostart {
    if !boot_reconcile_writes(setting_on, demo, owner) {
        BootAutostart::Nothing
    } else if tm_disabled {
        BootAutostart::TurnSettingOff
    } else {
        BootAutostart::Write
    }
}

// Whether the StartupApproved bytes Task Manager keeps per entry say "disabled": the low bit
// of the first byte (0x02 / 0x06 enabled, 0x03 / 0x07 disabled). No value counts as enabled.
pub(crate) fn startup_approved_disabled(bytes: Option<&[u8]>) -> bool {
    bytes.and_then(|b| b.first()).is_some_and(|b| b & 1 == 1)
}

// Turning a switch off must not remove an entry another copy (portable versus
// installed) registered; the setting still turns off.
pub(crate) fn skip_registry_removal(enabled: bool, owner: RegistryOwner) -> bool {
    !enabled && owner == RegistryOwner::OtherExe
}

// Changing "start minimized" rewrites the autostart entry only when this copy's setting
// is on AND the entry is not another copy's: repointing it would silently move the
// user's autostart to this exe (portable beside installed).
pub(crate) fn start_minimized_rewrites_entry(autostart_on: bool, owner: RegistryOwner) -> bool {
    autostart_on && owner != RegistryOwner::OtherExe
}

// A reset turns autostart off like the switch does, so it leaves another copy's
// Run entry alone too.
pub(crate) fn autostart_removal_skipped() -> bool {
    match autostart_command() {
        Ok(cmd) => skip_registry_removal(false, current_registry_owner(cmd.as_deref())),
        Err(_) => false,
    }
}

pub(crate) fn current_registry_owner(command: Option<&str>) -> RegistryOwner {
    match std::env::current_exe() {
        Ok(exe) => registry_owner(command, &exe.display().to_string(), |p| std::path::Path::new(p).exists()),
        Err(_) => RegistryOwner::Missing,
    }
}

// Start-up: refresh the autostart registration that the stored settings ask for. Returns what
// was decided; the caller turns the stored setting off for `TurnSettingOff`.
pub(crate) fn reconcile_windows_state(autostart_on: bool, start_min: bool, demo: bool) -> BootAutostart {
    match autostart_command() {
        Ok(cmd) => {
            let action = boot_reconcile_action(
                autostart_on,
                demo,
                current_registry_owner(cmd.as_deref()),
                task_manager_disabled(),
            );
            if action == BootAutostart::Write {
                if let Err(e) = apply_autostart(true, start_min, true) {
                    append_crash_log(&format!("autostart reconcile failed: {e}"));
                }
            }
            action
        }
        Err(e) => {
            append_crash_log(&format!("autostart reconcile could not read the entry: {e}"));
            BootAutostart::Nothing
        }
    }
}

// How the stored setting must change so it agrees with what Windows backs: our own Run
// entry exists and Task Manager has not disabled it.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum EffectiveSync {
    Keep,
    // Windows no longer backs a stored "on": only the setting turns off.
    SettingOff,
    // The entry was enabled again in Task Manager: the setting turns on and the Run value is
    // rewritten, so a moved exe path and the start-minimized flag are current again.
    SettingOnAndRefresh,
}

pub(crate) fn effective_sync(owner: RegistryOwner, tm_disabled: bool, setting_on: bool) -> EffectiveSync {
    let backed = owner == RegistryOwner::ThisExe && !tm_disabled;
    match (backed, setting_on) {
        (false, true) => EffectiveSync::SettingOff,
        (true, false) => EffectiveSync::SettingOnAndRefresh,
        _ => EffectiveSync::Keep,
    }
}

// What the settings switch should show: our own Run entry exists and Task Manager has not
// disabled it. A stored "on" that Windows no longer backs is turned off here, touching only
// the setting (the registry stays as it is, so the entry can be enabled again in Task Manager);
// an entry enabled again there turns the setting back on and refreshes the Run value.
#[tauri::command(async)]
pub(crate) fn autostart_effective(app: AppHandle, state: State<Db>) -> Result<bool, String> {
    let cmd = autostart_command()?;
    let owner = current_registry_owner(cmd.as_deref());
    let tm_disabled = task_manager_disabled();
    let effective = owner == RegistryOwner::ThisExe && !tm_disabled;
    let mut store = lock(&state);
    match effective_sync(owner, tm_disabled, store.settings.autostart) {
        EffectiveSync::Keep => {}
        EffectiveSync::SettingOff => {
            store.settings.autostart = false;
            save_settings(&app, &store.settings);
        }
        EffectiveSync::SettingOnAndRefresh => {
            store.settings.autostart = true;
            save_settings(&app, &store.settings);
            if let Err(e) = apply_autostart(true, store.settings.start_minimized, false) {
                append_crash_log(&format!("autostart entry could not be refreshed: {e}"));
            }
        }
    }
    Ok(effective)
}

// ---------- Leftover address-scheme registration ----------
//
// An earlier version registered a per-user URL scheme named `promptsaver`. The feature is
// gone, so start-up takes the registration out again, but only when its open command starts
// this very exe: another program (or another copy of this one) may own the scheme now, and
// that entry is none of our business.
const LEGACY_PROTOCOL_KEY: &str = "Software\\Classes\\promptsaver";

// True when a registered open command starts `this_exe`. A missing, empty or unreadable
// command is never ours.
pub(crate) fn legacy_protocol_points_at_us(command: Option<&str>, this_exe: &str) -> bool {
    command.and_then(command_program).is_some_and(|program| same_program_path(&program, this_exe))
}

// The cleanup with its two registry touches passed in, so every branch is provable without
// the registry. Returns whether the key was removed.
pub(crate) fn remove_legacy_protocol_with(
    this_exe: &str,
    read_command: impl FnOnce() -> Result<Option<String>, String>,
    delete_key: impl FnOnce() -> Result<(), String>,
) -> Result<bool, String> {
    let command = read_command()?;
    if !legacy_protocol_points_at_us(command.as_deref(), this_exe) {
        return Ok(false);
    }
    delete_key()?;
    Ok(true)
}

#[cfg(windows)]
pub(crate) fn remove_legacy_protocol_key() {
    use crate::registry::{delete_tree, Hive, Key};
    let Ok(exe) = std::env::current_exe() else { return };
    let read_command = || {
        match Key::open_read(Hive::CurrentUser, &format!("{LEGACY_PROTOCOL_KEY}\\shell\\open\\command")) {
            Ok(key) => match key.get_string("") {
                Ok(v) => Ok(Some(v)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.to_string()),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    };
    let delete_key = || match delete_tree(Hive::CurrentUser, LEGACY_PROTOCOL_KEY) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    };
    if let Err(e) = remove_legacy_protocol_with(&exe.display().to_string(), read_command, delete_key) {
        append_crash_log(&format!("old address scheme entry could not be removed: {e}"));
    }
}


#[tauri::command(async)]
pub(crate) fn set_autostart(app: AppHandle, state: State<Db>, enabled: bool) -> Result<bool, String> {
    // The store lock stays held across both steps so no other change slips in
    // between the save and the registry write.
    let mut store = lock(&state);
    let prior = store.settings.autostart;
    let minimized = store.settings.start_minimized;
    let prior_os = autostart_command()?;
    let skip = skip_registry_removal(enabled, current_registry_owner(prior_os.as_deref()));
    two_state_change(
        &mut *store,
        prior_os,
        |st| {
            st.settings.autostart = enabled;
            save_settings(&app, &st.settings)
        },
        |st, persist| {
            st.settings.autostart = prior;
            !persist || save_settings(&app, &st.settings)
        },
        || if skip { Ok(()) } else { apply_autostart(enabled, minimized, true) },
        autostart_restore,
    )?;
    Ok(enabled)
}

#[tauri::command(async)]
pub(crate) fn set_start_minimized(app: AppHandle, state: State<Db>, enabled: bool) -> Result<bool, String> {
    let mut store = lock(&state);
    let prior = store.settings.start_minimized;
    let autostart = store.settings.autostart;
    // Only an active autostart entry carries the flag; without one there is no
    // Windows side to change.
    let prior_os = if autostart { autostart_command()? } else { None };
    let rewrite = start_minimized_rewrites_entry(autostart, current_registry_owner(prior_os.as_deref()));
    two_state_change(
        &mut *store,
        prior_os,
        |st| {
            st.settings.start_minimized = enabled;
            save_settings(&app, &st.settings)
        },
        |st, persist| {
            st.settings.start_minimized = prior;
            !persist || save_settings(&app, &st.settings)
        },
        || if rewrite { apply_autostart(true, enabled, false) } else { Ok(()) },
        |prior_os| if rewrite { autostart_restore(prior_os) } else { Ok(()) },
    )?;
    Ok(enabled)
}
