use super::*;

// ---- Demo mode ----
//
// A bundled, harmless sample library that stands in for the user's data while switched on,
// so showing the app or taking a screenshot exposes nothing real. The sample store lives in
// its own subfolder of the canonical data directory and is rebuilt from scratch on every
// switch-on. The real store is never opened, written or read: switching only touches the
// "datapath" redirect file and the demo subfolder next to it.

// An app-specific name, not a generic "demo", which could collide with a folder a user
// (portable mode especially) already has beside their data; build_demo_store's
// wipe-and-rebuild must never touch a folder it did not create (see DEMO_MARKER_FILE).
pub(crate) const DEMO_SUBFOLDER: &str = "prompt-saver-demo";
// Written into DEMO_SUBFOLDER when this app first builds it; build_demo_store refuses to
// wipe an existing folder at that path without it, so an unrelated folder that shares the
// name survives.
pub(crate) const DEMO_MARKER_FILE: &str = ".prompt-saver-demo-marker";
pub(crate) const DEMO_POINTER_BACKUP: &str = "datapath.before-demo";

// A small solid-colour PNG kept as a literal instead of a shipped file; the sample image
// tile needs a valid image, not a real photo.
pub(crate) const DEMO_IMAGE_PNG_BASE64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAABgAAAAYCAIAAABvFaqvAAAAH0lEQVR42mNwm7aKKohh1KBRg0YNGjVo1KBRgwbeIACHvm2u1wMIiAAAAABJRU5ErkJggg==";

pub(crate) fn demo_prompt(id: &str, name: &str, text: &str) -> Prompt {
    Prompt { name: name.to_string(), text: text.to_string(), ..sync_blank_prompt(id.to_string()) }
}

// Ten prompts covering as many features at once as one harmless example
// reasonably can: plain text, an image tile, a rotating prompt, a self-fill
// placeholder, a favorite, two members of the sample folder, and (best
// effort, skipped if ffmpeg is not on hand) a real attached video next to a
// plain attached text file.
pub(crate) fn demo_prompts(dir: &Path, ffmpeg: Option<&Path>, lang: &str) -> Vec<Prompt> {
    let d = demo_text::demo_text(lang);
    let mut list = vec![
        Prompt {
            tags: vec![d.tag_demo.to_string()],
            ..demo_prompt("demo-welcome", d.welcome_name, d.welcome_text)
        },
        Prompt {
            image: format!("data:image/png;base64,{DEMO_IMAGE_PNG_BASE64}"),
            show_image: true,
            ..demo_prompt("demo-image", d.image_name, d.image_text)
        },
        Prompt {
            variants: vec![d.rotating_a.to_string(), d.rotating_b.to_string()],
            ..demo_prompt("demo-rotating", d.rotating_name, d.rotating_text)
        },
        demo_prompt("demo-placeholder", d.placeholder_name, d.placeholder_text),
        Prompt {
            favorite: true,
            tags: vec![d.tag_demo.to_string(), d.tag_favorite.to_string()],
            ..demo_prompt("demo-favorite", d.favorite_name, d.favorite_text)
        },
        demo_prompt("demo-chain-a", d.chain_a_name, d.chain_a_text),
        demo_prompt("demo-chain-b", d.chain_b_name, d.chain_b_text),
        Prompt {
            // Relative, not dir.join(...): an absolute path would bake the Windows account name (part
            // of %APPDATA%) into a row shown in demo mode's screenshots. load_store() resolves it at
            // read time.
            file_path: d.attach_file.to_string(),
            ..demo_prompt("demo-attachment", d.attach_name, "")
        },
        demo_prompt("demo-folder-a", d.folder_a_name, d.folder_a_text),
        demo_prompt("demo-folder-b", d.folder_b_name, d.folder_b_text),
    ];
    let _ = fs::write(dir.join(d.attach_file), d.attach_body);
    if let Some(ffmpeg) = ffmpeg {
        let video = dir.join(d.video_file);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let mut cmd = std::process::Command::new(ffmpeg);
            cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=duration=2:size=320x240:rate=10"])
                .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
                .arg(&video)
                .creation_flags(CREATE_NO_WINDOW);
            // Time-boxed and tied to the app job: a stalled ffmpeg must neither hang the switch
            // nor outlive the app.
            let ok = status_in_job(&mut cmd, std::time::Duration::from_secs(20)).map(|s| s.success()).unwrap_or(false);
            if ok {
                list.push(Prompt {
                    // Relative for the same reason as the attachment row above; load_store() resolves it.
                    file_path: d.video_file.to_string(),
                    ..demo_prompt("demo-video", d.video_name, "")
                });
            }
        }
    }
    list
}

// Two views and one grid folder, arranged the same way every time a fresh
// sample store is built.
pub(crate) fn demo_settings(has_video: bool, lang: &str) -> Settings {
    let d = demo_text::demo_text(lang);
    let mut settings = Settings::default();
    settings.ui_flags.insert("demoMode".to_string(), true);
    settings.active_view = "home".to_string();
    // The demo store has its own settings; without this the gear and banner would follow the
    // system language while the samples follow the language active when demo mode was switched
    // on.
    settings.language = lang.to_string();
    let mut home = View {
        id: "home".to_string(),
        name: d.view_home.to_string(),
        cols: default_cols(),
        rows: default_rows(),
        layouts: HashMap::new(),
        color: String::new(),
        members: {
            let mut m = vec![
                "demo-welcome".to_string(),
                "demo-image".to_string(),
                "demo-rotating".to_string(),
                "demo-placeholder".to_string(),
                "demo-favorite".to_string(),
                "demo-chain-a".to_string(),
                "demo-chain-b".to_string(),
                "demo-folder-a".to_string(),
                "demo-folder-b".to_string(),
            ];
            if has_video {
                m.push("demo-video".to_string());
            }
            m
        },
    };
    let folder = TileFolder {
        id: "demo-folder".to_string(),
        name: d.folder.to_string(),
        color: String::new(),
        view_id: "home".to_string(),
        cell: [0, 0],
        members: vec!["demo-folder-a".to_string(), "demo-folder-b".to_string()],
    };
    let folder_cells: std::collections::HashSet<(u32, u32)> = [(0u32, 0u32)].into_iter().collect();
    let folder_members: std::collections::HashSet<String> = folder.members.iter().cloned().collect();
    rebuild_view_layout(&mut home, &folder_cells, &folder_members);
    let mut team = View {
        id: "team".to_string(),
        name: d.view_team.to_string(),
        cols: default_cols(),
        rows: default_rows(),
        layouts: HashMap::new(),
        color: "#ef4444".to_string(),
        members: vec!["demo-attachment".to_string()],
    };
    rebuild_view_layout(&mut team, &std::collections::HashSet::new(), &std::collections::HashSet::new());
    settings.views = vec![home, team];
    settings.tile_folders = vec![folder];
    settings
}

// Wipes and rebuilds the sample store at `dir`. Never touches anything outside `dir`, and
// never wipes `dir` unless it is absent or carries the marker written below, so an
// unrelated folder at that path is left alone.
pub(crate) fn build_demo_store(dir: &Path, ffmpeg: Option<&Path>, lang: &str) -> Result<(), String> {
    if dir.is_dir() && !dir.join(DEMO_MARKER_FILE).is_file() {
        return Err("a folder already exists at the demo data path and was not created by demo mode; refusing to touch it".to_string());
    }
    // Delete every entry except the marker one at a time rather than remove_dir_all on the
    // folder: a locked file (data.db held open elsewhere) fails one removal and leaves the
    // rest, while a whole-folder remove can fail after deleting the marker, leaving a folder
    // without proof this app made it, and the guard above would refuse every later enable.
    // Best-effort per entry: a stray file is overwritten below (data.db) or harmless.
    if dir.is_dir() {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.file_name().to_str() == Some(DEMO_MARKER_FILE) {
                    continue;
                }
                let path = entry.path();
                let _ = if path.is_dir() { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
            }
        }
    }
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // Written unconditionally, whatever the cleanup above did or did not remove.
    fs::write(dir.join(DEMO_MARKER_FILE), b"").map_err(|e| e.to_string())?;
    // Not db_conn_at(): its schema-init guard runs once per process, right for the app's single
    // real connection but it would skip creating the tables here whenever another store was
    // opened first (every test in this file, a session that enters demo mode twice).
    let mut conn = Connection::open(dir.join("data.db")).map_err(|e| e.to_string())?;
    if !db_conn_init_schema(&conn) {
        return Err("could not prepare the demo store".to_string());
    }
    let mut written = WrittenRows::new();
    let prompts = demo_prompts(dir, ffmpeg, lang);
    let has_video = prompts.iter().any(|p| p.id == "demo-video");
    db_write_prompts(&mut conn, &prompts, &mut written).map_err(|e| e.to_string())?;
    db_write_settings(&conn, &demo_settings(has_video, lang)).map_err(|e| e.to_string())?;
    Ok(())
}

// Points `canonical`'s "datapath" redirect at `demo`, first saving whatever the redirect
// held (a custom folder, or none) so exit_demo_mode can restore it exactly. Split out of
// enter_demo_mode so a caller can build the sample store (build_demo_store, possibly
// running ffmpeg) well before touching the redirect; see enable_demo_mode.
pub(crate) fn flip_pointer_to_demo(canonical: &Path, demo: &Path) -> Result<(), String> {
    let pointer = canonical.join("datapath");
    let backup = canonical.join(DEMO_POINTER_BACKUP);
    // Never overwrite an existing backup: one exists only when an earlier enable never got its
    // exit (a crash, or a restart mid-demo), and it holds the one pointer value from before
    // demo mode, which is all exit_demo_mode must restore.
    if !backup.exists() {
        let already_demo = fs::read_to_string(&pointer)
            .map(|s| Path::new(s.trim()) == demo)
            .unwrap_or(false);
        if pointer.exists() && !already_demo {
            fs::copy(&pointer, &backup).map_err(|e| e.to_string())?;
        } else {
            // No redirect existed, or it already pointed at the demo folder (nothing legitimate to
            // preserve): record "no redirect before" instead of saving the demo path.
            fs::write(&backup, b"").map_err(|e| e.to_string())?;
        }
    }
    fs::write(&pointer, demo.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

// Test-only convenience (build + flip together). enable_demo_mode calls build_demo_store
// and flip_pointer_to_demo separately, with the store lock and DEMO_TRANSITION_LOCKED in
// between.
#[cfg(test)]
pub(crate) fn enter_demo_mode(canonical: &Path, ffmpeg: Option<&Path>) -> Result<PathBuf, String> {
    fs::create_dir_all(canonical).map_err(|e| e.to_string())?;
    let demo = canonical.join(DEMO_SUBFOLDER);
    build_demo_store(&demo, ffmpeg, "de")?;
    flip_pointer_to_demo(canonical, &demo)?;
    Ok(demo)
}

// ---- A real second running instance, not the allowMultipleInstances flag ----
//
// The allowMultipleInstances setting lives inside the very store being switched (it can be
// read back from the demo data.db after a redirect flip, or flipped by another instance
// mid-session), so it is no evidence about another process holding this data dir. Instead
// every process registers its pid in `instances/` under the canonical (never redirected)
// data dir at startup and removes it on a clean exit; another live pid there is real
// evidence, independent of either process's Settings.
pub(crate) fn instances_dir(canonical: &Path) -> PathBuf {
    canonical.join("instances")
}

pub(crate) fn register_running_instance(canonical: &Path) {
    let dir = instances_dir(canonical);
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(dir.join(std::process::id().to_string()), b"");
}

pub(crate) fn unregister_running_instance(canonical: &Path) {
    let _ = fs::remove_file(instances_dir(canonical).join(std::process::id().to_string()));
}

// Never a raw PathBuf == PathBuf: Windows paths are case-insensitive, and canonicalize()
// resolves a subst drive or junction to the same file that two differently spelled paths
// can reach, so an exact compare would call a real second instance "not this exe" over
// spelling. Falls back to a case-folded compare of the raw paths if either side cannot be
// canonicalized (the file replaced under a running process, access denied). Its own
// function so the comparison is unit-testable without a live pid.
pub(crate) fn same_exe_path(a: &Path, b: &Path) -> bool {
    let same = |a: &Path, b: &Path| a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase();
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => same(&a, &b),
        _ => same(a, b),
    }
}

// pid_is_alive() is shared with promptSync's lock file (see pid_from_lock/sync_lock_acquire
// below). It is not enough alone: Windows reuses freed pids, and the gap between a crash
// (or a task-killed process that never reached RunEvent::Exit) and pruning of its stale
// entry can span a whole later session, so an entry naming a pid some unrelated process now
// holds would read as alive forever and permanently refuse enable/disable_demo_mode. This
// adds the second check: not just that some process uses the pid, but that it is another
// running copy of this exe.
#[cfg(windows)]
pub(crate) fn instance_pid_is_this_exe(pid: u32) -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn CloseHandle(h: isize) -> i32;
        fn QueryFullProcessImageNameW(h: isize, flags: u32, buffer: *mut u16, size: *mut u32) -> i32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h == 0 {
            return false;
        }
        let mut buf = [0u16; 512];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut size) != 0;
        CloseHandle(h);
        if !ok {
            return false;
        }
        let image = PathBuf::from(String::from_utf16_lossy(&buf[..size as usize]));
        let Ok(exe) = std::env::current_exe() else {
            return false;
        };
        same_exe_path(&exe, &image)
    }
}
#[cfg(not(windows))]
pub(crate) fn instance_pid_is_this_exe(_pid: u32) -> bool {
    true // conservative off Windows: never treat a registered entry as a reused pid
}

// True when some other process currently has this install's canonical data dir registered.
// A marker is pruned only when its pid is actually dead (a crash, or a task-killed process
// that never reached RunEvent::Exit): pruning on a failed image-path check as well would
// delete a real other instance's entry whenever instance_pid_is_this_exe cannot confirm it
// (a permission edge case), losing the safety net for that instance for good. A failed
// image check with a live pid does not count as another instance and leaves the marker
// alone.
pub(crate) fn other_instance_running(canonical: &Path) -> bool {
    let Ok(entries) = fs::read_dir(instances_dir(canonical)) else {
        return false;
    };
    let me = std::process::id();
    let mut found = false;
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        if pid == me {
            continue;
        }
        if !pid_is_alive(pid) {
            let _ = fs::remove_file(entry.path());
            continue;
        }
        if instance_pid_is_this_exe(pid) {
            found = true;
        }
    }
    found
}

// Restores whatever `canonical`'s redirect held before demo mode. The demo subfolder is
// left alone: the next enable wipes and rebuilds it, and nothing from the ended session is
// read again.
pub(crate) fn exit_demo_mode(canonical: &Path) -> Result<(), String> {
    let pointer = canonical.join("datapath");
    let backup = canonical.join(DEMO_POINTER_BACKUP);
    let demo = canonical.join(DEMO_SUBFOLDER);
    if !backup.exists() {
        // Nothing recorded (a restart mid-demo, or exit called twice). Clear the redirect only if
        // it still points at the demo folder: an unrelated custom redirect must never be deleted
        // just because the backup is missing, or the app would open the canonical (empty) store
        // instead of the user's real custom folder.
        if fs::read_to_string(&pointer).map(|s| Path::new(s.trim()) == demo).unwrap_or(false) {
            let _ = fs::remove_file(&pointer);
        }
        return Ok(());
    }
    let prev = fs::read_to_string(&backup).map_err(|e| e.to_string())?;
    if prev.trim().is_empty() {
        let _ = fs::remove_file(&pointer);
    } else {
        fs::write(&pointer, prev.as_bytes()).map_err(|e| e.to_string())?;
    }
    let _ = fs::remove_file(&backup);
    Ok(())
}

// The flush of the real store before the switch. Both writes are attempted; a failed one aborts
// the switch, because the transition lock refuses every later write in this process and the
// unsaved change could never be written afterwards. A locked prompt set is not written at all.
pub(crate) fn demo_flush_result(prompts_locked: bool, prompts: impl FnOnce() -> bool, settings: impl FnOnce() -> bool) -> Result<(), String> {
    let prompts_ok = prompts_locked || prompts();
    let settings_ok = settings();
    if prompts_ok && settings_ok {
        Ok(())
    } else {
        Err("save-failed".to_string())
    }
}

// Switches to the bundled sample library, relaunching so every window loads it fresh (the
// relaunch restore_backup uses for a full data swap). The real store is flushed first so
// nothing pending is lost, then never touched again.
#[tauri::command]
pub(crate) async fn enable_demo_mode(app: AppHandle, state: State<'_, Db>, lang: Option<String>) -> Result<(), String> {
    let canonical = default_data_dir(&app);
    // A real second process (see other_instance_running), not the allowMultipleInstances
    // setting: that flag lives inside the store about to be switched, so it is no evidence
    // either way.
    if other_instance_running(&canonical) {
        return Err("demo-mode-multiple-instances".to_string());
    }
    // Build the sample store before the store lock or the redirect are touched: it can run
    // ffmpeg and take real time, and must not block anything meanwhile. (Flipping the redirect
    // first would let a racing ordinary write land in the freshly rebuilt demo store.)
    let ffmpeg = ffmpeg_search_path(&app);
    let demo = canonical.join(DEMO_SUBFOLDER);
    // The language the interface resolved, sent by the window; none means English.
    let lang_owned = lang.unwrap_or_else(|| "en".to_string());
    {
        let (demo, ffmpeg) = (demo.clone(), ffmpeg.clone());
        tauri::async_runtime::spawn_blocking(move || build_demo_store(&demo, ffmpeg.as_deref(), &lang_owned))
            .await
            .map_err(|e| e.to_string())??;
    }
    {
        let store = lock(&state);
        demo_flush_result(store.prompts_locked, || save_prompts(&app, &store), || save_settings_paired(&app, &store.settings))?;
        // Block every further store write for the rest of this process before the redirect flips
        // (disable_demo_mode does the same in reverse; db_conn() checks it), so nothing racing the
        // flip lands a real write in the demo store or clobbers its demoMode=true.
        DEMO_TRANSITION_LOCKED.store(true, Ordering::Relaxed);
    }
    // Re-checked here, not only at the top: build_demo_store can run ffmpeg for a while, during
    // which a second instance could have started and registered.
    if other_instance_running(&canonical) {
        DEMO_TRANSITION_LOCKED.store(false, Ordering::Relaxed);
        return Err("demo-mode-multiple-instances".to_string());
    }
    // On failure the flag must come back off: left on, every save in this process would fail
    // silently although demo mode never turned on.
    if let Err(e) = flip_pointer_to_demo(&canonical, &demo) {
        DEMO_TRANSITION_LOCKED.store(false, Ordering::Relaxed);
        return Err(e);
    }
    app.restart();
}

// Switches back to the real library and relaunches. Whatever was changed
// during the demo session simply stops being read from anywhere: the demo
// folder is not deleted, only abandoned until the next enable overwrites it.
#[tauri::command]
pub(crate) async fn disable_demo_mode(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    let canonical = default_data_dir(&app);
    if other_instance_running(&canonical) {
        return Err("demo-mode-multiple-instances".to_string());
    }
    {
        let store = lock(&state);
        // Flush demo-session settings still collected in memory to disk now, under the store lock
        // and while the "datapath" redirect still points at the demo folder, so they land in the
        // demo store. Then block every further store write for the rest of this (about to be
        // replaced) process: db_conn() refuses while this is set, covering every writer. After
        // exit_demo_mode flips the redirect back, RunEvent::Exit's flush (app.restart() triggers
        // it) or the 250ms settings_flush_tick must not write demo settings (views, sample folder,
        // demoMode=true) over the real ones.
        flush_settings(&app, &store.settings);
        DEMO_TRANSITION_LOCKED.store(true, Ordering::Relaxed);
    }
    // As in enable_demo_mode: left on after a failed exit, the app stays in demo mode with
    // every save silently refused.
    if let Err(e) = exit_demo_mode(&canonical) {
        DEMO_TRANSITION_LOCKED.store(false, Ordering::Relaxed);
        return Err(e);
    }
    app.restart();
}
