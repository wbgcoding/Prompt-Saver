use super::*;

// ---- the pipe server is created inside the runtime, never beside it ----

// Pins the platform fact install_server's shape rests on: a tokio named
// pipe registers with the runtime reactor as it is created, so creating
// the first server instance on the plain setup thread takes the whole
// process down before any window exists. Creating it inside the spawned
// task is what keeps the app startable.
#[test]
fn creating_a_pipe_server_outside_a_runtime_panics_but_works_inside_one() {
    use tokio::net::windows::named_pipe::ServerOptions;
    let name = format!(r"\\.\pipe\prompt-saver-reactor-probe-{}", std::process::id());

    let outside = {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {})); // the panic here is the expected result
        let name = name.clone();
        let result = std::panic::catch_unwind(move || {
            let _ = ServerOptions::new().first_pipe_instance(true).create(&name);
        });
        std::panic::set_hook(previous);
        result
    };
    assert!(outside.is_err(), "a pipe server created off the runtime must panic");

    let rt = tokio::runtime::Builder::new_current_thread().enable_io().build().unwrap();
    rt.block_on(async {
        assert!(
            ServerOptions::new().first_pipe_instance(true).create(&name).is_ok(),
            "inside a runtime the same call must succeed"
        );
    });
}

// ---- a second launch never ends silently ----

#[test]
fn a_running_instance_that_answers_in_time_is_never_taken_over() {
    assert!(!should_take_over_hung_instance(true, true, true));
}

#[test]
fn a_silent_running_instance_is_taken_over_after_the_timeout() {
    assert!(should_take_over_hung_instance(false, true, true));
}

#[test]
fn a_different_executable_path_is_never_taken_over() {
    assert!(!should_take_over_hung_instance(false, true, false));
    assert!(!should_take_over_hung_instance(false, false, false));
}

#[test]
fn a_different_user_is_never_taken_over_even_at_the_same_path() {
    assert!(!should_take_over_hung_instance(false, false, true));
}

// ---- a link's own handshake never ends the launch by itself ----

#[test]
fn a_live_instance_falls_through_to_the_plugin_instead_of_ending_the_link() {
    assert_eq!(link_decision(LaunchDecision::HandedOff), LaunchDecision::Primary);
}

#[test]
fn no_running_instance_stays_primary() {
    assert_eq!(link_decision(LaunchDecision::Primary), LaunchDecision::Primary);
}

#[test]
fn a_taken_over_hang_stays_a_takeover() {
    assert_eq!(link_decision(LaunchDecision::TookOver), LaunchDecision::TookOver);
}

// The Win32 half of the same decision, against real processes: without
// this, a failing token or image-name query would silently turn every
// takeover into a no-op while the pure tests above stayed green.
#[test]
fn process_identity_check_recognises_this_process_and_rejects_a_foreign_exe() {
    let (same_user, same_exe) = process_identity_check(std::process::id());
    assert!(same_user, "this process must count as the same user");
    assert!(same_exe, "this process must count as the same executable");

    let mut child = std::process::Command::new("cmd.exe")
        .args(["/c", "ping -n 20 127.0.0.1 >nul"])
        .spawn()
        .expect("a child process to compare against");
    let (child_user, child_exe) = process_identity_check(child.id());
    let _ = child.kill();
    let _ = child.wait();
    assert!(child_user, "a child started by this test runs as the same user");
    assert!(!child_exe, "a different executable must never count as the same one");
}

#[test]
fn terminating_a_process_waits_until_it_is_really_gone() {
    let mut child = std::process::Command::new("cmd.exe")
        .args(["/c", "ping -n 60 127.0.0.1 >nul"])
        .spawn()
        .expect("a child process to end");
    let pid = child.id();
    assert!(terminate_process(pid), "the child must be reported as ended");
    // Reported gone means gone: the handle is signalled, so a wait here
    // returns at once instead of blocking for the ping's remaining minute.
    assert!(child.try_wait().expect("wait").is_some(), "the process must already be reaped");
}

// A directory under the system temp folder, unique per test, removed on
// drop -- the CLI's own store lives in a real folder (portable/APPDATA
// resolution is bypassed entirely by pointing db_conn_at/do_backup_at at
// it directly), so nothing here ever touches the real Prompt Saver data.
struct TempDir(PathBuf);
impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "prompt-saver-clitest-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn mk_prompt(id: &str, name: &str, text: &str, tags: &[&str], favorite: bool) -> Prompt {
    Prompt {
        id: id.to_string(),
        name: name.to_string(),
        text: text.to_string(),
        color: String::new(),
        image: String::new(),
        show_image: false,
        copy_image: false,
        file_path: String::new(),
        pdf_page: 0,
        icon_path: String::new(),
        caption: String::new(),
        caption_size: 0,
        font: String::new(),
        font_size: 0,
        favorite,
        tags: tags.iter().map(|s| (*s).to_string()).collect(),
        notes: String::new(),
        hotkey: String::new(),
        hotkey_paste: String::new(),
        trashed_at: 0,
        variants: Vec::new(),
        rotate_mode: String::new(),
        rotate_auto: false,
        media_duration_ms: 0,
        media_width: 0,
        media_height: 0,
        media_size_bytes: 0,
        image_text: String::new(),
        secret: false,
    }
}

fn fixture_store() -> Store {
    let mut store = Store {
        prompts: Vec::new(),
        trash: Vec::new(),
        settings: crate::Settings::default(),
        prompts_locked: false,
        written: std::cell::RefCell::new(WrittenRows::new()),
    };
    store.settings.views.push(crate::View {
        id: "v1".to_string(),
        name: "Main".to_string(),
        cols: 5,
        rows: 4,
        layouts: std::collections::HashMap::new(),
        color: String::new(),
        members: Vec::new(),
    });
    store.settings.active_view = "v1".to_string();
    store.prompts.push(mk_prompt("p1", "Alpha", "alpha text", &["work"], true));
    store.prompts.push(mk_prompt("p2", "Beta", "beta text", &[], false));
    store
}

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn run_cmd(store: &mut Store, dir: &Path, items: &[&str]) -> Outcome {
    let parsed = parse(&args(items)).unwrap_or_else(|e| panic!("expected {items:?} to parse: {e}"));
    let cmd = match parsed {
        Parsed::Run(c) => c,
        Parsed::Help(_) => panic!("expected a command, got help for {items:?}"),
    };
    exec(store, dir, &cmd, Path::new("."))
}

fn parse_err(items: &[&str]) {
    let argv = args(items);
    assert!(parse(&argv).is_err(), "expected {items:?} to fail parsing");
}

// ---- parsing ----

#[test]
fn every_top_level_command_parses_with_its_minimum_arguments() {
    assert!(matches!(parse(&args(&["version"])), Ok(Parsed::Run(Cmd::Version))));
    assert!(matches!(parse(&args(&["list"])), Ok(Parsed::Run(Cmd::List { .. }))));
    assert!(matches!(parse(&args(&["get", "p1"])), Ok(Parsed::Run(Cmd::Get { .. }))));
    assert!(matches!(parse(&args(&["copy", "p1"])), Ok(Parsed::Run(Cmd::Copy { .. }))));
    assert!(matches!(parse(&args(&["add", "--name", "n", "--text", "t"])), Ok(Parsed::Run(Cmd::Add { .. }))));
    assert!(matches!(parse(&args(&["edit", "p1", "--name", "n"])), Ok(Parsed::Run(Cmd::Edit { .. }))));
    assert!(matches!(parse(&args(&["delete", "p1"])), Ok(Parsed::Run(Cmd::Delete { .. }))));
    assert!(matches!(parse(&args(&["restore", "p1"])), Ok(Parsed::Run(Cmd::Restore { .. }))));
    assert!(matches!(parse(&args(&["tag", "list"])), Ok(Parsed::Run(Cmd::TagList { .. }))));
    assert!(matches!(parse(&args(&["tag", "add", "p1", "x"])), Ok(Parsed::Run(Cmd::TagAdd { .. }))));
    assert!(matches!(parse(&args(&["tag", "remove", "p1", "x"])), Ok(Parsed::Run(Cmd::TagRemove { .. }))));
    assert!(matches!(parse(&args(&["view", "list"])), Ok(Parsed::Run(Cmd::ViewList { .. }))));
    assert!(matches!(parse(&args(&["view", "add", "n"])), Ok(Parsed::Run(Cmd::ViewAdd { .. }))));
    assert!(matches!(parse(&args(&["view", "remove", "v1"])), Ok(Parsed::Run(Cmd::ViewRemove { .. }))));
    assert!(matches!(parse(&args(&["search", "q"])), Ok(Parsed::Run(Cmd::Search { .. }))));
    assert!(matches!(parse(&args(&["export", "out.json"])), Ok(Parsed::Run(Cmd::Export { .. }))));
    assert!(matches!(parse(&args(&["import", "in.json"])), Ok(Parsed::Run(Cmd::Import { .. }))));
    assert!(matches!(parse(&args(&["backup", "create"])), Ok(Parsed::Run(Cmd::BackupCreate { .. }))));
    assert!(matches!(parse(&args(&["backup", "list"])), Ok(Parsed::Run(Cmd::BackupList { .. }))));
    assert!(matches!(parse(&args(&["backup", "restore", "2024-01-01_00-00-00"])), Ok(Parsed::Run(Cmd::BackupRestore { .. }))));
    assert!(matches!(parse(&args(&["settings", "get", "theme"])), Ok(Parsed::Run(Cmd::SettingsGet { .. }))));
    assert!(matches!(parse(&args(&["settings", "set", "theme", "dark"])), Ok(Parsed::Run(Cmd::SettingsSet { .. }))));
}

#[test]
fn missing_required_arguments_are_a_bad_invocation() {
    parse_err(&["get"]);
    parse_err(&["copy"]);
    parse_err(&["add", "--name", "n"]); // no --text
    parse_err(&["add", "--text", "t"]); // no --name
    parse_err(&["delete"]);
    parse_err(&["restore"]);
    parse_err(&["tag"]);
    parse_err(&["tag", "add", "p1"]); // no tag name
    parse_err(&["view"]);
    parse_err(&["view", "add"]); // no name
    parse_err(&["search"]);
    parse_err(&["export"]);
    parse_err(&["import"]);
    parse_err(&["backup"]);
    parse_err(&["backup", "restore"]);
    parse_err(&["settings"]);
    parse_err(&["settings", "get"]);
    parse_err(&["settings", "set", "theme"]); // no value
    parse_err(&["nonsense"]);
    parse_err(&["get", "p1", "--extra"]); // unexpected argument
    parse_err(&["edit", "p1"]); // nothing to change
}

#[test]
fn edit_rejects_favorite_and_no_favorite_together() {
    parse_err(&["edit", "p1", "--favorite", "--no-favorite"]);
}

#[test]
fn ids_with_an_invalid_shape_are_rejected_before_any_lookup() {
    parse_err(&["get", "../escape"]);
    parse_err(&["get", "id with spaces"]);
    parse_err(&["get", ""]);
}

#[test]
fn bare_help_never_needs_the_rest_of_the_arguments() {
    assert!(matches!(parse(&args(&["--help"])), Ok(Parsed::Help(_))));
    assert!(matches!(parse(&args(&["get", "--help"])), Ok(Parsed::Help(_))));
    assert!(matches!(parse(&args(&["tag", "--help"])), Ok(Parsed::Help(_))));
    assert!(matches!(parse(&args(&["tag", "add", "--help"])), Ok(Parsed::Help(_))));
    assert!(matches!(parse(&args(&["backup", "restore", "--help"])), Ok(Parsed::Help(_))));
}

// ---- list / get / copy / search: exit codes and both output forms ----

#[test]
fn list_prints_every_prompt_in_text_and_json() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let text = run_cmd(&mut store, &dir, &["list"]);
    assert_eq!(text.code, 0);
    assert!(text.stdout.contains("p1") && text.stdout.contains("p2"));
    let json = run_cmd(&mut store, &dir, &["list", "--json"]);
    assert_eq!(json.code, 0);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(json.stdout.trim()).expect("valid JSON array");
    assert_eq!(parsed.len(), 2);
}

#[test]
fn list_favorite_filters_down_to_the_starred_prompt() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let out = run_cmd(&mut store, &dir, &["list", "--favorite"]);
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("p1"));
    assert!(!out.stdout.contains("p2"));
}

#[test]
fn get_returns_not_found_for_an_unknown_id() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let out = run_cmd(&mut store, &dir, &["get", "does-not-exist"]);
    assert_eq!(out.code, 2);
    assert!(out.stdout.is_empty());
    assert!(!out.stderr.is_empty());
}

#[test]
fn get_prints_the_full_prompt_in_text_and_json() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let text = run_cmd(&mut store, &dir, &["get", "p1"]);
    assert_eq!(text.code, 0);
    assert!(text.stdout.contains("alpha text"));
    let json = run_cmd(&mut store, &dir, &["get", "p1", "--json"]);
    assert_eq!(json.code, 0);
    let v: serde_json::Value = serde_json::from_str(json.stdout.trim()).expect("valid JSON object");
    assert_eq!(v["id"], "p1");
    assert_eq!(v["text"], "alpha text");
}

#[test]
fn copy_reports_not_found_for_an_unknown_id() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let out = run_cmd(&mut store, &dir, &["copy", "does-not-exist"]);
    assert_eq!(out.code, 2);
}

#[test]
fn copy_never_changes_the_store() {
    let mut store = fixture_store();
    let before = store.prompts.len();
    let dir = std::env::temp_dir();
    let _ = run_cmd(&mut store, &dir, &["copy", "p1"]);
    assert_eq!(store.prompts.len(), before);
}

#[test]
fn search_matches_name_text_and_tags_case_insensitively() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let by_name = run_cmd(&mut store, &dir, &["search", "ALPHA"]);
    assert!(by_name.stdout.contains("p1") && !by_name.stdout.contains("p2"));
    let by_tag = run_cmd(&mut store, &dir, &["search", "", "--tag", "work"]);
    assert!(by_tag.stdout.contains("p1") && !by_tag.stdout.contains("p2"));
    let none = run_cmd(&mut store, &dir, &["search", "no-such-word-anywhere"]);
    assert!(none.stdout.is_empty());
}

// ---- mutating commands: refused without --yes, applied with it ----

#[test]
fn add_is_refused_without_yes_and_leaves_the_store_untouched() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let before = store.prompts.len();
    let out = run_cmd(&mut store, &dir, &["add", "--name", "New", "--text", "hi"]);
    assert_eq!(out.code, 4);
    assert_eq!(store.prompts.len(), before);
}

#[test]
fn add_with_yes_appends_a_new_prompt_and_prints_its_id() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let before = store.prompts.len();
    let out = run_cmd(&mut store, &dir, &["add", "--name", "New", "--text", "hi", "--tags", "a,b", "--yes"]);
    assert_eq!(out.code, 0);
    assert_eq!(store.prompts.len(), before + 1);
    let new_id = out.stdout.trim();
    let added = store.prompts.iter().find(|p| p.id == new_id).expect("new prompt present");
    assert_eq!(added.name, "New");
    assert_eq!(added.tags, vec!["a".to_string(), "b".to_string()]);
    // JSON form reports the same id as an object field, not a bare line.
    let out2 = run_cmd(&mut store, &dir, &["add", "--name", "N2", "--text", "t2", "--yes", "--json"]);
    let v: serde_json::Value = serde_json::from_str(out2.stdout.trim()).unwrap();
    assert!(v["id"].as_str().is_some());
}

#[test]
fn add_rejects_an_over_long_tag_and_changes_nothing() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let before = store.prompts.len();
    let long_tag = "x".repeat(61);
    let out = run_cmd(&mut store, &dir, &["add", "--name", "New", "--text", "hi", "--tags", &long_tag, "--yes"]);
    assert_eq!(out.code, 4);
    assert_eq!(store.prompts.len(), before);
}

#[test]
fn edit_is_refused_without_yes() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let out = run_cmd(&mut store, &dir, &["edit", "p1", "--name", "Changed"]);
    assert_eq!(out.code, 4);
    assert_eq!(store.prompts[0].name, "Alpha");
}

#[test]
fn edit_with_yes_changes_only_the_given_fields() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let out = run_cmd(&mut store, &dir, &["edit", "p1", "--favorite" /* already true */, "--notes", "hello", "--yes"]);
    assert_eq!(out.code, 0);
    let p = store.prompts.iter().find(|p| p.id == "p1").unwrap();
    assert_eq!(p.text, "alpha text"); // untouched
    assert_eq!(p.notes, "hello");
}

#[test]
fn edit_on_an_unknown_id_is_not_found() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let out = run_cmd(&mut store, &dir, &["edit", "ghost", "--name", "x", "--yes"]);
    assert_eq!(out.code, 2);
}

#[test]
fn delete_moves_a_prompt_to_the_trash_only_with_yes() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let refused = run_cmd(&mut store, &dir, &["delete", "p1"]);
    assert_eq!(refused.code, 4);
    assert_eq!(store.prompts.len(), 2);
    let ok = run_cmd(&mut store, &dir, &["delete", "p1", "--yes"]);
    assert_eq!(ok.code, 0);
    assert_eq!(store.prompts.len(), 1);
    assert_eq!(store.trash.len(), 1);
    assert_eq!(store.trash[0].id, "p1");
}

#[test]
fn delete_of_an_unknown_id_is_not_found() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let out = run_cmd(&mut store, &dir, &["delete", "ghost", "--yes"]);
    assert_eq!(out.code, 2);
}

#[test]
fn restore_moves_a_trashed_prompt_back_only_with_yes() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let _ = run_cmd(&mut store, &dir, &["delete", "p1", "--yes"]);
    let refused = run_cmd(&mut store, &dir, &["restore", "p1"]);
    assert_eq!(refused.code, 4);
    assert_eq!(store.trash.len(), 1);
    let ok = run_cmd(&mut store, &dir, &["restore", "p1", "--yes"]);
    assert_eq!(ok.code, 0);
    assert_eq!(store.trash.len(), 0);
    assert!(store.prompts.iter().any(|p| p.id == "p1"));
}

#[test]
fn tag_add_and_remove_round_trip_only_with_yes() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let refused = run_cmd(&mut store, &dir, &["tag", "add", "p2", "urgent"]);
    assert_eq!(refused.code, 4);
    assert!(store.prompts[1].tags.is_empty());
    let ok = run_cmd(&mut store, &dir, &["tag", "add", "p2", "urgent", "--yes"]);
    assert_eq!(ok.code, 0);
    assert_eq!(store.prompts[1].tags, vec!["urgent".to_string()]);
    let missing = run_cmd(&mut store, &dir, &["tag", "remove", "p2", "no-such-tag", "--yes"]);
    assert_eq!(missing.code, 2);
    let removed = run_cmd(&mut store, &dir, &["tag", "remove", "p2", "urgent", "--yes"]);
    assert_eq!(removed.code, 0);
    assert!(store.prompts[1].tags.is_empty());
}

#[test]
fn tag_list_counts_every_distinct_tag() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let out = run_cmd(&mut store, &dir, &["tag", "list", "--json"]);
    assert_eq!(out.code, 0);
    let rows: Vec<serde_json::Value> = serde_json::from_str(out.stdout.trim()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["name"], "work");
    assert_eq!(rows[0]["count"], 1);
}

#[test]
fn view_add_and_remove_round_trip_only_with_yes() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let refused = run_cmd(&mut store, &dir, &["view", "add", "Second"]);
    assert_eq!(refused.code, 4);
    assert_eq!(store.settings.views.len(), 1);
    let ok = run_cmd(&mut store, &dir, &["view", "add", "Second", "--yes"]);
    assert_eq!(ok.code, 0);
    assert_eq!(store.settings.views.len(), 2);
    let new_id = ok.stdout.trim().to_string();
    let remove_last = run_cmd(&mut store, &dir, &["view", "remove", "v1", "--yes"]);
    assert_eq!(remove_last.code, 0);
    assert_eq!(store.settings.views.len(), 1);
    // The one remaining view can never be removed.
    let refused_last = run_cmd(&mut store, &dir, &["view", "remove", &new_id, "--yes"]);
    assert_eq!(refused_last.code, 4);
    assert_eq!(store.settings.views.len(), 1);
}

#[test]
fn view_remove_of_an_unknown_id_is_not_found() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let out = run_cmd(&mut store, &dir, &["view", "remove", "ghost", "--yes"]);
    assert_eq!(out.code, 2);
}

// ---- settings ----

#[test]
fn settings_get_reports_not_found_for_an_unknown_key() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let out = run_cmd(&mut store, &dir, &["settings", "get", "not-a-real-key"]);
    assert_eq!(out.code, 2);
}

#[test]
fn the_receiver_secrets_can_neither_be_read_nor_written_here() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    for key in ["remoteTlsKey", "remoteTlsCert", "remoteTlsCa", "remotePasswordHash"] {
        store.settings.ui_texts.insert(key.to_string(), "sealed".to_string());
        let got = run_cmd(&mut store, &dir, &["settings", "get", key]);
        assert_eq!(got.code, 2, "{key} must not be readable");
        assert!(!got.stdout.contains("sealed"));
        let set = run_cmd(&mut store, &dir, &["settings", "set", key, "planted", "--yes"]);
        assert_ne!(set.code, 0, "{key} must not be writable");
        assert_eq!(store.settings.ui_texts.get(key).map(String::as_str), Some("sealed"));
    }
}

#[test]
fn a_refused_write_leaves_the_live_store_as_it_was() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let id = store.prompts[0].id.clone();
    let before = (serde_json::to_string(&store.prompts).unwrap(), serde_json::to_string(&store.trash).unwrap(), serde_json::to_string(&store.settings).unwrap());
    for items in [vec!["delete", id.as_str(), "--yes"], vec!["settings", "set", "myFlag", "true", "--yes"]] {
        let parsed = match parse(&args(&items)).unwrap() {
            Parsed::Run(c) => c,
            Parsed::Help(_) => panic!("help"),
        };
        let out = exec_persisted(&mut store, &dir, &parsed, Path::new("."), |_, _| false);
        assert_eq!(out.code, 3, "{items:?}");
        let after = (serde_json::to_string(&store.prompts).unwrap(), serde_json::to_string(&store.trash).unwrap(), serde_json::to_string(&store.settings).unwrap());
        assert_eq!(after, before, "{items:?} must not change the live store");
    }
    let ok = match parse(&args(&["settings", "set", "myFlag", "true", "--yes"])).unwrap() {
        Parsed::Run(c) => c,
        Parsed::Help(_) => panic!("help"),
    };
    assert_eq!(exec_persisted(&mut store, &dir, &ok, Path::new("."), |_, _| true).code, 0);
    assert_eq!(store.settings.ui_flags.get("myFlag"), Some(&true));
}

#[test]
fn settings_set_ocr_index_off_erases_the_picture_text() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    store.settings.ui_flags.insert("ocrIndex".into(), true);
    store.prompts[0].image_text = "read from a picture".into();
    let set = run_cmd(&mut store, &dir, &["settings", "set", "ocrIndex", "false", "--yes"]);
    assert_eq!(set.code, 0);
    assert!(set.mutated_settings && set.mutated_prompts && set.mutated_trash);
    assert_eq!(store.settings.ui_flags.get("ocrIndex"), Some(&false));
    assert!(store.prompts.iter().all(|p| p.image_text.is_empty()));
}

#[test]
fn settings_set_refuses_non_finite_numbers_and_clamps_the_rest() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    for value in ["NaN", "nan", "inf", "-inf", "infinity", "1e999"] {
        let set = run_cmd(&mut store, &dir, &["settings", "set", "hoverLift", value, "--yes"]);
        assert_ne!(set.code, 0, "{value} must be refused");
        assert!(!set.mutated_settings);
        assert!(!store.settings.ui_values.contains_key("hoverLift"), "{value} was stored");
    }
    let set = run_cmd(&mut store, &dir, &["settings", "set", "importFolderMax", "1e18", "--yes"]);
    assert_eq!(set.code, 0);
    assert_eq!(store.settings.ui_values["importFolderMax"], 2000.0);
    // Stored settings with a non-finite number would not load again; the load path drops it too.
    let mut s = crate::Settings::default();
    s.ui_values.insert("keyWithoutARange".into(), f64::NAN);
    s.migrate();
    assert!(!s.ui_values.contains_key("keyWithoutARange"));
}

#[test]
fn settings_set_refuses_every_key_with_its_own_dedicated_writer() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    for key in [
        "syncDir", "recordingDir", "screenshotDir", "videoExportDir", "secretVaultSalt", "secretVaultVerify",
        "backupPw", "remoteNetworkFilter", "remoteApi", "remoteAllowedRanges", "remotePort", "remoteRateLimit", "remoteTlsShortLived",
        "clipWatcher", "urlSchemeCopyAllowed", "urlScheme", "demoMode", "promptSync",
        "voiceOnlineOk", "dataDirLock", "clipSkipSecrets",
    ] {
        let before = serde_json::to_string(&store.settings).unwrap();
        for value in ["true", "12", "x"] {
            let set = run_cmd(&mut store, &dir, &["settings", "set", key, value, "--yes"]);
            assert_ne!(set.code, 0, "{key} must not be writable here");
            assert!(!set.mutated_settings, "{key} must not flag a settings write");
        }
        assert_eq!(serde_json::to_string(&store.settings).unwrap(), before, "{key} changed the store");
    }
}

#[test]
fn settings_get_and_get_keep_the_vault_material_to_themselves() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    for key in ["secretVaultSalt", "secretVaultVerify", "backupPw"] {
        store.settings.ui_texts.insert(key.to_string(), "sealed-bytes".to_string());
        let got = run_cmd(&mut store, &dir, &["settings", "get", key]);
        assert_ne!(got.code, 0, "{key} must not be readable here");
        assert!(!got.stdout.contains("sealed-bytes"));
    }
    store.prompts[0].secret = true;
    store.prompts[0].text = "CIPHERTEXT".to_string();
    for extra in [&[][..], &["--json"][..]] {
        let mut argv = vec!["get", "p1"];
        argv.extend_from_slice(extra);
        let got = run_cmd(&mut store, &dir, &argv);
        assert_ne!(got.code, 0);
        assert!(!got.stdout.contains("CIPHERTEXT") && !got.stderr.contains("CIPHERTEXT"));
    }
    let plain = run_cmd(&mut store, &dir, &["get", "p2"]);
    assert_eq!(plain.code, 0);
}

#[test]
fn settings_round_trips_a_well_known_and_a_generic_key_only_with_yes() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let refused = run_cmd(&mut store, &dir, &["settings", "set", "theme", "dark"]);
    assert_eq!(refused.code, 4);
    assert_ne!(store.settings.theme, "dark");
    let ok = run_cmd(&mut store, &dir, &["settings", "set", "theme", "dark", "--yes"]);
    assert_eq!(ok.code, 0);
    assert_eq!(store.settings.theme, "dark");
    let got = run_cmd(&mut store, &dir, &["settings", "get", "theme"]);
    assert_eq!(got.stdout.trim(), "dark");

    let _ = run_cmd(&mut store, &dir, &["settings", "set", "myFlag", "true", "--yes"]);
    assert_eq!(store.settings.ui_flags.get("myFlag"), Some(&true));
    let _ = run_cmd(&mut store, &dir, &["settings", "set", "myNumber", "12.5", "--yes"]);
    assert_eq!(store.settings.ui_values.get("myNumber"), Some(&12.5));
    let _ = run_cmd(&mut store, &dir, &["settings", "set", "myText", "hello", "--yes"]);
    assert_eq!(store.settings.ui_texts.get("myText").map(String::as_str), Some("hello"));
}

// ---- export / import: real files, since export writes and import reads one ----

#[test]
fn export_writes_a_file_and_refuses_to_overwrite_without_yes() {
    let tmp = TempDir::new("export");
    let out_path = tmp.path().join("out.json");
    let out_str = out_path.to_string_lossy().to_string();
    let mut store = fixture_store();
    let export_cmd = |confirmed: bool| Cmd::Export { path: out_str.clone(), format: "json".to_string(), ids: None, confirmed };

    let first = exec(&mut store, tmp.path(), &export_cmd(false), Path::new("."));
    assert_eq!(first.code, 0, "a new file needs no --yes: {}", first.stderr);
    assert!(out_path.exists());

    let refused = exec(&mut store, tmp.path(), &export_cmd(false), Path::new("."));
    assert_eq!(refused.code, 4, "an existing file must not be silently overwritten");

    let overwritten = exec(&mut store, tmp.path(), &export_cmd(true), Path::new("."));
    assert_eq!(overwritten.code, 0);
}

#[test]
fn export_relative_paths_resolve_against_the_callers_cwd_not_this_process() {
    let tmp = TempDir::new("export-cwd");
    let mut store = fixture_store();
    let out = exec(&mut store, tmp.path(), &Cmd::Export { path: "rel.json".to_string(), format: "json".to_string(), ids: None, confirmed: false }, tmp.path());
    assert_eq!(out.code, 0);
    assert!(tmp.path().join("rel.json").exists());
}

#[test]
fn import_requires_yes_and_appends_exported_prompts() {
    let tmp = TempDir::new("import");
    let mut store = fixture_store();
    let export_path = tmp.path().join("dump.json");
    let e = exec(&mut store, tmp.path(), &Cmd::Export { path: export_path.to_string_lossy().to_string(), format: "json".to_string(), ids: None, confirmed: false }, Path::new("."));
    assert_eq!(e.code, 0);

    let mut fresh = Store { prompts: Vec::new(), trash: Vec::new(), settings: crate::Settings::default(), prompts_locked: false, written: std::cell::RefCell::new(WrittenRows::new()) };
    let refused = run_cmd(&mut fresh, tmp.path(), &["import", export_path.to_str().unwrap()]);
    assert_eq!(refused.code, 4);
    assert_eq!(fresh.prompts.len(), 0);
    let ok = run_cmd(&mut fresh, tmp.path(), &["import", export_path.to_str().unwrap(), "--yes"]);
    assert_eq!(ok.code, 0);
    assert_eq!(fresh.prompts.len(), 2);
}

#[test]
fn import_of_a_missing_file_fails_cleanly() {
    let tmp = TempDir::new("import-missing");
    let mut store = fixture_store();
    let out = run_cmd(&mut store, tmp.path(), &["import", "does-not-exist.json", "--yes"]);
    assert_ne!(out.code, 0);
}

// ---- backups: real files under a temp "data dir" ----

#[test]
fn backup_create_then_list_finds_the_new_snapshot() {
    let tmp = TempDir::new("backup");
    // do_backup_at requires a real data.db to snapshot.
    let mut conn = db_conn_at(tmp.path()).expect("open store");
    let mut written = WrittenRows::new();
    db_write_prompts(&mut conn, &[], &mut written).ok();
    let store = fixture_store();
    let created = cmd_backup_create(&store, tmp.path(), Some(1), true);
    assert_eq!(created.code, 0, "stderr: {}", created.stderr);
    let listed = cmd_backup_list(tmp.path(), true);
    assert_eq!(listed.code, 0);
    let rows: Vec<serde_json::Value> = serde_json::from_str(listed.stdout.trim()).unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn backup_restore_is_refused_without_yes_at_the_launch_dispatch_level() {
    // The refusal for backup restore happens before either caller (live or
    // standalone) touches a single file -- checked directly on the parsed
    // command here, since exec() itself is never reached for this one.
    let cmd = match parse(&args(&["backup", "restore", "2024-01-01_00-00-00"])).unwrap() {
        Parsed::Run(c) => c,
        Parsed::Help(_) => unreachable!(),
    };
    assert!(!confirmed_of(&cmd));
    assert!(is_mutating(&cmd));
}

// ---- standalone round trip: a real store on disk, via run_standalone ----

// Exercises exactly what run_standalone() does around exec() -- load,
// run, persist by writing straight to the database -- against an explicit
// temp directory instead of run_standalone() itself, which resolves its
// folder from APPDATA/portable detection. Those touch process-global state
// (env vars, the exe's own folder) that a parallel test run must not poke,
// so the directory is passed in here rather than faked through them.
// db_conn_at() runs the schema batch once per data folder, so a fresh TempDir
// per test would get its tables anyway; creating them here first keeps the
// tests that open the file directly independent of that.
fn ensure_test_schema(dir: &Path) {
    let conn = rusqlite::Connection::open(dir.join("data.db")).expect("open db for test schema");
    assert!(crate::db_conn_init_schema(&conn), "schema init failed for test directory");
}

fn standalone_roundtrip(dir: &Path, items: &[&str]) -> (i32, String) {
    ensure_test_schema(dir);
    let store = load_store_headless(dir);
    let parsed = parse(&args(items)).unwrap();
    let cmd = match parsed {
        Parsed::Run(c) => c,
        Parsed::Help(t) => return (0, t),
    };
    let mut store = store;
    let outcome = exec(&mut store, dir, &cmd, Path::new("."));
    if outcome.mutated_prompts || outcome.mutated_trash || outcome.mutated_settings {
        let mut conn = db_conn_at(dir).expect("open store");
        // store.written, not a fresh map -- see the matching comment in
        // run_standalone(). A fresh map here silently turned every delete
        // into a no-op on disk, caught by
        // standalone_delete_then_restore_round_trips_through_the_trash_table.
        if outcome.mutated_trash {
            db_write_prompts_and_trash(&mut conn, &store.prompts, &store.trash, &mut store.written.borrow_mut()).unwrap();
        } else if outcome.mutated_prompts {
            db_write_prompts(&mut conn, &store.prompts, &mut store.written.borrow_mut()).unwrap();
        }
        if outcome.mutated_settings {
            db_write_settings(&conn, &store.settings).unwrap();
        }
    }
    (outcome.code, outcome.stdout)
}

#[test]
fn standalone_add_persists_to_disk_for_the_next_load() {
    let tmp = TempDir::new("standalone-add");
    let (code, stdout) = standalone_roundtrip(tmp.path(), &["add", "--name", "Solo", "--text", "hi", "--yes"]);
    assert_eq!(code, 0);
    let new_id = stdout.trim().to_string();

    // A fresh load_store_headless() call re-opens the same data.db --
    // proving the write really reached disk, not just this run's Store.
    let reloaded = load_store_headless(tmp.path());
    assert!(reloaded.prompts.iter().any(|p| p.id == new_id && p.name == "Solo"));
}

#[test]
fn standalone_delete_then_restore_round_trips_through_the_trash_table() {
    let tmp = TempDir::new("standalone-delete");
    let (add_code, stdout) = standalone_roundtrip(tmp.path(), &["add", "--name", "Solo", "--text", "hi", "--yes"]);
    assert_eq!(add_code, 0);
    let id = stdout.trim().to_string();

    let (del_code, _) = standalone_roundtrip(tmp.path(), &["delete", &id, "--yes"]);
    assert_eq!(del_code, 0);
    let after_delete = load_store_headless(tmp.path());
    assert!(!after_delete.prompts.iter().any(|p| p.id == id));
    assert!(after_delete.trash.iter().any(|p| p.id == id));

    let (restore_code, _) = standalone_roundtrip(tmp.path(), &["restore", &id, "--yes"]);
    assert_eq!(restore_code, 0);
    let after_restore = load_store_headless(tmp.path());
    assert!(after_restore.prompts.iter().any(|p| p.id == id));
    assert!(!after_restore.trash.iter().any(|p| p.id == id));
}

#[test]
fn standalone_restore_clears_picture_text_when_the_index_ended_up_off() {
    let tmp = TempDir::new("standalone-ocr-off");
    let (code, stdout) = standalone_roundtrip(tmp.path(), &["add", "--name", "Solo", "--text", "hi", "--yes"]);
    assert_eq!(code, 0);
    let id = stdout.trim().to_string();
    let store = load_store_headless(tmp.path());
    let mut prompts = store.prompts.clone();
    prompts.iter_mut().find(|p| p.id == id).unwrap().image_text = "read from a picture".into();
    {
        let mut conn = db_conn_at(tmp.path()).unwrap();
        db_write_prompts(&mut conn, &prompts, &mut store.written.borrow_mut()).unwrap();
    }
    // The restored settings have the index off (the default).
    clear_ocr_text_after_restore(tmp.path(), false).unwrap();
    let kept = load_store_headless(tmp.path());
    assert_eq!(kept.prompts.iter().find(|p| p.id == id).unwrap().image_text, "read from a picture");
    clear_ocr_text_after_restore(tmp.path(), true).unwrap();
    let cleared = load_store_headless(tmp.path());
    assert!(cleared.prompts.iter().all(|p| p.image_text.is_empty()));
}

#[test]
fn run_standalone_reports_bad_invocation_as_exit_one() {
    let code = run_standalone(&args(&["not-a-command"]), Path::new("."));
    assert_eq!(code, 1);
}

#[test]
fn run_standalone_prints_help_and_exits_zero() {
    let code = run_standalone(&args(&["--help"]), Path::new("."));
    assert_eq!(code, 0);
}

// ---- pipe framing ----

#[test]
fn frame_round_trips_through_an_in_memory_duplex_pipe() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_io().enable_time().build().unwrap();
    rt.block_on(async {
        let (mut a, mut b) = tokio::io::duplex(4096);
        let payload = b"{\"argv\":[\"list\"],\"cwd\":\"C:/\"}".to_vec();
        write_frame(&mut a, &payload).await.unwrap();
        let got = read_frame(&mut b, MAX_FRAME_BYTES).await.unwrap();
        assert_eq!(got, payload);
    });
}

#[test]
fn oversized_frame_is_rejected_instead_of_allocated() {
    use tokio::io::AsyncWriteExt;
    let rt = tokio::runtime::Builder::new_current_thread().enable_io().enable_time().build().unwrap();
    rt.block_on(async {
        let (mut a, mut b) = tokio::io::duplex(64);
        // Only the length header is written, never the claimed 100-byte body --
        // proving read_frame rejects on the header alone instead of waiting on
        // (or allocating for) bytes that were never sent.
        a.write_all(&100u32.to_le_bytes()).await.unwrap();
        let err = read_frame(&mut b, 10).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    });
}

// ---- remote-caller path confinement ----

#[test]
fn export_inside_the_data_folder_is_allowed() {
    let dir = TempDir::new("confine-ok");
    let argv = args(&["export", "out.json", "--yes"]);
    assert!(confine_path_arg(&argv, dir.path()).is_ok());
}

#[test]
fn export_escaping_the_data_folder_via_dotdot_is_refused() {
    let dir = TempDir::new("confine-escape");
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    let argv = args(&["export", "../outside.json", "--yes"]);
    let inner = dir.path().join("sub");
    assert!(confine_path_arg(&argv, &inner).is_err());
}

#[test]
fn export_with_an_absolute_path_outside_the_data_folder_is_refused() {
    let dir = TempDir::new("confine-abs");
    let outside = TempDir::new("confine-abs-outside");
    let abs = outside.path().join("out.json");
    let argv = args(&["export", abs.to_str().unwrap(), "--yes"]);
    assert!(confine_path_arg(&argv, dir.path()).is_err());
}

#[test]
fn import_from_inside_the_data_folder_is_allowed() {
    let dir = TempDir::new("confine-import-ok");
    let file = dir.path().join("in.json");
    std::fs::write(&file, "[]").unwrap();
    let argv = args(&["import", "in.json", "--yes"]);
    assert!(confine_path_arg(&argv, dir.path()).is_ok());
}

#[test]
fn import_escaping_the_data_folder_is_refused() {
    let dir = TempDir::new("confine-import-escape");
    let outside = TempDir::new("confine-import-outside");
    let file = outside.path().join("in.json");
    std::fs::write(&file, "[]").unwrap();
    let argv = args(&["import", file.to_str().unwrap(), "--yes"]);
    assert!(confine_path_arg(&argv, dir.path()).is_err());
}

#[test]
fn a_non_path_command_is_never_confined() {
    let dir = TempDir::new("confine-noop");
    assert!(confine_path_arg(&args(&["list"]), dir.path()).is_ok());
    assert!(confine_path_arg(&args(&["get", "p1"]), dir.path()).is_ok());
}

// ---- `describe` / docs/automation-manifest.json ----

#[test]
fn describe_is_recognized_as_cli_and_parses_with_no_arguments() {
    assert!(looks_like_cli(&args(&["describe"])));
    assert!(matches!(parse(&args(&["describe"])), Ok(Parsed::Run(Cmd::Describe))));
}

#[test]
fn describe_json_is_valid_json_listing_every_manifest_entry_once() {
    let parsed: serde_json::Value = serde_json::from_str(&describe_json()).expect("describe output must be valid JSON");
    let commands = parsed["commands"].as_array().expect("commands must be an array");
    assert_eq!(commands.len(), MANIFEST_ENTRIES.len());
    assert_eq!(parsed["commands"][0]["name"], "list");
    assert!(parsed["exit_codes"]["4"].as_str().unwrap().contains("refused"));
}

// The path docs/gen_automation_doc.py and docs/automation_doc_check.py
// also read (relative to the crate root, two levels up from src-tauri).
fn manifest_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("docs").join("automation-manifest.json")
}

// A command added to MANIFEST_ENTRIES without regenerating
// docs/automation-manifest.json (and, downstream, docs/automation.md)
// makes this fail immediately, inside the ordinary `cargo test --lib`
// run -- nobody has to remember to run the doc check separately.
#[test]
fn automation_manifest_json_matches_the_generated_source() {
    let expected = serde_json::to_value(command_manifest()).unwrap();
    let on_disk = std::fs::read_to_string(manifest_fixture_path()).expect("docs/automation-manifest.json must exist");
    let on_disk: serde_json::Value = serde_json::from_str(&on_disk).expect("docs/automation-manifest.json must be valid JSON");
    assert_eq!(
        on_disk, expected,
        "docs/automation-manifest.json is stale -- regenerate it with \
         `cargo test --lib -- --ignored regenerate_automation_manifest_json`, then rerun \
         `python tests/gen_automation_doc.py`"
    );
}

// Not run by the ordinary suite (`cargo test --lib` skips `#[ignore]`d
// tests) -- this is the "kleines Werkzeug" half that writes the fixture;
// `tests/gen_automation_doc.py` is the half that turns it into markdown.
#[test]
#[ignore]
fn regenerate_automation_manifest_json() {
    let json = serde_json::to_string_pretty(&command_manifest()).unwrap();
    std::fs::write(manifest_fixture_path(), format!("{json}\n")).expect("failed to write docs/automation-manifest.json");
}

// ---- docs/automation.md's four "Examples" really run, at this same
// cargo-test level, and their shape matches what the doc claims. Each
// test's name matches the doc's own numbered heading. ----

#[test]
fn docs_example_1_add_matches_the_manual() {
    let dir = TempDir::new("docs-ex1");
    let mut store = fixture_store();
    let out = run_cmd(&mut store, dir.path(), &["add", "--name", "Release checklist", "--text", "1. Bump version...", "--tags", "work,release", "--yes", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let body: serde_json::Value = serde_json::from_str(out.stdout.trim()).expect("add --json must print JSON");
    assert!(body["id"].as_str().is_some_and(|id| !id.is_empty()), "add --json must print {{\"id\": \"...\"}} as documented");
    assert_eq!(body.as_object().unwrap().len(), 1, "add --json's object has exactly one field, id");
}

#[test]
fn docs_example_2_search_matches_the_manual() {
    let dir = TempDir::new("docs-ex2");
    let mut store = fixture_store();
    let added = run_cmd(&mut store, dir.path(), &["add", "--name", "Release checklist", "--text", "1. Bump version...", "--tags", "work,release", "--yes", "--json"]);
    let id: String = serde_json::from_str::<serde_json::Value>(added.stdout.trim()).unwrap()["id"].as_str().unwrap().to_string();
    let out = run_cmd(&mut store, dir.path(), &["search", "release", "--tag", "work", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let rows: Vec<serde_json::Value> = serde_json::from_str(out.stdout.trim()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], id);
    assert_eq!(rows[0]["name"], "Release checklist");
    assert_eq!(rows[0]["tags"], serde_json::json!(["work", "release"]));
}

#[test]
fn docs_example_3_fill_a_placeholder_and_copy_matches_the_manual() {
    let dir = TempDir::new("docs-ex3");
    let mut store = fixture_store();
    let added = run_cmd(&mut store, dir.path(), &["add", "--name", "Release checklist", "--text", "Version: {{version}}", "--tags", "work", "--yes", "--json"]);
    let id: String = serde_json::from_str::<serde_json::Value>(added.stdout.trim()).unwrap()["id"].as_str().unwrap().to_string();
    let got = run_cmd(&mut store, dir.path(), &["get", &id, "--json"]);
    assert_eq!(got.code, 0);
    let detail: serde_json::Value = serde_json::from_str(got.stdout.trim()).unwrap();
    assert_eq!(detail["text"], "Version: {{version}}");
    let edited = run_cmd(&mut store, dir.path(), &["edit", &id, "--text", "Version: 2.7.0", "--yes"]);
    assert_eq!(edited.code, 0, "stderr: {}", edited.stderr);
    let refetched = run_cmd(&mut store, dir.path(), &["get", &id, "--json"]);
    let detail2: serde_json::Value = serde_json::from_str(refetched.stdout.trim()).unwrap();
    assert_eq!(detail2["text"], "Version: 2.7.0", "edit --text must have replaced the placeholder before copy");
    let copied = run_cmd(&mut store, dir.path(), &["copy", &id]);
    assert_eq!(copied.code, 0, "stderr: {}", copied.stderr);
}

#[test]
fn docs_example_4_backup_create_matches_the_manual() {
    let dir = TempDir::new("docs-ex4");
    let mut conn = db_conn_at(dir.path()).expect("open store");
    let mut written = WrittenRows::new();
    db_write_prompts(&mut conn, &[], &mut written).ok();
    let store = fixture_store();
    let out = cmd_backup_create(&store, dir.path(), Some(10), true);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let body: serde_json::Value = serde_json::from_str(out.stdout.trim()).expect("backup create --json must print JSON");
    assert!(body["name"].as_str().is_some_and(|n| !n.is_empty()), "backup create --json must print {{\"name\": \"...\"}} as documented");
    assert_eq!(body.as_object().unwrap().len(), 1, "backup create --json's object has exactly one field, name");
}

// Taking a flag out of the argument list must not leak a copy of the rest: the
// running app parses a command for every pipe and receiver request. No red
// exists for this as a behaviour (a counting allocator is flaky under parallel
// tests), so the test pins the shape: Args owns a Vec and the leak is gone.
#[test]
fn taking_a_flag_does_not_leak_the_remaining_arguments() {
    let source = include_str!("cli.rs");
    let needle = ["Box", "::", "leak"].concat();
    assert!(!source.contains(&needle), "cli.rs must not leak argument lists");
    let a = Args::new(&args(&["list", "--json", "--tag", "x"]));
    let _owned: &Vec<String> = &a.items;
    let mut a = Args::new(&args(&["--json", "--tag", "x", "rest"]));
    assert!(take_flag(&mut a, "--json"));
    assert_eq!(take_flag_value(&mut a, "--tag").unwrap().as_deref(), Some("x"));
    assert_eq!(a.items, vec!["rest".to_string()]);
}
// ---- an old placeholder spelling is upgraded on write ----

#[test]
fn add_and_edit_store_the_new_placeholder_spelling() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    let out = run_cmd(&mut store, &dir, &["add", "--name", "Old", "--text", "Hi {#{name}#} on {#{/date\\}#}", "--yes", "--json"]);
    assert_eq!(out.code, 0);
    let added = store.prompts.last().unwrap();
    assert_eq!(added.text, "Hi {?name} on {?@date}");
    let out = run_cmd(&mut store, &dir, &["edit", "p1", "--text", "Dear {#{who=you}#}", "--yes"]);
    assert_eq!(out.code, 0);
    let edited = store.prompts.iter().find(|p| p.id == "p1").unwrap();
    assert_eq!(edited.text, "Dear {?who=you}");
}

#[test]
fn copy_refuses_a_confidential_prompt_like_get() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    store.prompts[0].secret = true;
    store.prompts[0].text = "CIPHERTEXT".to_string();
    let id = store.prompts[0].id.clone();
    let out = run_cmd(&mut store, &dir, &["copy", &id]);
    assert_eq!(out.code, 1);
    assert_eq!(run_cmd(&mut store, &dir, &["get", &id]).code, 1);
}

#[test]
fn edit_refuses_new_text_for_a_confidential_prompt() {
    let mut store = fixture_store();
    let dir = std::env::temp_dir();
    store.prompts[0].secret = true;
    store.prompts[0].text = "CIPHERTEXT".to_string();
    let id = store.prompts[0].id.clone();
    let out = run_cmd(&mut store, &dir, &["edit", &id, "--text", "plain", "--yes"]);
    assert_ne!(out.code, 0);
    assert_eq!(store.prompts[0].text, "CIPHERTEXT");
    let out = run_cmd(&mut store, &dir, &["edit", &id, "--name", "Renamed", "--yes"]);
    assert_eq!(out.code, 0);
    assert_eq!(store.prompts[0].name, "Renamed");
}
