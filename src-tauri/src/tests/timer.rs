use super::*;

// ---- The shared timer thread ----
//
// Real intervals here are hours; every test below injects millisecond ones
// through the same registration call the app uses, so the behaviour under
// test is the scheduling itself and not a production constant.


pub(super) fn wait_until(mut cond: impl FnMut() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    cond()
}

pub(super) const NEVER_AGAIN: Duration = Duration::from_secs(3600);

#[test]
fn every_registered_job_fires_in_due_order() {
    let log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
    let s = Scheduler::start();
    // Deliberately registered out of order: the due time decides, not the
    // order they arrived in. Each job owns its own thread, so nothing bounds
    // how far apart two due times must be to survive real OS scheduling
    // jitter under load; wide gaps keep this a scheduling-order test rather
    // than a thread-wakeup-latency test. The full suite runs many tests
    // concurrently, so the gaps are wide (600ms) rather than the original
    // 150ms: a 150ms gap measurably flaked under that contention.
    for (name, delay_ms) in [("third", 1800u64), ("first", 600), ("second", 1200)] {
        let log = log.clone();
        s.register_periodic(name, Duration::from_millis(delay_ms), NEVER_AGAIN, move || {
            log.lock().unwrap_or_else(|e| e.into_inner()).push(name);
        });
    }
    let done = wait_until(
        || log.lock().unwrap_or_else(|e| e.into_inner()).len() == 3,
        Duration::from_secs(5),
    );
    let fired = log.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(done, "only {fired:?} fired");
    assert_eq!(fired, vec!["first", "second", "third"]);
}

#[test]
fn a_shortened_interval_wakes_the_thread_early() {
    let hits = Arc::new(AtomicU64::new(0));
    let s = Scheduler::start();
    {
        let hits = hits.clone();
        s.register_periodic("slow", NEVER_AGAIN, NEVER_AGAIN, move || {
            hits.fetch_add(1, Ordering::SeqCst);
        });
    }
    s.retime("slow", Duration::from_millis(10));
    assert!(
        wait_until(|| hits.load(Ordering::SeqCst) >= 1, Duration::from_secs(5)),
        "the job was still serving out its old wait"
    );
}

// Counts the threads this process currently owns, so the "one thread instead
// of one per chore" claim is a measured number rather than an assertion about
// the source.
#[cfg(windows)]
pub(super) mod thread_census {
    #[repr(C)]
    #[derive(Default)]
    struct ThreadEntry32 {
        size: u32,
        usage: u32,
        thread_id: u32,
        owner_pid: u32,
        base_pri: i32,
        delta_pri: i32,
        flags: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> isize;
        fn Thread32First(snap: isize, entry: *mut ThreadEntry32) -> i32;
        fn Thread32Next(snap: isize, entry: *mut ThreadEntry32) -> i32;
        fn CloseHandle(handle: isize) -> i32;
        fn GetCurrentProcessId() -> u32;
    }

    pub fn live_threads() -> usize {
        const SNAP_THREAD: u32 = 0x0000_0004;
        let size = std::mem::size_of::<ThreadEntry32>() as u32;
        unsafe {
            let snap = CreateToolhelp32Snapshot(SNAP_THREAD, 0);
            if snap == -1 {
                return 0;
            }
            let me = GetCurrentProcessId();
            let mut entry = ThreadEntry32 { size, ..Default::default() };
            let mut n = 0usize;
            if Thread32First(snap, &mut entry) != 0 {
                loop {
                    if entry.owner_pid == me {
                        n += 1;
                    }
                    entry.size = size;
                    if Thread32Next(snap, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snap);
            n
        }
    }
}

// Each registered job gets its own dedicated thread on purpose (a hung job
// must not block any other job's timer): so four chores now cost exactly
// four threads, same as four hand-rolled `loop { sleep(); work(); }` threads
// would. This proves the Scheduler is not secretly sharing one thread again.
//
// The census sees every thread in the process, the test harness's own
// included, so each attempt brackets its measurement with a baseline reading
// and only accepts a round where the baseline did not move underneath it.
// A round is also rejected outright when `with_jobs` itself is off by more
// than the four chores: cargo runs every test in this binary concurrently,
// so another test's own thread can start or stop in the narrow window
// between the two `live_threads()` reads even when the before/after
// baseline matches: that used to flake this test under load rather than
// prove anything about the Scheduler.
#[cfg(windows)]
#[test]
fn four_chores_cost_four_dedicated_threads() {
    use thread_census::live_threads;
    const CHORES: usize = 4;

    let mut measured = None;
    for _ in 0..40 {
        let before = live_threads();

        let s = Scheduler::start();
        for name in ["one", "two", "three", "four"] {
            s.register_periodic(name, NEVER_AGAIN, NEVER_AGAIN, || {});
        }
        wait_until(|| live_threads() >= before + CHORES, Duration::from_secs(5));
        let with_jobs = live_threads();
        s.shutdown(Duration::from_millis(500));
        wait_until(|| live_threads() <= before, Duration::from_secs(5));
        let after = live_threads();

        if before == after && with_jobs.saturating_sub(before) == CHORES {
            measured = Some((before, with_jobs));
            break;
        }
    }

    let (idle, with_jobs) =
        measured.expect("the process never stayed quiet long enough to count its threads");
    println!("idle threads: {idle} baseline, {with_jobs} with four registered chores");
    assert_eq!(with_jobs - idle, CHORES, "each chore should own its own thread");
}

#[test]
fn shutdown_stops_the_thread_promptly() {
    let s = Scheduler::start();
    s.register_periodic("parked", NEVER_AGAIN, NEVER_AGAIN, || {});
    // Let it actually reach the long wait before asking it to leave.
    std::thread::sleep(Duration::from_millis(20));
    let started = Instant::now();
    // Generous relative to the 2ms poll interval: this only proves shutdown
    // does not hang forever, not a tight production latency bound, so it must
    // not flake when many other tests are contending for CPU at the same time.
    let stopped = s.shutdown(Duration::from_millis(500));
    let took = started.elapsed();
    assert!(stopped, "the timer thread was still running after {took:?}");
    assert!(took < Duration::from_millis(500), "shutdown took {took:?}");
}

pub(super) fn wn_notes() -> Vec<WhatsNewSection> {
    parse_release_notes(
        "## Prompt Saver 1.0.0\nfirst\n\n## Prompt Saver 1.1.0\nsecond\n\n## Prompt Saver 2.0.0\nthird\n",
    )
}

#[test]
fn parse_release_notes_splits_on_each_version_heading() {
    let sections = wn_notes();
    assert_eq!(sections.len(), 3);
    assert_eq!(sections[0].version, "1.0.0");
    assert!(sections[0].markdown.starts_with("## Prompt Saver 1.0.0"));
    assert!(sections[0].markdown.contains("first"));
    assert_eq!(sections[2].version, "2.0.0");
}

#[test]
fn sections_for_update_same_version_shows_nothing() {
    let all = wn_notes();
    assert!(sections_for_update(&all, Some("2.0.0"), "2.0.0").is_empty());
}

#[test]
fn sections_for_update_jump_shows_every_section_in_between() {
    // Stored version is the oldest, current is the newest, expect both
    // later sections, oldest excluded.
    let all = wn_notes();
    let shown = sections_for_update(&all, Some("1.0.0"), "2.0.0");
    assert_eq!(shown.iter().map(|s| s.version.as_str()).collect::<Vec<_>>(), vec!["1.1.0", "2.0.0"]);
}

#[test]
fn sections_for_update_unknown_stored_version_shows_only_current() {
    let all = wn_notes();
    let shown = sections_for_update(&all, None, "1.1.0");
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].version, "1.1.0");
}

#[test]
fn parse_release_notes_ignores_a_heading_without_a_version() {
    let sections = parse_release_notes("## Prompt Saver 1.0.0\nbody\n\n## Downloads\nnot a release\n");
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].version, "1.0.0");
    // The non-version heading's text is swallowed, not attached to the
    // previous section or promoted to one of its own.
    assert!(!sections[0].markdown.contains("Downloads"));
}

#[test]
fn whats_new_check_impl_treats_an_empty_stored_version_as_fresh_install() {
    let all = wn_notes();
    let shown = whats_new_check_impl(&all, "", "1.0.0");
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].version, "1.0.0");
}

#[test]
fn whats_new_check_impl_passes_a_non_empty_stored_version_through() {
    let all = wn_notes();
    let shown = whats_new_check_impl(&all, "1.0.0", "2.0.0");
    assert_eq!(shown.iter().map(|s| s.version.as_str()).collect::<Vec<_>>(), vec!["1.1.0", "2.0.0"]);
}

// A store adopted from another folder mid-session must still get its schema:
// the one-time init used to be remembered per process, not per folder.
#[test]
fn schema_init_runs_for_every_data_folder() {
    let base = std::env::temp_dir().join(format!("{}-test-schema-per-dir", brand::SLUG));
    let _ = std::fs::remove_dir_all(&base);
    let dir_a = base.join("a");
    let dir_b = base.join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();
    assert!(db_conn_at(&dir_a).is_some());
    {
        let old = Connection::open(dir_b.join("data.db")).unwrap();
        old.execute_batch(
            "CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL);",
        )
        .unwrap();
    }
    let conn = db_conn_at(&dir_b).expect("the second folder opens");
    conn.execute(
        "INSERT INTO prompt_versions(prompt_id, ts, name, text, variants) VALUES('p', 1, 'n', 't', 'v')",
        [],
    )
    .expect("the variants column exists after the open");
    drop(conn);
    let _ = std::fs::remove_dir_all(&base);
}

// A purged prompt whose version-history DELETE failed must not keep its text
// forever: the startup sweep drops history of ids that are in neither table.
#[test]
fn a_read_only_instance_never_sweeps_version_history() {
    let root = std::env::temp_dir().join(format!("{}-test-sweep-gate", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    assert!(version_sweep_allowed(&root, false));
    assert!(!version_sweep_allowed(&root, true));
    std::fs::write(root.join("prompts.json"), b"[]").unwrap();
    assert!(!version_sweep_allowed(&root, false));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn startup_sweep_drops_version_history_of_vanished_prompts() {
    let conn = Connection::open_in_memory().unwrap();
    assert!(db_conn_init_schema(&conn));
    conn.execute_batch(
        "INSERT INTO prompts(id, ord, data) VALUES('live', 0, 'x');
             INSERT INTO trash(id, ord, data) VALUES('binned', 0, 'x');
             INSERT INTO prompt_versions(prompt_id, ts, name, text) VALUES('gone', 1, 'n', 'secret');
             INSERT INTO prompt_versions(prompt_id, ts, name, text) VALUES('gone', 2, 'n', 'secret2');
             INSERT INTO prompt_versions(prompt_id, ts, name, text) VALUES('live', 1, 'n', 't');
             INSERT INTO prompt_versions(prompt_id, ts, name, text) VALUES('binned', 1, 'n', 't');",
    )
    .unwrap();
    assert_eq!(db_sweep_orphan_versions(&conn), 2);
    let count = |id: &str| -> i64 {
        conn.query_row("SELECT COUNT(*) FROM prompt_versions WHERE prompt_id = ?1", [id], |r| r.get(0)).unwrap()
    };
    assert_eq!(count("gone"), 0);
    assert_eq!(count("live"), 1);
    assert_eq!(count("binned"), 1);
}

// sweep_orphan_files locks the store itself, so a caller that still holds a
// store guard deadlocks the whole app. Every call site must sit outside a live
// `lock(&state)` guard of its own function.
#[test]
fn sweep_orphan_files_is_never_called_under_the_store_guard() {
    let src = production_sources();
    let code: String = src
        .lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n");
    // Every top-level fn start, whatever its visibility or asyncness.
    let mut starts: Vec<usize> = Vec::new();
    let mut off = 0;
    for line in code.split('\n') {
        let rest = line.strip_prefix("pub(crate) ").or_else(|| line.strip_prefix("pub ")).unwrap_or(line);
        if rest.strip_prefix("async ").unwrap_or(rest).starts_with("fn ") {
            starts.push(off);
        }
        off += line.len() + 1;
    }
    let mut checked = 0;
    let mut defs = 0;
    for (n, &start) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(code.len());
        let body = &code[start..end];
        let head = body.lines().next().unwrap_or("");
        let name = head.split("fn ").nth(1).unwrap_or("?").split(['(', '<']).next().unwrap_or("?").to_string();
        if name == "sweep_orphan_files" {
            defs += 1;
            continue;
        }
        if !body.contains("sweep_orphan_files(") {
            continue;
        }
        // (guard name, brace depth it was declared at)
        let mut guards: Vec<(String, i32)> = Vec::new();
        let mut depth = 0i32;
        for line in body.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("let ") {
                if rest.contains("= lock(&state)") {
                    let nm = rest.trim_start_matches("mut ").split([' ', ':']).next().unwrap_or("").to_string();
                    guards.push((nm, depth));
                }
            }
            if let Some(arg) = t.strip_prefix("drop(") {
                let nm = arg.trim_end_matches(';').trim_end_matches(')').to_string();
                guards.retain(|(g, _)| *g != nm);
            }
            if t.contains("sweep_orphan_files(") {
                checked += 1;
                assert!(guards.is_empty(), "{name} calls sweep_orphan_files while holding the store lock");
            }
            for ch in line.chars() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        guards.retain(|(_, d)| *d <= depth);
                    }
                    _ => {}
                }
            }
        }
    }
    assert_eq!(defs, 1, "sweep_orphan_files must be defined exactly once in the crate sources");
    assert!(checked >= 5, "the scan found only {checked} call sites");
}

#[test]
fn taking_a_prompt_out_moves_the_occupant_instead_of_dropping_it() {
    let mut view = test_view("v");
    view.cols = 2;
    view.rows = 1;
    let key = grid_key(2, 1);
    view.layouts.entry(key.clone()).or_default().insert("occ".to_string(), [0, 0]);
    place_taken_out_member(&mut view, &[], "new", [0, 0]).unwrap();
    let layout = &view.layouts[&key];
    assert_eq!(layout.get("new"), Some(&[0, 0]));
    assert_eq!(layout.get("occ"), Some(&[1, 0]), "the occupant keeps a cell");
}

#[test]
fn the_last_folder_member_takes_the_folder_cell() {
    let mut view = test_view("v");
    view.cols = 3;
    view.rows = 1;
    let key = grid_key(3, 1);
    view.layouts.entry(key.clone()).or_default().insert("out".to_string(), [2, 0]);
    let folders = vec![test_tile_folder("f", "v", [1, 0], &["rest"])];
    let mut views = vec![view];
    settle_lone_folder_members(&mut views, &folders);
    assert_eq!(views[0].layouts[&key].get("rest"), Some(&[1, 0]));
}

#[test]
fn the_last_folder_member_joins_the_view_member_list() {
    let mut view = test_view("v");
    view.cols = 3;
    view.rows = 1;
    view.members = vec!["other".to_string()];
    let folders = vec![test_tile_folder("f", "v", [1, 0], &["rest"])];
    let mut views = vec![view];
    settle_lone_folder_members(&mut views, &folders);
    assert!(views[0].members.contains(&"rest".to_string()), "the seated member must be listed");
    assert_eq!(views[0].members.iter().filter(|m| *m == "rest").count(), 1);
    settle_lone_folder_members(&mut views, &folders);
    assert_eq!(views[0].members.iter().filter(|m| *m == "rest").count(), 1, "no duplicate on a repeat");
}

#[test]
fn the_last_folder_member_moves_aside_when_its_cell_was_taken() {
    let mut view = test_view("v");
    view.cols = 3;
    view.rows = 1;
    let key = grid_key(3, 1);
    view.layouts.entry(key.clone()).or_default().insert("out".to_string(), [1, 0]);
    let folders = vec![test_tile_folder("f", "v", [1, 0], &["rest"])];
    let mut views = vec![view];
    settle_lone_folder_members(&mut views, &folders);
    assert_eq!(views[0].layouts[&key].get("rest"), Some(&[0, 0]));
}

#[test]
fn taking_a_prompt_out_is_refused_when_nowhere_is_free() {
    let mut view = test_view("v");
    view.cols = 2;
    view.rows = 1;
    let key = grid_key(2, 1);
    view.layouts.entry(key.clone()).or_default().insert("occ".to_string(), [0, 0]);
    // the second cell belongs to another folder
    assert!(place_taken_out_member(&mut view, &[[1, 0]], "new", [0, 0]).is_err());
    let layout = &view.layouts[&key];
    assert_eq!(layout.get("occ"), Some(&[0, 0]));
    assert!(layout.get("new").is_none());
}

#[test]
fn deleting_a_view_also_drops_its_grid_folders() {
    let mut settings = Settings {
        views: vec![test_view("a"), test_view("b")],
        active_view: "a".to_string(),
        ..Settings::default()
    };
    settings.tile_folders.push(TileFolder {
        id: "f".to_string(),
        name: String::new(),
        color: String::new(),
        view_id: "a".to_string(),
        cell: [0, 0],
        members: vec!["x".to_string(), "y".to_string()],
    });
    let prompt_ids = vec!["x".to_string(), "y".to_string()];
    remove_view_from_settings(&mut settings, "a", &prompt_ids).unwrap();
    assert!(settings.tile_folders.is_empty(), "the orphan folder must not linger");
    assert_eq!(settings.active_view, "b");
    assert!(remove_view_from_settings(&mut settings, "b", &prompt_ids).is_err());
}

#[test]
fn a_failed_settings_save_rolls_the_tag_rename_back_on_disk_too() {
    let mut p = test_prompt("p1");
    p.tags = vec!["Old".to_string()];
    let mut store = secret_test_store(vec![p]);
    store.settings.tag_colors.insert("old".to_string(), "#111111".to_string());
    let mut disk: Vec<Vec<String>> = Vec::new();
    let result = rename_tag_in_store(
        &mut store,
        "old",
        "New",
        "new",
        &mut |st| {
            disk.push(st.prompts[0].tags.clone());
            true
        },
        &mut |_| false,
    );
    assert!(result.is_err());
    assert_eq!(store.prompts[0].tags, vec!["Old".to_string()]);
    assert_eq!(store.settings.tag_colors.get("old").map(String::as_str), Some("#111111"));
    assert_eq!(disk.last().unwrap(), &vec!["Old".to_string()], "the last write to disk is the original");
    assert_eq!(disk.len(), 2);
}

#[test]
fn a_failed_settings_save_rolls_the_tag_delete_back_on_disk_too() {
    let mut p = test_prompt("p1");
    p.tags = vec!["Old".to_string(), "Keep".to_string()];
    let mut store = secret_test_store(vec![p]);
    store.settings.tag_colors.insert("old".to_string(), "#111111".to_string());
    let mut disk: Vec<Vec<String>> = Vec::new();
    let result = delete_tag_in_store(
        &mut store,
        "old",
        &mut |st| {
            disk.push(st.prompts[0].tags.clone());
            true
        },
        &mut |_| false,
    );
    assert!(result.is_err());
    assert_eq!(store.prompts[0].tags, vec!["Old".to_string(), "Keep".to_string()]);
    assert_eq!(store.settings.tag_colors.get("old").map(String::as_str), Some("#111111"));
    assert_eq!(disk.last().unwrap(), &vec!["Old".to_string(), "Keep".to_string()]);
    assert_eq!(disk.len(), 2, "the delete and its undo were both written");

    // A clean run removes the tag and its colour and writes once.
    let mut writes = 0;
    delete_tag_in_store(&mut store, "old", &mut |_| { writes += 1; true }, &mut |_| true).unwrap();
    assert_eq!(store.prompts[0].tags, vec!["Keep".to_string()]);
    assert!(!store.settings.tag_colors.contains_key("old"));
    assert_eq!(writes, 1);
}

#[test]
fn a_palette_edit_remaps_tag_colors_too() {
    let mut colors: HashMap<String, String> = HashMap::new();
    colors.insert("a".to_string(), "#old".to_string());
    colors.insert("b".to_string(), "#keep".to_string());
    let map: HashMap<String, String> = [("#old".to_string(), "#new".to_string())].into_iter().collect();
    assert!(remap_tag_colors(&mut colors, &map));
    assert_eq!(colors["a"], "#new");
    assert_eq!(colors["b"], "#keep");
    assert!(!remap_tag_colors(&mut colors, &map), "a second pass changes nothing");
}

#[test]
fn two_staged_backups_never_share_one_file() {
    let root = std::env::temp_dir().join(format!("{}-test-stage-unique", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let (x, y) = (root.join("x.db"), root.join("y.db"));
    full_schema_db(&x, "x")
        .execute("INSERT INTO meta(key, value) VALUES('who', 'X')", [])
        .unwrap();
    full_schema_db(&y, "y")
        .execute("INSERT INTO meta(key, value) VALUES('who', 'Y')", [])
        .unwrap();
    let p1 = stage_and_validate_backup(&x, &root).unwrap();
    let p2 = stage_and_validate_backup(&y, &root).unwrap();
    assert_ne!(p1, p2);
    let who: String = Connection::open(&p1)
        .unwrap()
        .query_row("SELECT value FROM meta WHERE key = 'who'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(who, "X", "the first staged copy is still the first backup");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn stale_staged_restores_are_swept_but_other_files_stay() {
    let root = std::env::temp_dir().join(format!("{}-test-stage-sweep", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for name in ["data.db.restoring", "data.db.restoring-abc", "data.db", "data.db.locked.bak"] {
        std::fs::write(root.join(name), b"x").unwrap();
    }
    remove_stale_staged_restores(&root, Duration::from_secs(3600));
    assert!(root.join("data.db.restoring").exists(), "a young file is left alone");
    remove_stale_staged_restores(&root, Duration::ZERO);
    assert!(!root.join("data.db.restoring").exists());
    assert!(!root.join("data.db.restoring-abc").exists());
    assert!(root.join("data.db").exists());
    assert!(root.join("data.db.locked.bak").exists());
    let _ = std::fs::remove_dir_all(&root);
}

pub(super) fn long_child() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/c", "ping", "-n", "30", "127.0.0.1"]);
    cmd.stdout(std::process::Stdio::null());
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

pub(super) fn exits_within(child: &mut std::process::Child, ms: u64) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_millis(ms);
    while std::time::Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

#[test]
fn a_child_in_the_job_dies_when_the_job_handle_closes() {
    let job = KillOnCloseJob::new().expect("a job object");
    let mut child = spawn_in_job(&mut long_child(), Some(&job)).unwrap();
    assert!(child.try_wait().unwrap().is_none(), "the child is running");
    drop(job);
    let died = exits_within(&mut child, 1000);
    let _ = child.kill();
    assert!(died, "closing the job must end the child");
}

#[test]
fn a_child_outside_any_job_survives_the_same_drop() {
    let job = KillOnCloseJob::new().expect("a job object");
    let mut child = spawn_in_job(&mut long_child(), None).unwrap();
    drop(job);
    let died = exits_within(&mut child, 600);
    let _ = child.kill();
    let _ = child.wait();
    assert!(!died, "without the job nothing ends the child");
}

#[test]
fn the_export_journal_sweeps_only_dead_staged_exports_of_this_host() {
    let root = std::env::temp_dir().join(format!("{}-test-export-journal", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let dead = root.join(".export-4000000001-123.mp4");
    let alive = root.join(".export-4000000002-456.mp4");
    let foreign = root.join("precious.docx");
    let other_host = root.join(".export-4000000003-789.mp4");
    for f in [&dead, &alive, &foreign, &other_host] {
        std::fs::write(f, b"x").unwrap();
    }
    let journal = root.join("export-staging.txt");
    let host = local_host_name();
    let mut lines: Vec<String> = [(&dead, 4000000001u32), (&alive, 4000000002), (&foreign, 1)]
        .iter()
        .map(|(f, pid)| journal_line(&host, *pid, f))
        .collect();
    let other_line = journal_line("OTHER-PC", 4000000003, &other_host);
    lines.push(other_line.clone());
    journal_write(&journal, &lines);
    sweep_stale_exports(&journal, &host, |pid| pid == 4000000002);
    assert!(!dead.exists(), "a staged file of a dead process is removed");
    assert!(alive.exists(), "a live process keeps its file");
    assert!(foreign.exists(), "a journal line that is not a staged export name is never deleted");
    assert!(other_host.exists(), "another host's staged file is never deleted");
    let left = journal_lines(&journal);
    assert_eq!(left, vec![journal_line(&host, 4000000002, &alive), other_line]);
    let mine = root.join(format!(".export-{}-1.mp4", std::process::id()));
    export_journal_add(&journal, &mine);
    assert_eq!(journal_lines(&journal).len(), 3);
    export_journal_remove(&journal, &mine);
    assert_eq!(journal_lines(&journal).len(), 2, "only this process's own line is removed");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_journal_path_must_be_local_and_absolute() {
    assert!(is_local_absolute_path(r"C:\x\.export-1-1.mp4"));
    assert!(is_local_absolute_path(r"C:/x/.export-1-1.mp4"));
    assert!(!is_local_absolute_path(r"\\host\share\.export-1-1.mp4"));
    assert!(!is_local_absolute_path(r"//host/share/.export-1-1.mp4"));
    assert!(!is_local_absolute_path(r"\\?\UNC\host\share\.export-1-1.mp4"));
    assert!(!is_local_absolute_path(r".export-1-1.mp4"));
    assert!(!is_local_absolute_path(r"sub\.export-1-1.mp4"));
    assert!(!is_local_absolute_path(r"C:.export-1-1.mp4"));
}

#[test]
fn a_staged_restore_copy_is_stamped_with_now() {
    let root = std::env::temp_dir().join(format!("{}-test-stage-mtime", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let src = root.join("old.db");
    drop(full_schema_db(&src, "x"));
    let old = SystemTime::now() - Duration::from_secs(86400);
    std::fs::OpenOptions::new().write(true).open(&src).unwrap().set_modified(old).unwrap();
    let staged = stage_and_validate_backup(&src, &root).unwrap();
    let age = std::fs::metadata(&staged).unwrap().modified().unwrap().elapsed().unwrap_or_default();
    assert!(age < Duration::from_secs(60), "a fresh staged copy must not look stale");
    let _ = std::fs::remove_dir_all(&root);
}

pub(super) fn txt_round_trip(text: &str) -> ImportedPrompt {
    let mut p = test_prompt("p");
    p.text = text.to_string();
    let mut data = parse_txt(&to_txt(&[p]));
    assert_eq!(data.prompts.len(), 1, "one block in, one block out: {text:?}");
    data.prompts.remove(0)
}

#[test]
fn a_body_line_that_looks_like_metadata_survives_the_txt_round_trip() {
    for text in [
        "a\n@file x.js\nb",
        "@tags x",
        "tail\n@color red",
        "x\n@notes hi\n@style a 1",
        r"\@file keeps its backslash",
        "a\n\\\\@file two",
        r"path \\server\share
\begin{x}",
    ] {
        let got = txt_round_trip(text);
        assert_eq!(got.text, text);
        assert_eq!(got.file_path, "");
        assert!(got.tags.is_empty());
        assert_eq!(got.color, "");
    }
}

#[test]
fn a_markdown_rule_inside_a_prompt_survives_the_txt_round_trip() {
    let got = txt_round_trip("x\n\n---\n\ny");
    assert_eq!(got.text, "x\n\n---\n\ny");
    let mut p = test_prompt("q");
    p.text = "x\n\n---\n\ny".to_string();
    let data = parse_txt(&to_txt(&[p, test_prompt("r")]));
    assert_eq!(data.prompts.len(), 2);
}

#[test]
fn a_single_text_holding_the_variant_marker_is_not_split_on_import() {
    let got = txt_round_trip("one\n<!-- next -->\ntwo");
    assert_eq!(got.text, "one\n<!-- next -->\ntwo");
    assert!(got.variants.is_empty());
    let joined = join_variants("one\n<!-- next -->\ntwo", &[]);
    assert_eq!(split_variants(&joined), ("one\n<!-- next -->\ntwo".to_string(), Vec::new()));
}

#[test]
fn real_metadata_still_round_trips_beside_an_escaped_body() {
    let mut p = test_prompt("p");
    p.text = "line\n@file inside".to_string();
    p.file_path = r"C:\docs\a.txt".to_string();
    p.tags = vec!["t1".to_string(), "t2".to_string()];
    p.color = "#112233".to_string();
    let got = parse_txt(&to_txt(&[p])).prompts.remove(0);
    assert_eq!(got.text, "line\n@file inside");
    assert_eq!(got.file_path, r"C:\docs\a.txt");
    assert_eq!(got.tags, vec!["t1".to_string(), "t2".to_string()]);
    assert_eq!(got.color, "#112233");
}

#[test]
fn an_older_txt_export_with_trailing_metadata_still_imports() {
    let got = parse_txt("### Old\nbody line\n@color #abcdef\n@file D:\\x.txt").prompts.remove(0);
    assert_eq!(got.text, "body line");
    assert_eq!(got.color, "#abcdef");
    assert_eq!(got.file_path, r"D:\x.txt");
}

// An append that lands between the scrub's read and its rename used to be
// overwritten by the rewritten copy: the read has to happen under the same lock.
#[test]
fn strip_user_path_lines_keeps_a_line_appended_while_it_waits_for_the_lock() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-scrub-race-{}", std::process::id()));
    fs::write(&path, "[t=1 v1.0] panicked at C:\\Users\\example\\lib.rs:42\n").unwrap();
    let held = CrashLogLock::acquire(&path);
    let worker = {
        let path = path.clone();
        std::thread::spawn(move || strip_user_path_lines(&path))
    };
    std::thread::sleep(Duration::from_millis(100));
    {
        use std::io::Write;
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"[t=2 v1.0] LATE\n").unwrap();
    }
    drop(held);
    worker.join().unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("LATE"), "the appended line must survive the scrub: {text:?}");
    assert!(!text.to_ascii_lowercase().contains("\\users\\"), "the path is still scrubbed: {text:?}");
    let _ = fs::remove_file(&path);
}

// A standalone restore in a folder another machine holds locked must be refused
// with exit 3 and leave the store alone (the check used to sit behind the
// restore arm, which returned before reaching it).
pub(super) fn restore_fixture(name: &str) -> (PathBuf, String) {
    let dir = ddlock_test_dir(name);
    {
        let live = db_conn_at(&dir).expect("live store");
        live.execute("INSERT INTO meta(key, value) VALUES('marker', 'live')", []).unwrap();
    }
    let backup_name = "2024-01-01_00-00-00".to_string();
    let folder = dir.join("backups").join(&backup_name);
    fs::create_dir_all(&folder).unwrap();
    {
        let snap = db_conn_at(&folder).expect("snapshot store");
        snap.execute("INSERT INTO meta(key, value) VALUES('marker', 'backup')", []).unwrap();
    }
    (dir, backup_name)
}

pub(super) fn marker_of(dir: &Path) -> String {
    let conn = Connection::open(dir.join("data.db")).unwrap();
    conn.query_row("SELECT value FROM meta WHERE key = 'marker'", [], |r| r.get(0)).unwrap()
}

#[test]
fn a_standalone_restore_is_refused_while_another_machine_holds_the_folder() {
    let _g = DdlockGuard::capture();
    let (dir, name) = restore_fixture("restore-locked");
    write_data_dir_lock(
        &data_dir_lock_path(&dir),
        &DataDirLockInfo { host: "OTHER-PC".to_string(), pid: 999, seen: now_secs(), id: String::new(), beat: 0 },
    );
    let argv: Vec<String> = ["backup", "restore", &name, "--yes"].iter().map(|s| s.to_string()).collect();
    let code = cli::run_standalone_with(|| dir.clone(), &argv, &dir);
    assert_eq!(code, 3);
    assert_eq!(marker_of(&dir), "live", "the locked store must be untouched");
    assert_eq!(fs::read_dir(dir.join("backups")).unwrap().count(), 1, "no safety snapshot either");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_standalone_restore_of_a_free_folder_swaps_the_database_and_exits_zero() {
    let _g = DdlockGuard::capture();
    let (dir, name) = restore_fixture("restore-free");
    let argv: Vec<String> = ["backup", "restore", &name, "--yes"].iter().map(|s| s.to_string()).collect();
    let code = cli::run_standalone_with(|| dir.clone(), &argv, &dir);
    assert_eq!(code, 0);
    assert_eq!(marker_of(&dir), "backup");
    let _ = fs::remove_dir_all(&dir);
}

// The answer has to be on the wire before the relaunch: a restart inside the
// call drops the connection and the caller reads a restore that worked as a failure.
#[test]
fn the_answer_is_written_before_the_restart_runs() {
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let order = std::sync::Mutex::new(Vec::<&str>::new());
    rt.block_on(cli::answer_then_restart(
        async { order.lock().unwrap().push("answer") },
        true,
        || order.lock().unwrap().push("restart"),
    ));
    assert_eq!(*order.lock().unwrap(), vec!["answer", "restart"]);
    order.lock().unwrap().clear();
    rt.block_on(cli::answer_then_restart(
        async { order.lock().unwrap().push("answer") },
        false,
        || order.lock().unwrap().push("restart"),
    ));
    assert_eq!(*order.lock().unwrap(), vec!["answer"], "no restart unless one is due");
}

pub(super) fn recall_rects(dpi_for_wishes: f64, dpi_on_screen: f64) -> Vec<Rect> {
    let mut settings = Settings::default();
    for id in ["a", "b", "c"] {
        settings.floating.insert(id.to_string(), Pos::default());
    }
    settings.float_folders.push(FloatFolder {
        id: "f".to_string(),
        name: String::new(),
        pos: Pos::default(),
        scale: 1.0,
        members: vec!["a".to_string(), "b".to_string()],
    });
    let work = Rect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 };
    let (_ids, wishes) = recall_wishes(&settings, &[], work, dpi_for_wishes);
    let spots = spread_free_positions(&wishes, &[], work);
    // What the windows really occupy: the logical box times the screen's factor.
    let (_ids, real) = recall_wishes(&settings, &[], work, dpi_on_screen);
    spots
        .iter()
        .zip(real.iter())
        .map(|((x, y), r)| Rect { x: *x, y: *y, w: r.w, h: r.h })
        .collect()
}

pub(super) fn any_overlap(rects: &[Rect]) -> bool {
    for (i, a) in rects.iter().enumerate() {
        for b in &rects[i + 1..] {
            if a.x < b.x + b.w - 0.5 && b.x < a.x + a.w - 0.5 && a.y < b.y + b.h - 0.5 && b.y < a.y + a.h - 0.5 {
                return true;
            }
        }
    }
    false
}

#[test]
fn recalled_windows_do_not_overlap_on_a_scaled_screen() {
    assert!(!any_overlap(&recall_rects(1.5, 1.5)), "boxes sized for the screen spread without overlap");
    assert!(any_overlap(&recall_rects(1.0, 1.5)), "boxes sized without the factor overlap once drawn at it");
}

// The elevated script must reach PowerShell on its own command line, never as a
// file a same-user process could swap after the consent prompt.
#[test]
fn the_elevated_script_travels_as_an_encoded_command_and_decodes_exactly() {
    let script = "$ErrorActionPreference = 'Stop'\r\ntry { Add-WindowsCapability -Online -Name 'Language.OCR~~~de-DE~0.0.1.0' } catch { }\r\n";
    let params = ocr::elevated_params(script).unwrap();
    assert!(params.contains("-EncodedCommand "));
    assert!(!params.contains("ExecutionPolicy") && !params.contains("WindowStyle"), "no malware-looking switches: {params}");
    assert!(!params.contains("-File"), "no script file is referenced: {params}");
    assert!(!params.contains("-Command "), "no inline command either: {params}");
    let payload = params.rsplit("-EncodedCommand ").next().unwrap();
    let raw = base64_decode(payload);
    assert!(!raw.is_empty());
    // RFC 4648 vectors pin the shared encoder and decoder now that no second implementation checks them.
    for (plain, coded) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
        assert_eq!(base64_encode(plain.as_bytes()), coded);
        assert_eq!(base64_decode(coded), plain.as_bytes());
    }
    let units: Vec<u16> = raw.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    assert_eq!(String::from_utf16(&units).unwrap(), script);
    assert!(ocr::elevated_params(&"x".repeat(40_000)).is_err(), "an oversized script is refused, not truncated");
}

#[test]
fn a_failed_window_capture_brings_the_overlay_back() {
    let shown = std::cell::Cell::new(0);
    let failed: Result<(), String> = Err("ocr-language-missing".to_string());
    let out = show_overlay_again_on_error(failed, || shown.set(shown.get() + 1));
    assert_eq!(out, Err("ocr-language-missing".to_string()), "the error is passed on unchanged");
    assert_eq!(shown.get(), 1);
    let fine: Result<u8, String> = Ok(7);
    assert_eq!(show_overlay_again_on_error(fine, || shown.set(shown.get() + 1)), Ok(7));
    assert_eq!(shown.get(), 1, "a finished capture leaves the overlay alone");
}

#[test]
fn crop_rgba_keeps_the_requested_right_and_bottom_edge_when_it_starts_off_the_image() {
    let img = image::RgbaImage::new(100, 100);
    let c = crop_rgba(&img, -10, 0, 50, 10).unwrap();
    assert_eq!(c.dimensions(), (40, 10), "the 10 px left of the image are cut off, not added on the right");
    let c = crop_rgba(&img, 0, -5, 20, 30).unwrap();
    assert_eq!(c.dimensions(), (20, 25));
    let c = crop_rgba(&img, 90, 90, 50, 50).unwrap();
    assert_eq!(c.dimensions(), (10, 10), "past the far edge is still clamped");
    assert!(crop_rgba(&img, -60, 0, 50, 10).is_none(), "a rect wholly outside is nothing");
}

// A stand-in for ffmpeg that prints nothing and just sleeps, like a real one
// stuck reading a stalled source.
pub(super) fn silent_stub(dir: &Path) -> PathBuf {
    let src = dir.join("stub.rs");
    fs::write(&src, "fn main() { std::thread::sleep(std::time::Duration::from_secs(30)); }").unwrap();
    let exe = dir.join("stub.exe");
    let status = std::process::Command::new("rustc")
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .arg("-Copt-level=0")
        .status()
        .expect("rustc is on the path while tests run");
    assert!(status.success());
    exe
}

#[test]
fn a_cancel_stops_an_ffmpeg_that_prints_nothing() {
    let dir = std::env::temp_dir().join(format!("{}-test-silent-ffmpeg", brand::SLUG));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let stub = silent_stub(&dir);
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let flipper = {
        let cancel = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            cancel.store(true, Ordering::SeqCst);
        })
    };
    let started = std::time::Instant::now();
    let result = run_ffmpeg_to_staged(
        &stub,
        &dir.join("in.mp4"),
        &dir.join("out.mp4"),
        &[],
        &[],
        || cancel.load(Ordering::SeqCst),
        |_| {},
    );
    let took = started.elapsed();
    flipper.join().unwrap();
    assert_eq!(result.unwrap_err(), "cancelled");
    assert!(took < Duration::from_secs(2), "the cancel must not wait for ffmpeg's next line: {took:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn fresh_settings_close_to_tray() {
    assert!(Settings::default().minimize_to_tray);
    let bare: Settings = serde_json::from_str("{}").unwrap();
    assert!(bare.minimize_to_tray, "a store without the field gets the new default");
    let kept: Settings = serde_json::from_str(r#"{"minimize_to_tray":false}"#).unwrap();
    assert!(!kept.minimize_to_tray, "a stored false stays false");
}

#[test]
fn old_store_keeps_retention_forever() {
    let mut s: Settings = serde_json::from_str("{}").unwrap();
    assert!(!s.retention_defaults_v2, "a store without the marker predates the new defaults");
    s.migrate();
    assert_eq!(s.ui_values.get("historyDays"), Some(&0.0));
    assert_eq!(s.ui_values.get("recordingRetentionDays"), Some(&0.0));
    assert_eq!(history_max_age(&s), None);
    assert!(s.retention_defaults_v2);
}

#[test]
fn old_store_keeps_explicit_retention() {
    let mut s: Settings = serde_json::from_str(r#"{"ui_values":{"historyDays":7.0}}"#).unwrap();
    s.migrate();
    assert_eq!(s.ui_values.get("historyDays"), Some(&7.0), "a stored value is never touched");
    assert_eq!(s.ui_values.get("recordingRetentionDays"), Some(&0.0));
    assert_eq!(history_max_age(&s), Some(7 * 86_400));
}

#[test]
fn new_store_gets_retention_defaults() {
    let mut s = Settings::default();
    s.migrate();
    assert!(!s.ui_values.contains_key("historyDays"));
    assert!(!s.ui_values.contains_key("recordingRetentionDays"));
    assert_eq!(history_max_age(&s), Some(30 * 86_400));
    assert_eq!(RECORDING_RETENTION_DAYS_DEFAULT, 90.0);
}

#[test]
fn json_fallback_pins_retention_next_to_existing_data() {
    let base = std::env::temp_dir().join(format!("{}-test-retention-pin", brand::SLUG));
    let _ = std::fs::remove_dir_all(&base);
    // No settings next to a prompt list: an existing install.
    let with_prompts = base.join("prompts");
    std::fs::create_dir_all(&with_prompts).unwrap();
    assert!(write_sealed_json(&with_prompts.join("prompts.json"), &numbered_prompts(2)));
    let store = load_store_from_json(&with_prompts);
    assert_eq!(history_max_age(&store.settings), None);
    assert_eq!(store.settings.ui_values.get("recordingRetentionDays"), Some(&0.0));
    // Unreadable settings: also an existing install.
    let garbled = base.join("garbled");
    std::fs::create_dir_all(&garbled).unwrap();
    std::fs::write(garbled.join("settings.json"), "not json at all").unwrap();
    assert_eq!(history_max_age(&load_store_from_json(&garbled).settings), None);
    // Nothing at all: a new install gets the defaults.
    let fresh = base.join("fresh");
    std::fs::create_dir_all(&fresh).unwrap();
    assert_eq!(history_max_age(&load_store_from_json(&fresh).settings), Some(30 * 86_400));
    // Settings written by this version carry the marker and are left alone.
    let current = base.join("current");
    std::fs::create_dir_all(&current).unwrap();
    assert!(write_sealed_json(&current.join("settings.json"), &Settings::default()));
    assert_eq!(history_max_age(&load_store_from_json(&current).settings), Some(30 * 86_400));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn secret_timeout_defaults_to_15() {
    let mut s = Settings::default();
    assert_eq!(secret_timeout_min(&s), 15, "an untouched store locks after 15 minutes");
    s.ui_values.insert("secretTimeoutMin".into(), 0.0);
    assert_eq!(secret_timeout_min(&s), 0, "a stored 0 stays 'only on quit'");
    s.ui_values.insert("secretTimeoutMin".into(), 60.0);
    assert_eq!(secret_timeout_min(&s), 60);
}

// A pill or folder window shown at creation is an empty rectangle on screen until
// WebView2 has loaded its page (seen as a blank window at a slow boot). Both are
// built hidden and revealed by the page-load handler, never by a direct show().
#[test]
fn pill_and_folder_windows_are_built_hidden_and_shown_on_page_load() {
    let src = production_sources();
    for page in ["floating.html", "folder.html"] {
        let start_marker = format!("WebviewWindowBuilder::new(app, &label, WebviewUrl::App(\"{page}\".into()))");
        assert_eq!(src.matches(&start_marker).count(), 1, "{page} builder must occur exactly once");
        let a = src.find(&start_marker).unwrap();
        let b = a + src[a..].find(".build();").expect("end of builder chain");
        let chain = &src[a..b];
        assert!(chain.contains(".visible(false)"), "{page} window must be built hidden");
        assert!(chain.contains(".on_page_load(reveal_when_loaded)"), "{page} window must be revealed on page load");
        let rest_end = b + src[b..].find("\n}\n").expect("end of function");
        assert!(!src[b..rest_end].contains("win.show()"), "{page} window must not be shown right after build()");
    }
}

// A pill or folder that finishes loading while a snip session runs must not appear
// in the capture or over the overlay: it is queued for the restore pass instead, once.
// A window that is already visible is left alone (the fallback timer runs after the
// page-load reveal in the normal case).
#[test]
fn float_reveal_decision_defers_during_a_snip_and_skips_a_visible_window() {
    assert_eq!(reveal_decision(false, false), Reveal::Now);
    assert_eq!(reveal_decision(false, true), Reveal::AfterSnip);
    assert_eq!(reveal_decision(true, false), Reveal::Skip);
    assert_eq!(reveal_decision(true, true), Reveal::Skip);
    let mut hid = vec!["main".to_string()];
    queue_for_snip_restore(&mut hid, "float-1");
    queue_for_snip_restore(&mut hid, "float-1");
    assert_eq!(hid, vec!["main".to_string(), "float-1".to_string()]);
}

// The overlay watchdog must fire for a page that never presents, also on the reuse
// path where the parked overlay already counts as visible: it keys off "presented",
// never off the window's visibility.
#[test]
fn snip_watchdog_keys_off_presented_state() {
    assert!(watchdog_should_reveal(true, false), "session running, nothing presented yet");
    assert!(!watchdog_should_reveal(true, true), "already presented");
    assert!(!watchdog_should_reveal(false, false), "no session any more");
}

// Surfacing the overlay of an already running session goes through the same bounds
// assert as the normal reveal; a bare show() left a parked overlay at its parking spot.
#[test]
fn snip_already_active_path_sets_bounds_before_showing() {
    let src = production_sources();
    let start = src.find("pub(crate) async fn begin_capture(").expect("begin_capture");
    let end = start + src[start..].find("PICK_ACTIVE.store(pick").expect("end of the claim block");
    let claim = &src[start..end];
    let revealed = claim.find("reveal_snip(&app).await").expect("the already-active branch must reveal through reveal_snip");
    if let Some(shown) = claim.find("win.show()") {
        assert!(revealed < shown, "the already-active branch must not show the overlay before setting its bounds");
    }
}

// The pre-warmed snip overlay is built once at launch and then idles hidden. If
// anything ever shows it while idle (a slow boot, a window-manager quirk) it must
// not be an 800x600 white rectangle at the default spot of the main screen: it is
// built tiny, far outside every monitor and with a dark native backdrop, and an
// idle sweep puts it away again. The real snip path always sets its own bounds
// before it reveals the overlay.
#[test]
fn prewarmed_snip_overlay_is_tiny_off_screen_and_dark() {
    let spec = snip_prewarm_spec();
    assert!(spec.width <= 4 && spec.height <= 4, "the idle overlay must be a few pixels at most");
    assert!(spec.x <= -10000 && spec.y <= -10000, "the idle overlay must sit far outside every monitor");
    assert_eq!(spec.color.3, 255, "the idle overlay backdrop must be opaque, never white");
    assert!(spec.color.0 < 64 && spec.color.1 < 64 && spec.color.2 < 64, "the idle overlay backdrop must be dark");
}

#[test]
fn idle_snip_overlay_is_parked_whenever_it_shows_up() {
    assert!(snip_idle_needs_parking(false, true), "visible with no session: park it");
    assert!(!snip_idle_needs_parking(true, true), "a live session owns the overlay");
    assert!(!snip_idle_needs_parking(false, false), "already hidden: nothing to do");
    assert!(!snip_idle_needs_parking(true, false));
}

#[test]
fn snip_overlay_builder_uses_the_prewarm_spec_and_every_reveal_sets_bounds() {
    let src = production_sources();
    let a = src.find("pub(crate) fn build_snip_window").expect("build_snip_window");
    let b = a + src[a..].find("\n}\n").expect("end of build_snip_window");
    let body = &src[a..b];
    assert!(body.contains(".visible(false)"), "the overlay is built hidden");
    assert!(body.contains(".inner_size(") && body.contains(".position("), "the overlay is built tiny and off-screen");
    assert!(body.contains("set_background_color"), "the overlay never shows the default white backdrop");
    // The reveal puts the full bounds on before the show, on the reuse path and the shared reveal.
    let r = src.find("pub(crate) async fn reveal_snip").expect("reveal_snip");
    let rend = r + src[r..].find("\n}\n").expect("end of reveal_snip");
    let reveal = &src[r..rend];
    let size = reveal.find("set_size(").expect("reveal sets size");
    let pos = reveal.find("set_position(").expect("reveal sets position");
    let show = reveal.find("win.show()").expect("reveal shows");
    assert!(size < show && pos < show, "reveal_snip must set size and position before show");
}

// A finished snip must not leave the overlay hidden at full desktop size: it goes
// back to the tiny off-screen resting place, so a stray show can never put a
// desktop-sized window on screen.
#[test]
fn ending_a_snip_parks_the_overlay_tiny_and_off_screen() {
    let src = production_sources();
    let a = src.find("pub(crate) fn close_all_snip").expect("close_all_snip");
    let b = a + src[a..].find("
}
").expect("end of close_all_snip");
    assert!(src[a..b].contains("park_snip_window(&win)"), "close_all_snip must park the overlay");
    let c = src.find("pub(crate) fn park_snip_window").expect("park_snip_window");
    let d = c + src[c..].find("
}
").expect("end of park_snip_window");
    let body = &src[c..d];
    assert!(body.contains("snip_prewarm_spec()") && body.contains(".hide()") && body.contains("set_size(") && body.contains("set_position("));
}

#[test]
fn even_dim_rounds_down_to_even_with_a_floor_of_two() {
    assert_eq!(recording::even_dim(1008), 1008);
    assert_eq!(recording::even_dim(163), 162);
    assert_eq!(recording::even_dim(1921), 1920);
    assert_eq!(recording::even_dim(2), 2);
    assert_eq!(recording::even_dim(3), 2);
    assert_eq!(recording::even_dim(1), 2);
    assert_eq!(recording::even_dim(0), 2);
}

#[test]
fn discard_empty_output_removes_only_an_empty_file() {
    let dir = std::env::temp_dir().join(format!("ps-discard-empty-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let empty = dir.join("rec-empty.mp4");
    fs::write(&empty, b"").unwrap();
    assert!(recording::discard_empty_output(&empty));
    assert!(!empty.exists());
    let full = dir.join("rec-full.mp4");
    fs::write(&full, b"data").unwrap();
    assert!(!recording::discard_empty_output(&full));
    assert!(full.exists());
    assert!(!recording::discard_empty_output(&dir.join("missing.mp4")));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn uia_class_order_prefers_the_eight_class_and_falls_back() {
    use windows::Win32::UI::Accessibility::{CUIAutomation, CUIAutomation8};
    let order = keylog::uia_class_order();
    assert_eq!(order, [CUIAutomation8, CUIAutomation]);
    // Eight works: the original class is never tried.
    let mut tried = Vec::new();
    let got = keylog::first_ready_uia(&order, |c| {
        tried.push(*c);
        Some(1)
    });
    assert_eq!(got, Some(1));
    assert_eq!(tried, vec![CUIAutomation8]);
    // Eight fails: falls back to the original class.
    let mut tried = Vec::new();
    let got = keylog::first_ready_uia(&order, |c| {
        tried.push(*c);
        (*c == CUIAutomation).then_some(2)
    });
    assert_eq!(got, Some(2));
    assert_eq!(tried, vec![CUIAutomation8, CUIAutomation]);
    // Neither works: None, so the gap-everything path runs.
    assert_eq!(keylog::first_ready_uia::<u8>(&order, |_| None), None);
}

// Alternating pure red and pure blue pixel columns: the hardest case for subsampled chroma.
pub(super) fn red_blue_columns(w: u32, h: u32) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for _y in 0..h {
        for x in 0..w {
            rgba.extend_from_slice(if x % 2 == 0 { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
        }
    }
    rgba
}

// Spread between the red and blue content of the first two decoded pixels (0 = fully smeared).
pub(super) fn red_blue_separation(jpeg: &[u8]) -> i32 {
    let img = image::load_from_memory(jpeg).expect("decode").to_rgba8();
    let a = img.get_pixel(8, 4).0;
    let b = img.get_pixel(9, 4).0;
    (a[0] as i32 - b[0] as i32).abs() + (a[2] as i32 - b[2] as i32).abs()
}

#[test]
fn snip_overlay_preview_keeps_full_chroma_for_coloured_text() {
    // Below quality 90 the encoder halves the colour resolution and one-pixel coloured
    // strokes smear into each other; the overlay preview must stay above that line.
    let rgba = red_blue_columns(32, 16);
    let low = snip::encode_snip_jpeg(&rgba, 32, 16, 80).expect("q80");
    let used = snip::encode_snip_jpeg(&rgba, 32, 16, snip::SNIP_PREVIEW_Q).expect("preview");
    assert!(red_blue_separation(&low) < 300, "q80 was expected to smear this pattern");
    assert!(
        red_blue_separation(&used) > 500,
        "the overlay preview quality {} smears coloured one-pixel strokes",
        snip::SNIP_PREVIEW_Q
    );
}

#[test]
fn the_locked_database_copy_is_removed_and_nothing_else() {
    let root = std::env::temp_dir().join(format!("{}-test-locked-bak", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for name in ["data.db", "data.db.locked.bak", "data.db.locked.1700000000.bak", "data.db.locked.1700009999.bak"] {
        std::fs::write(root.join(name), b"x").unwrap();
    }
    remove_locked_backup(&root);
    assert!(!root.join("data.db.locked.bak").exists());
    assert!(!root.join("data.db.locked.1700000000.bak").exists());
    assert!(!root.join("data.db.locked.1700009999.bak").exists());
    assert!(root.join("data.db").exists());
    remove_locked_backup(&root); // already gone: harmless
    let _ = std::fs::remove_dir_all(&root);
}
