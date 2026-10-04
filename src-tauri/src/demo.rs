use super::*;

// ---- Demo mode ----
//
// A bundled, harmless sample library that stands in for the user's own data
// while it is switched on, so showing the app or taking a screenshot never
// exposes anything real. The sample store lives in its own "demo" subfolder
// of the canonical data directory and is rebuilt from scratch every time
// demo mode is switched on, so nothing changed during an earlier session
// carries into the next one. The real store is never opened, written or even
// read by any of this: switching demo mode on or off only ever touches the
// "datapath" redirect file and the demo subfolder next to it.

// An app-specific name, not a generic "demo" -- a folder that name would
// collide with something a user (portable mode especially) already has
// beside their data, and build_demo_store's wipe-and-rebuild must never
// touch a folder it did not create itself (see DEMO_MARKER_FILE).
pub(crate) const DEMO_SUBFOLDER: &str = "prompt-saver-demo";
// Written into DEMO_SUBFOLDER the first time this app builds it there;
// build_demo_store refuses to wipe an existing folder at that path unless
// this marker is present, so an unrelated folder that merely happens to
// share the name survives untouched.
pub(crate) const DEMO_MARKER_FILE: &str = ".prompt-saver-demo-marker";
pub(crate) const DEMO_POINTER_BACKUP: &str = "datapath.before-demo";

// A small solid-colour picture, generated once and kept as a literal instead
// of a shipped file, since the sample image tile needs a valid PNG, not a real
// photo.
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
            // Relative, not dir.join(...): an absolute path here would bake the
            // Windows account name (part of %APPDATA%) into a row shown in demo
            // mode's own screenshots. load_store() resolves it back to a real
            // path at read time.
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
            let ok = std::process::Command::new(ffmpeg)
                .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=duration=2:size=320x240:rate=10"])
                .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
                .arg(&video)
                .creation_flags(CREATE_NO_WINDOW)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                list.push(Prompt {
                    // Same reasoning as the attachment row above: relative, resolved
                    // back to a real path by load_store().
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
    // The demo store starts with its own settings; without this its gear and
    // banner would follow the system language while the samples follow the
    // language the interface was using when demo mode was switched on.
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

// Wipes and rebuilds the sample store at `dir` from nothing. Never touches
// anything outside `dir`, and never wipes `dir` itself unless it is either
// absent or already carries the marker this function itself writes below --
// a pre-existing, unrelated folder that merely happens to sit at that path
// is left completely alone.
pub(crate) fn build_demo_store(dir: &Path, ffmpeg: Option<&Path>, lang: &str) -> Result<(), String> {
    if dir.is_dir() && !dir.join(DEMO_MARKER_FILE).is_file() {
        return Err("a folder already exists at the demo data path and was not created by demo mode; refusing to touch it".to_string());
    }
    // Delete every entry EXCEPT the marker, one at a time, rather than
    // remove_dir_all(dir) on the whole folder -- a locked file (data.db held
    // open by something else) fails one removal and leaves the rest, but a
    // whole-folder remove_dir_all can fail after already having deleted the
    // marker along with everything else it managed to remove, leaving a
    // folder that still exists but no longer carries proof this app made it
    // -- the guard above would then refuse every later enable forever.
    // Best-effort per entry: a failure here does not abort the whole rebuild,
    // it just leaves a stray file that db_write_prompts/db_write_settings
    // below overwrite anyway (data.db) or that is harmless either way.
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
    // Written unconditionally, independent of whatever the cleanup above did
    // or did not manage to remove -- see the comment above it.
    fs::write(dir.join(DEMO_MARKER_FILE), b"").map_err(|e| e.to_string())?;
    // Not db_conn_at(): its schema-init guard runs only once per process (see
    // its own comment), which is exactly right for the app's single real
    // connection but would silently skip creating the tables here whenever
    // something else already opened a store first in the same process (every
    // test in this file, and a session that enters demo mode more than once).
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

// Points `canonical`'s "datapath" redirect at the demo subfolder, after
// rebuilding it fresh, first saving whatever the redirect held before (a
// custom folder, or none) so `exit_demo_mode` can put it back exactly.
// Points `canonical`'s "datapath" redirect at `demo`, first saving whatever
// the redirect held before (a custom folder, or none) so exit_demo_mode can
// put it back exactly. Split out of enter_demo_mode so a caller can build the
// sample store (build_demo_store, possibly running ffmpeg) well before ever
// touching the redirect -- see enable_demo_mode's own comment.
pub(crate) fn flip_pointer_to_demo(canonical: &Path, demo: &Path) -> Result<(), String> {
    let pointer = canonical.join("datapath");
    let backup = canonical.join(DEMO_POINTER_BACKUP);
    // Never overwrite an existing backup: one is already there only when a
    // previous enable never got a matching exit (a crash, or a restart that
    // landed mid-demo), and it still holds the one pointer value that came
    // from before demo mode -- the only thing exit_demo_mode must restore.
    if !backup.exists() {
        let already_demo = fs::read_to_string(&pointer)
            .map(|s| Path::new(s.trim()) == demo)
            .unwrap_or(false);
        if pointer.exists() && !already_demo {
            fs::copy(&pointer, &backup).map_err(|e| e.to_string())?;
        } else {
            // Either no redirect existed, or the redirect already pointed at the
            // demo folder itself (nothing legitimate to preserve) -- record "no
            // redirect before" rather than saving the demo path as the backup.
            fs::write(&backup, b"").map_err(|e| e.to_string())?;
        }
    }
    fs::write(&pointer, demo.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

// Test-only convenience (build + flip together, the shape a caller with
// nothing else going on wants). enable_demo_mode itself calls
// build_demo_store and flip_pointer_to_demo separately instead, with the
// store lock and DEMO_TRANSITION_LOCKED sitting in between them.
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
// enable_demo_mode/disable_demo_mode used to refuse based on the
// allowMultipleInstances setting alone -- but that flag lives INSIDE the very
// store being switched (it can be read back from the demo data.db after a
// redirect flip, or flipped by another instance mid-session), so it is not
// trustworthy evidence that a second process is not, right now, also holding
// this data dir open. Instead, every process registers its own pid in
// `instances/` under the CANONICAL (never redirected) data dir at startup and
// removes it again on a clean exit; checking for another LIVE pid there is
// real evidence, independent of anything either process's Settings currently
// says.
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

// pid_is_alive() itself is shared with promptSync's own lock file (see
// pid_from_lock/sync_lock_acquire below) -- same liveness probe, different
// registry. It is not enough here on its own, though: Windows reuses a
// freed pid, and the gap between a crash (or a task-killed process that
// never reached RunEvent::Exit) and its stale entry getting pruned can span
// an entire later session, so a stale entry that happens to name a pid some
// unrelated later process now holds would read as "alive" forever, wrongly
// and permanently refusing enable/disable_demo_mode for a user who is
// actually alone (a third review finding). instance_pid_is_this_exe adds
// the second check other_instance_running actually needs: not just "is
// SOME process using this pid", but "is it specifically another running
// copy of this exe".
// Never a raw PathBuf == PathBuf (a fourth review finding): Windows paths
// are case-insensitive on disk, and canonicalize() resolves a subst drive or
// a junction/symlink down to the same underlying file two differently-
// spelled paths can both reach -- an exact, case-sensitive compare of the
// raw strings would wrongly call a real second instance "not this exe" over
// nothing but spelling. Falls back to a case-folded compare of the raw paths
// only if either side cannot be canonicalized (e.g. the file was replaced
// out from under a running process, or an access-denied edge case). Its own
// function, pulled out of instance_pid_is_this_exe below, so the comparison
// itself is unit testable without a live pid.
pub(crate) fn same_exe_path(a: &Path, b: &Path) -> bool {
    let same = |a: &Path, b: &Path| a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase();
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => same(&a, &b),
        _ => same(a, b),
    }
}

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

// True when some OTHER process currently has this same install's canonical
// data dir registered. Prunes a marker only when its pid is actually dead (a
// crash, or a task-killed process that never reached RunEvent::Exit) -- a
// fourth review finding: pruning on the image-path check alone as well would
// delete a REAL other instance's own registry entry the moment
// instance_pid_is_this_exe cannot confirm it (a permission edge case, or a
// bug in that check itself), permanently losing this whole safety net for
// that instance's entry with no way for it to notice and re-register.
// Failing the image check without the pid being dead still refuses to count
// it as another instance (the pid is either reused by something unrelated,
// or genuinely not this exe), it just leaves the marker alone either way.
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

// Restores whatever `canonical`'s redirect held before demo mode. The demo
// subfolder itself is left alone: the next enable wipes and rebuilds it, so
// there is nothing to clean up here, and nothing changed during the session
// that just ended is kept anywhere the app will read from again.
pub(crate) fn exit_demo_mode(canonical: &Path) -> Result<(), String> {
    let pointer = canonical.join("datapath");
    let backup = canonical.join(DEMO_POINTER_BACKUP);
    let demo = canonical.join(DEMO_SUBFOLDER);
    if !backup.exists() {
        // Nothing recorded (a restart mid-demo, or exit called twice). Only clear
        // the redirect when it is actually still pointing at the demo folder --
        // an unrelated custom redirect the backup logic above did not touch must
        // never be deleted just because the backup happens to be missing, or the
        // app falls back to opening the canonical (empty) store instead of the
        // user's real custom folder.
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

// Switches to the bundled sample library, relaunching the app so every
// window loads it fresh, the same relaunch restore_backup already uses for
// a full data swap. The real store is flushed first so nothing pending is
// lost, then never touched again by anything below.
#[tauri::command]
pub(crate) async fn enable_demo_mode(app: AppHandle, state: State<'_, Db>, lang: Option<String>) -> Result<(), String> {
    let canonical = default_data_dir(&app);
    // A real second process (see other_instance_running's own comment), not
    // the allowMultipleInstances setting: that flag lives inside the very
    // store this is about to switch, so it is not trustworthy evidence either
    // way.
    if other_instance_running(&canonical) {
        return Err("demo-mode-multiple-instances".to_string());
    }
    // Build the sample store BEFORE the store lock or the redirect are
    // touched at all: this can run ffmpeg and take real wall-clock time, and
    // must never sit in front of anything blocked while it does (a second
    // review finding: the old order flipped the redirect first, so an
    // ordinary write racing the still-running build landed in the freshly
    // rebuilt demo store instead of the real one it was meant for).
    let ffmpeg = ffmpeg_search_path(&app);
    let demo = canonical.join(DEMO_SUBFOLDER);
    // The language the interface resolved, sent by the window; none means English.
    build_demo_store(&demo, ffmpeg.as_deref(), lang.as_deref().unwrap_or("en"))?;
    {
        let store = lock(&state);
        if !store.prompts_locked {
            save_prompts(&app, &store);
        }
        save_settings(&app, &store.settings);
        // Block every further store write for the rest of this process before
        // the redirect flips below -- the same guard disable_demo_mode uses in
        // reverse (db_conn() checks it directly), so nothing racing the flip
        // can land a real write into the demo store just built, or clobber its
        // own demoMode=true.
        DEMO_TRANSITION_LOCKED.store(true, Ordering::Relaxed);
    }
    // Re-checked here, not only at the top: build_demo_store above can run
    // ffmpeg and take real wall-clock time, during which a second instance
    // could have started and registered itself (a third review finding) --
    // the check at the top of this function only rules out one already
    // running before the build began.
    if other_instance_running(&canonical) {
        DEMO_TRANSITION_LOCKED.store(false, Ordering::Relaxed);
        return Err("demo-mode-multiple-instances".to_string());
    }
    // On failure the flag must come back off: left on, every save in this
    // process would silently fail forever even though demo mode never
    // actually turned on (a second review finding).
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
        // Flush whatever demo-session settings are still only collected in memory
        // to disk NOW, under the store lock and while the "datapath" redirect
        // still points at the demo folder -- so they land in the demo store, not
        // the real one. Then block every further store write for the rest of
        // this (about to be replaced) process: db_conn() itself refuses while
        // this is set, which covers every writer, not only the four that also
        // call data_dir_write_blocked() (a second review finding). exit_demo_mode
        // below flips the redirect back to the real store, and after that point
        // RunEvent::Exit's own flush (app.restart() below triggers it) or the
        // 250ms settings_flush_tick must not be allowed to write demo settings
        // (views, sample folder, demoMode=true) over the real ones.
        flush_settings(&app, &store.settings);
        DEMO_TRANSITION_LOCKED.store(true, Ordering::Relaxed);
    }
    // Same reasoning as enable_demo_mode's own Err path above: left on after a
    // failed exit, the app stays in demo mode with every save silently
    // refused forever.
    if let Err(e) = exit_demo_mode(&canonical) {
        DEMO_TRANSITION_LOCKED.store(false, Ordering::Relaxed);
        return Err(e);
    }
    app.restart();
}
