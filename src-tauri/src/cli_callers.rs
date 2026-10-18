use super::*;

// ---------- Standalone caller (nothing is running) ----------

pub(super) fn load_store_headless(dir: &Path) -> Store {
    if let Some(conn) = db_conn_at(dir) {
        let mut written = WrittenRows::new();
        let (mut prompts, settings_opt, _needs_mig, prompt_fails, settings_unreadable) = db_load(&conn, &mut written);
        let (trash, trash_fails) = db_load_trash(&conn, &mut written);
        let decrypt_fails = prompt_fails + trash_fails;
        let mut settings = settings_opt.unwrap_or_default();
        settings.migrate();
        migrate_prompts(&mut prompts);
        Store { prompts, trash, settings, prompts_locked: decrypt_fails > 0 || settings_unreadable, written: std::cell::RefCell::new(written) }
    } else {
        Store { prompts: Vec::new(), trash: Vec::new(), settings: crate::Settings::default(), prompts_locked: true, written: std::cell::RefCell::new(WrittenRows::new()) }
    }
}

pub(crate) fn run_standalone(argv: &[String], cwd: &Path) -> i32 {
    run_standalone_with(data_dir_headless, argv, cwd)
}

// run_standalone with the folder supplied by the caller (asked only once the command needs a
// store), so a test can point it at a folder of its own.
pub(crate) fn run_standalone_with(resolve_dir: impl FnOnce() -> PathBuf, argv: &[String], cwd: &Path) -> i32 {
    let parsed = match parse(argv) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let cmd = match parsed {
        Parsed::Help(text) => {
            println!("{text}");
            return 0;
        }
        Parsed::Run(cmd) => cmd,
    };
    if matches!(cmd, Cmd::Version) {
        print_version();
        return 0;
    }
    if matches!(cmd, Cmd::Describe) {
        println!("{}", describe_json());
        return 0;
    }
    let dir = resolve_dir();
    let store = load_store_headless(&dir);
    acquire_data_dir_lock(&dir, data_dir_lock_enabled(&store.settings));

    // Before the restore arm: a restore replaces the whole database, so a store another machine
    // holds locked must be refused for it like for every other change.
    if is_mutating(&cmd) && crate::DATA_DIR_READONLY.load(Ordering::Relaxed) {
        eprintln!("the data folder is locked by another machine right now\n");
        return 3;
    }

    if let Cmd::BackupRestore { name, keep, scope, confirmed } = &cmd {
        if !confirmed {
            eprintln!("refused: {}. Pass --yes to actually do it.", dry_run_description(&cmd));
            return 4;
        }
        let keep = keep.unwrap_or_else(|| default_keep(&store.settings));
        return match restore_backup_standalone(&dir, name, keep, scope, &store.settings) {
            Ok(()) => {
                println!("restored backup {name}");
                0
            }
            Err(e) => {
                eprintln!("{e}");
                map_restore_err_code(&e)
            }
        };
    }

    if is_mutating(&cmd) && store.prompts_locked {
        eprintln!("the store has rows that failed to decrypt; writes are refused until that is resolved\n");
        return 3;
    }

    let mut store = store;
    // What an autostart change has to put back if the Windows entry cannot follow.
    let prior_autostart = store.settings.autostart;
    let prior_entry = match os_backed_toggle(&cmd) {
        Some(("autostart", Ok(_))) => match crate::autostart_command() {
            Ok(entry) => Some(entry),
            Err(e) => {
                eprintln!("the Windows entry could not be read: {e}\n");
                return 3;
            }
        },
        _ => None,
    };
    let outcome = exec(&mut store, &dir, &cmd, cwd);
    if outcome.mutated_prompts || outcome.mutated_trash || outcome.mutated_settings {
        let Some(mut conn) = db_conn_at(&dir) else {
            eprintln!("could not open the store to save the change\n");
            return 3;
        };
        // Reuses the map load_store_headless() populated from the table, not an empty one:
        // write_rows deletes a row only if it is missing from the new list and already known to this
        // map, so a process that never saw a row does not erase it. Starting empty would make every
        // deletion a no-op on disk.
        let write_ok = if outcome.mutated_trash {
            db_write_prompts_and_trash(&mut conn, &store.prompts, &store.trash, &mut store.written.borrow_mut()).is_ok()
        } else if outcome.mutated_prompts {
            db_write_prompts(&mut conn, &store.prompts, &mut store.written.borrow_mut()).is_ok()
        } else {
            true
        };
        let settings_ok = if outcome.mutated_settings { db_write_settings(&conn, &store.settings).is_ok() } else { true };
        if !write_ok || !settings_ok {
            eprintln!("the write did not reach the database\n");
            return 3;
        }
        // Nothing is running to do it, so the Run entry is brought in line here.
        if let Some(("autostart", Ok(on))) = os_backed_toggle(&cmd) {
            if outcome.code == 0 {
                // Same gate as the app's switch: turning it off leaves another copy's entry alone.
                let skip = prior_entry
                    .as_ref()
                    .map(|entry| crate::skip_registry_removal(on, crate::current_registry_owner(entry.as_deref())))
                    .unwrap_or(false);
                let applied = if skip { Ok(()) } else { crate::apply_autostart(on, store.settings.start_minimized) };
                if let Err(e) = applied {
                    // Same as the app's own switch: neither side keeps the change.
                    if let Some(entry) = prior_entry {
                        let _ = crate::autostart_restore(entry);
                    }
                    store.settings.autostart = prior_autostart;
                    let restored = db_write_settings(&conn, &store.settings).is_ok();
                    eprintln!(
                        "the Windows entry could not be changed{}: {e}\n",
                        if restored { "" } else { " and the setting could not be put back" }
                    );
                    return 3;
                }
            }
        }
    }
    if !outcome.stdout.is_empty() {
        print!("{}", outcome.stdout);
    }
    if !outcome.stderr.is_empty() {
        eprint!("{}", outcome.stderr);
    }
    outcome.code
}

pub(super) fn print_version() {
    println!("{} {}", crate::brand::DISPLAY_NAME, env!("CARGO_PKG_VERSION"));
}

// ---------- Already-running caller ----------

pub(crate) async fn run_in_process(app: &AppHandle, argv: Vec<String>, cwd: PathBuf) -> (i32, String, String) {
    let (code, stdout, stderr, restart) = run_in_process_deferred(app, argv, cwd).await;
    if restart {
        app.restart();
    }
    (code, stdout, stderr)
}

// Same as run_in_process, but a restore does not relaunch the app itself: the last element
// says a relaunch is due and the caller does it once its answer is written. A relaunch inside
// the call drops the connection before any answer goes out, which a script reads as a failed
// restore.
pub(crate) async fn run_in_process_deferred(app: &AppHandle, argv: Vec<String>, cwd: PathBuf) -> (i32, String, String, bool) {
    let mut restart = false;
    let (code, stdout, stderr) = run_in_process_inner(app, argv, cwd, &mut restart).await;
    (code, stdout, stderr, restart)
}

// Runs `after` once `answer` has finished: the answer must be on the wire before the relaunch
// tears the connection down.
pub(crate) async fn answer_then_restart(answer: impl std::future::Future<Output = ()>, restart_due: bool, restart: impl FnOnce()) {
    answer.await;
    if restart_due {
        restart();
    }
}

// A confirmed `settings set autostart|alwaysOnTop` with its value read as a switch.
pub(super) fn os_backed_toggle(cmd: &Cmd) -> Option<(&'static str, Result<bool, ()>)> {
    let Cmd::SettingsSet { key, value, confirmed: true } = cmd else { return None };
    let key = match key.as_str() {
        "autostart" => "autostart",
        "alwaysOnTop" => "alwaysOnTop",
        _ => return None,
    };
    Some((key, value.parse::<bool>().map_err(|_| ())))
}

pub(super) async fn run_in_process_inner(app: &AppHandle, argv: Vec<String>, cwd: PathBuf, restart: &mut bool) -> (i32, String, String) {
    let parsed = match parse(&argv) {
        Ok(p) => p,
        Err(e) => return (1, String::new(), format!("{e}\n")),
    };
    let cmd = match parsed {
        Parsed::Help(text) => return (0, format!("{text}\n"), String::new()),
        Parsed::Run(cmd) => cmd,
    };
    if matches!(cmd, Cmd::Version) {
        return (0, format!("{} {}\n", crate::brand::DISPLAY_NAME, env!("CARGO_PKG_VERSION")), String::new());
    }
    if matches!(cmd, Cmd::Describe) {
        return (0, format!("{}\n", describe_json()), String::new());
    }

    // Backup restore reuses the real, tested restore end to end (safety snapshot, staged swap)
    // instead of a second implementation that would have to fake what only a running app can do;
    // the relaunch is left to the caller (see run_in_process_deferred).
    if let Cmd::BackupRestore { name, keep, scope, confirmed } = &cmd {
        if !confirmed {
            return (4, String::new(), format!("refused: {}. Pass --yes to actually do it.\n", dry_run_description(&cmd)));
        }
        let (worker, name_c, scope_c, keep_c) = (app.clone(), name.clone(), scope.clone(), *keep);
        // Off the async runtime: the store-lock wait, the safety snapshot and the inflate block.
        let outcome = tauri::async_runtime::spawn_blocking(move || {
            let state: tauri::State<Db> = worker.state();
            let keep = keep_c.unwrap_or_else(|| default_keep(&lock(&state).settings));
            crate::restore_backup_core(worker.clone(), state, name_c, keep, scope_c)
        })
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
        return match outcome {
            Ok(()) => {
                *restart = true;
                (0, format!("restored backup {name}\n"), String::new())
            }
            Err(e) => {
                let code = if e == "missing" { 2 } else { 3 };
                (code, String::new(), format!("{e}\n"))
            }
        };
    }

    // Autostart and always-on-top live partly outside the store (the Run entry, the
    // window flag): they go through the app's own handlers, which change both.
    if let Some((key, parsed)) = os_backed_toggle(&cmd) {
        let Ok(on) = parsed else {
            return (1, String::new(), format!("{key} takes true or false\n"));
        };
        if crate::DATA_DIR_READONLY.load(Ordering::Relaxed) || crate::SETTINGS_LOCKED.load(Ordering::Relaxed) {
            return (3, String::new(), "the store refused the write (locked or unavailable)\n".to_string());
        }
        let state: tauri::State<Db> = app.state();
        let done = if key == "autostart" {
            crate::set_autostart(app.clone(), state, on).map(|_| ())
        } else {
            crate::set_always_on_top(app.clone(), state, on);
            Ok(())
        };
        return match done {
            Ok(()) => {
                let _ = tauri::Emitter::emit_to(app, "main", "store-synced", ());
                (0, String::new(), String::new())
            }
            Err(e) => (3, String::new(), format!("{e}\n")),
        };
    }

    let state: tauri::State<Db> = app.state();
    let dir = data_dir(app);
    let mut store = lock(&state);
    // The same refusal the standalone caller gives, up front: a store that is read-only, locked
    // after a failed decrypt or settings-locked must not even be changed in memory, or the next
    // save that succeeds would write it. (This also refuses a prompt-only command while only the
    // settings are locked; the standalone caller does not look at SETTINGS_LOCKED.)
    if is_mutating(&cmd)
        && (crate::DATA_DIR_READONLY.load(Ordering::Relaxed) || store.prompts_locked || crate::SETTINGS_LOCKED.load(Ordering::Relaxed))
    {
        return (3, String::new(), "the store refused the write (locked or unavailable)
".to_string());
    }
    let outcome = exec_persisted(&mut store, &dir, &cmd, &cwd, |st, o| {
        let persisted = if o.mutated_trash {
            crate::save_prompts_and_trash(app, st)
        } else if o.mutated_prompts {
            crate::save_prompts(app, st)
        } else {
            true
        };
        let settings_persisted = if o.mutated_settings { crate::save_settings(app, &st.settings) } else { true };
        persisted && settings_persisted
    });
    // A switch set from here needs the same live effects the settings window gives it
    // (capture exclusion, the error list, a running auto-advance session).
    let flag_set = match &cmd {
        Cmd::SettingsSet { key, .. } if outcome.code == 0 && outcome.mutated_settings => {
            store.settings.ui_flags.get(key.as_str()).map(|on| (key.clone(), *on))
        }
        _ => None,
    };
    drop(store);
    if let Some((key, on)) = flag_set {
        crate::apply_flag_effects(app, &key, on);
    }
    if outcome.code == 0 && (outcome.mutated_prompts || outcome.mutated_trash || outcome.mutated_settings) {
        // The windows, armed hotkeys and tray still hold what they read before this command: tell
        // them, or a stale grid keeps a deleted tile and its next layout write restores the cell.
        let _ = tauri::Emitter::emit_to(app, "main", "store-synced", ());
        crate::rearm_hotkeys(app);
        crate::rebuild_tray(app);
        crate::sync_float_windows(app);
        crate::notify_float_settings(app);
    }
    (outcome.code, outcome.stdout, outcome.stderr)
}

// Runs a command on the live store and keeps its change only when `persist` accepts it. A
// refused write puts prompts, trash and settings back as they were, so the caller's
// "refused" answer is true of the running app too.
pub(super) fn exec_persisted(store: &mut Store, dir: &Path, cmd: &Cmd, cwd: &Path, persist: impl FnOnce(&Store, &Outcome) -> bool) -> Outcome {
    // Only a command that can change something needs the copy; a read over a
    // large library stays free of it.
    let before = is_mutating(cmd).then(|| (store.prompts.clone(), store.trash.clone(), store.settings.clone()));
    let outcome = exec(store, dir, cmd, cwd);
    if !(outcome.mutated_prompts || outcome.mutated_trash || outcome.mutated_settings) {
        return outcome;
    }
    if persist(store, &outcome) {
        return outcome;
    }
    if let Some(before) = before {
        (store.prompts, store.trash, store.settings) = before;
    }
    // If the prompts write went through and only the settings write failed, memory is back but
    // the disk keeps the prompt change until the next save, which rewrites everything. The row
    // bookkeeping may have moved with the failed write, so start it over to avoid skipping rows
    // as already written.
    store.forget_written_rows();
    Outcome::err(3, "the store refused the write (locked or unavailable)\n")
}

// ---------- Remote-caller path confinement ----------
//
// A command line launch resolves `export`/`import` against the caller's own working directory
// because a trusted local user types it. The remote receiver and a promptsaver:// link have
// no such trusted caller, so the same two commands are confined to the data folder there.
// It is parsed here, in the module that owns `Cmd`/`parse`, rather than duplicating the
// grammar elsewhere.
pub(crate) fn confine_path_arg(argv: &[String], base_dir: &Path) -> Result<(), String> {
    let cmd = match parse(argv) {
        Ok(Parsed::Run(cmd)) => cmd,
        // A bad or --help invocation is reported by the real dispatch right after this call.
        _ => return Ok(()),
    };
    let raw = match &cmd {
        Cmd::Export { path, .. } | Cmd::Import { path, .. } => path.clone(),
        _ => return Ok(()),
    };
    let resolved = resolve_path_arg(base_dir, &raw);
    // Export's target usually does not exist yet, so its parent directory is confined instead,
    // as a shell resolves a not-yet-created path. An import's source must already exist, so this
    // covers it too.
    let check_dir = if resolved.exists() {
        resolved.clone()
    } else {
        match resolved.parent() {
            Some(p) => p.to_path_buf(),
            None => return Err("refused: the path escapes the data folder".to_string()),
        }
    };
    let canon_target = check_dir
        .canonicalize()
        .map_err(|_| "refused: the path does not resolve inside the data folder".to_string())?;
    let canon_base = base_dir
        .canonicalize()
        .map_err(|_| "the data folder could not be resolved".to_string())?;
    if !canon_target.starts_with(&canon_base) {
        return Err("refused: the path escapes the data folder".to_string());
    }
    // The data folder also holds the live store and the snapshots, which an export would
    // overwrite with JSON and no safety copy: only the folder's other files are valid targets.
    if matches!(cmd, Cmd::Export { .. }) {
        let target = if resolved.exists() {
            canon_target
        } else {
            match resolved.file_name() {
                Some(name) => canon_target.join(name),
                None => return Err("refused: the path has no file name".to_string()),
            }
        };
        if is_store_path(&target, &canon_base) {
            return Err("refused: that path belongs to the stored data".to_string());
        }
        if is_program_file_name(&target) {
            return Err("refused: that path belongs to the stored data".to_string());
        }
    }
    Ok(())
}

// True for a name an export must never replace: in a portable install the data folder is the
// program folder, and JSON written over an executable, a library, the uninstaller or the
// data-folder redirect file would break the next launch.
fn is_program_file_name(target: &Path) -> bool {
    let name = target.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let ext = target.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    name == "datapath" || name == "uninstall.exe" || matches!(ext.as_str(), "exe" | "dll" | "com" | "scr" | "msi" | "bat" | "cmd")
}

// True for a path that is part of the stored data inside the data folder: the database with
// its write-ahead files, the legacy json files, the folder lock, and anything under the
// snapshots and attachment folders. Both arguments are canonical; names compare
// case-insensitively (Windows).
pub(super) fn is_store_path(target: &Path, base: &Path) -> bool {
    let Ok(rel) = target.strip_prefix(base) else {
        return false;
    };
    let mut parts = rel.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase());
    let Some(first) = parts.next() else {
        return true; // the folder itself
    };
    if parts.next().is_some() {
        return matches!(first.as_str(), "backups" | "files");
    }
    matches!(first.as_str(), "backups" | "files" | "data.db-wal" | "data.db-shm")
        || crate::STORE_FILES.iter().any(|f| f.eq_ignore_ascii_case(&first))
        || first.eq_ignore_ascii_case(crate::DATA_DIR_LOCK_FILE)
}
