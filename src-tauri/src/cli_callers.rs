use super::*;

// ---------- Output ----------

// print! and eprint! panic when the reader of a pipe has gone (`list | head -1`, a parent that
// stopped reading): the process then dies with a panic message and exit 101, often after a change
// was already committed. A reader that left is not an error of the command, so a failed write is
// dropped and the real exit code stands.
pub(super) fn write_ignoring_closed_pipe(w: &mut impl std::io::Write, args: std::fmt::Arguments) {
    let _ = w.write_fmt(args);
    let _ = w.flush();
}

pub(super) fn out(args: std::fmt::Arguments) {
    write_ignoring_closed_pipe(&mut std::io::stdout().lock(), args);
}

pub(super) fn err(args: std::fmt::Arguments) {
    write_ignoring_closed_pipe(&mut std::io::stderr().lock(), args);
}

pub(super) fn outln(args: std::fmt::Arguments) {
    out(format_args!("{args}\n"));
}

pub(super) fn errln(args: std::fmt::Arguments) {
    err(format_args!("{args}\n"));
}

// ---------- Standalone caller (nothing is running) ----------

// How much of the store a headless load could actually read. A store that is not fully readable
// must not answer a read with what is left of it: an empty list would read as "the library is
// empty" and a default setting as the user's choice.
pub(super) struct StoreHealth {
    pub(super) settings_ok: bool,
    // Rows that failed to decrypt, or a database that did not open. An unreadable settings row
    // alone is not this: the prompt reads still answer faithfully then, as in the app.
    pub(super) rows_unreadable: bool,
}

pub(super) fn load_store_headless(dir: &Path) -> Store {
    load_store_headless_checked(dir).0
}

pub(super) fn load_store_headless_checked(dir: &Path) -> (Store, StoreHealth) {
    if let Some(conn) = db_conn_at(dir) {
        let mut written = WrittenRows::new();
        let (mut prompts, settings_opt, _needs_mig, prompt_fails, settings_unreadable) = db_load(&conn, &mut written);
        let (trash, trash_fails) = db_load_trash(&conn, &mut written);
        let decrypt_fails = prompt_fails + trash_fails;
        let mut settings = settings_opt.unwrap_or_default();
        settings.migrate();
        migrate_prompts(&mut prompts);
        let store = Store { prompts, trash, settings, prompts_locked: decrypt_fails > 0 || settings_unreadable, written: std::cell::RefCell::new(written) };
        (store, StoreHealth { settings_ok: !settings_unreadable, rows_unreadable: decrypt_fails > 0 })
    } else {
        let store = Store { prompts: Vec::new(), trash: Vec::new(), settings: crate::Settings::default(), prompts_locked: true, written: std::cell::RefCell::new(WrittenRows::new()) };
        (store, StoreHealth { settings_ok: false, rows_unreadable: true })
    }
}

// The answer for a read the store cannot give faithfully (exit 3, like a refused write), or None
// when the read may go ahead. `prompts_locked` here means rows that failed to decrypt or a database
// that did not open (not the settings row); `settings_ok` is false when the settings row is unreadable.
pub(super) fn unreadable_store_message(cmd: &Cmd, prompts_locked: bool, settings_ok: bool) -> Option<&'static str> {
    let unreadable = match cmd {
        Cmd::List { .. } | Cmd::Get { .. } | Cmd::Copy { .. } | Cmd::TagList { .. } | Cmd::ViewList { .. } | Cmd::Search { .. } | Cmd::Export { .. } => {
            prompts_locked
        }
        Cmd::SettingsGet { .. } => !settings_ok,
        _ => false,
    };
    unreadable.then_some("the store could not be read completely (the database did not open or a row failed to decrypt)\n")
}

// Gives back the data-folder claim a standalone run made, so the folder is free for the app or
// another machine the moment the command is done. Only a lock that still carries this
// process's claim id is removed.
struct OwnedLockRelease(PathBuf);

impl Drop for OwnedLockRelease {
    fn drop(&mut self) {
        if !crate::DATA_DIR_LOCK_OWNED.load(Ordering::Relaxed) {
            return;
        }
        let path = crate::store::data_dir_lock_path(&self.0);
        if crate::store::read_data_dir_lock(&path).is_some_and(|l| l.id == crate::store::lock_claim_id()) {
            let _ = std::fs::remove_file(&path);
            // A run that took over a dead claim never lives to the second beat that clears the
            // takeover markers, so they would stay in the data folder for good.
            crate::store::remove_takeover_markers(&path);
        }
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
            errln(format_args!("{e}"));
            return 1;
        }
    };
    let cmd = match parsed {
        Parsed::Help(text) => {
            outln(format_args!("{text}"));
            return 0;
        }
        Parsed::Run(cmd) => cmd,
    };
    if matches!(cmd, Cmd::Version) {
        print_version();
        return 0;
    }
    if matches!(cmd, Cmd::Describe) {
        outln(format_args!("{}", describe_json()));
        return 0;
    }
    let dir = resolve_dir();
    let (store, health) = load_store_headless_checked(&dir);
    // Only a command that writes claims the folder. A read claimed it too and left a lock naming a
    // process that was gone a moment later, which held the app's next start read-only on this
    // machine until the stale limit ran out. A claim this run made is given back when it ends.
    let _lock_release = writes_data_folder(&cmd).then(|| {
        acquire_data_dir_lock(&dir, data_dir_lock_enabled(&store.settings));
        OwnedLockRelease(dir.clone())
    });

    // Before the restore arm: a restore replaces the whole database, so a store another machine
    // holds locked must be refused for it like for every other change.
    if writes_data_folder(&cmd) && crate::DATA_DIR_READONLY.load(Ordering::Relaxed) {
        errln(format_args!("the data folder is locked by another machine right now\n"));
        return 3;
    }

    if let Cmd::BackupRestore { name, keep, scope, confirmed } = &cmd {
        if !confirmed {
            errln(format_args!("refused: {}. Pass --yes to actually do it.", dry_run_description(&cmd)));
            return 4;
        }
        let keep = keep.unwrap_or_else(|| default_keep(&store.settings));
        return match restore_backup_standalone(&dir, name, keep, scope, &store.settings) {
            Ok(()) => {
                outln(format_args!("restored backup {name}"));
                0
            }
            Err(e) => {
                errln(format_args!("{e}"));
                map_restore_err_code(&e)
            }
        };
    }

    if is_mutating(&cmd) && store.prompts_locked {
        errln(format_args!("the store has rows that failed to decrypt; writes are refused until that is resolved\n"));
        return 3;
    }
    if let Some(msg) = unreadable_store_message(&cmd, health.rows_unreadable, health.settings_ok) {
        err(format_args!("{msg}"));
        return 3;
    }

    let mut store = store;
    // What an autostart change has to put back if the Windows entry cannot follow.
    let prior_autostart = store.settings.autostart;
    let prior_entry = match os_backed_toggle(&cmd) {
        Some(("autostart", Ok(_))) => match crate::autostart_command() {
            Ok(entry) => Some(entry),
            Err(e) => {
                errln(format_args!("the Windows entry could not be read: {e}\n"));
                return 3;
            }
        },
        _ => None,
    };
    let outcome = exec(&mut store, &dir, &cmd, cwd);
    if outcome.mutated_prompts || outcome.mutated_trash || outcome.mutated_settings {
        let Some(mut conn) = db_conn_at(&dir) else {
            errln(format_args!("could not open the store to save the change\n"));
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
            errln(format_args!("the write did not reach the database\n"));
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
                    errln(format_args!(
                        "the Windows entry could not be changed{}: {e}\n",
                        if restored { "" } else { " and the setting could not be put back" }
                    ));
                    return 3;
                }
            }
        }
    }
    if !outcome.stdout.is_empty() {
        out(format_args!("{}", outcome.stdout));
    }
    if !outcome.stderr.is_empty() {
        err(format_args!("{}", outcome.stderr));
    }
    outcome.code
}

pub(super) fn print_version() {
    outln(format_args!("{} {}", crate::brand::DISPLAY_NAME, env!("CARGO_PKG_VERSION")));
}

// ---------- Already-running caller ----------

// Runs a forwarded command on the live store. A restore does not relaunch the app itself: the
// last element says a relaunch is due and the caller does it once its answer is written. A
// relaunch inside the call drops the connection before any answer goes out, which a script
// reads as a failed restore.
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
            return (4, String::new(), format!("{key} takes true or false\n"));
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

    // The store lock, the command and its save all block (file reads, VACUUM, compression), so the
    // whole run happens on a blocking worker, never on an async one.
    let worker = app.clone();
    let ran = tauri::async_runtime::spawn_blocking(move || {
        let app = &worker;
        let state: tauri::State<Db> = app.state();
        let dir = data_dir(app);
        let mut store = lock(&state);
        if let Cmd::BackupCreate { keep, json } = &cmd {
            if crate::DATA_DIR_READONLY.load(Ordering::Relaxed) {
                return (Outcome::err(3, "the store refused the write (locked or unavailable)\n"), None);
            }
            // Same sequence as the app's own backup: pending counter writes land in data.db
            // first, and the lock is released after the point-in-time copy, before compression.
            let keep = keep.unwrap_or_else(|| default_keep(&store.settings));
            let gfs = gfs_from_settings(&store.settings);
            crate::flush_settings(app, &store.settings);
            let done = crate::backup_released(store, &dir, &dir.join("backups"), keep, gfs, || {});
            return match done {
                Ok(name) => (Outcome::ok(if *json { format!("{{\"name\":\"{name}\"}}\n") } else { format!("{name}\n") }), None),
                Err(e) => (Outcome::err(3, format!("{e}\n")), None),
            };
        }
        // The same refusal the standalone caller gives, up front: a store that is read-only, locked
        // after a failed decrypt or settings-locked must not even be changed in memory, or the next
        // save that succeeds would write it. (This also refuses a prompt-only command while only the
        // settings are locked; the standalone caller does not look at SETTINGS_LOCKED.)
        if is_mutating(&cmd)
            && (crate::DATA_DIR_READONLY.load(Ordering::Relaxed) || store.prompts_locked || crate::SETTINGS_LOCKED.load(Ordering::Relaxed))
        {
            return (Outcome::err(3, "the store refused the write (locked or unavailable)\n"), None);
        }
        if let Some(msg) = unreadable_store_message(&cmd, store.prompts_locked, !crate::SETTINGS_LOCKED.load(Ordering::Relaxed)) {
            return (Outcome::err(3, msg), None);
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
        (outcome, flag_set)
    })
    .await;
    let (outcome, flag_set) = match ran {
        Ok(v) => v,
        Err(e) => (Outcome::err(3, format!("{e}\n")), None),
    };
    if let Some((key, on)) = flag_set {
        crate::apply_flag_effects(app, &key, on);
    }
    if outcome.code == 0 && (outcome.mutated_prompts || outcome.mutated_trash || outcome.mutated_settings) {
        // The windows, armed hotkeys and tray still hold what they read before this command: tell
        // them, or a stale grid keeps a deleted tile and its next layout write restores the cell.
        crate::store::refresh_after_store_swap(app);
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
