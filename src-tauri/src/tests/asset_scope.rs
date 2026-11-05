use super::*;
use std::collections::HashSet;

fn prompt_with(file: &str, icon: &str) -> Prompt {
    Prompt { file_path: file.to_string(), icon_path: icon.to_string(), ..Default::default() }
}

// The static scope allows nothing: every folder and file a window shows is allowed at run time
// by the app, so a subverted page cannot ask for an arbitrary path on the disk.
#[test]
fn the_static_asset_scope_allows_nothing_and_denies_the_sidecars() {
    fn wild(pat: &str, name: &str) -> bool {
        match pat.split_once('*') {
            None => pat == name,
            Some((head, tail)) => {
                name.strip_prefix(head).is_some_and(|rest| (0..=rest.len()).any(|i| rest.is_char_boundary(i) && wild(tail, &rest[i..])))
            }
        }
    }
    fn covers(pattern: &str, path: &str) -> bool {
        let Some(rest) = pattern.strip_prefix("**/") else { return false };
        if let Some(dir) = rest.strip_suffix("/**") {
            path.split('/').any(|seg| seg == dir)
        } else {
            path.rsplit('/').next().is_some_and(|name| wild(rest, name))
        }
    }
    let conf: serde_json::Value = serde_json::from_str(include_str!("../../tauri.conf.json")).expect("tauri.conf.json parses");
    let scope = &conf["app"]["security"]["assetProtocol"]["scope"];
    assert!(scope["allow"].as_array().expect("an allow list").is_empty(), "no path is allowed statically");
    let deny: Vec<&str> = scope["deny"].as_array().expect("a deny list").iter().filter_map(|v| v.as_str()).collect();
    for path in [
        "D:/Videos/Prompt Saver/rec.keys.json",
        "D:/Videos/Prompt Saver/rec.cam.json",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/keys.json",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/cam.json",
    ] {
        assert!(deny.iter().any(|p| covers(p, path)), "asset scope must deny {path}");
    }
}

#[test]
fn media_dirs_lists_data_recording_and_temp_once_without_empties() {
    let data = PathBuf::from("D:/data");
    let temp = PathBuf::from("D:/tmp");
    let dirs = crate::asset_scope::media_dirs(&data, "E:/takes", &temp);
    assert_eq!(dirs, vec![data.clone(), PathBuf::from("E:/takes"), temp.join(brand::TEMP_DIR)]);
    // A recording folder equal to the data folder is not listed twice.
    let same = crate::asset_scope::media_dirs(&data, "D:/data", &temp);
    assert_eq!(same, vec![data.clone(), temp.join(brand::TEMP_DIR)]);
    // An empty data folder is dropped.
    let none = crate::asset_scope::media_dirs(Path::new(""), "E:/takes", &temp);
    assert_eq!(none, vec![PathBuf::from("E:/takes"), temp.join(brand::TEMP_DIR)]);
}

#[test]
fn prompt_asset_paths_skips_empty_repeated_and_refused_paths() {
    let prompts = vec![
        prompt_with("D:/a.png", "D:/icon.png"),
        prompt_with("D:/a.png", "  "),
        prompt_with("", ""),
        prompt_with("//host/share/x.png", "D:/b.png"),
    ];
    let refused = |raw: &str| raw.starts_with("//");
    let got = crate::asset_scope::prompt_asset_paths(&prompts, &refused);
    assert_eq!(got, vec!["D:/a.png", "D:/icon.png", "D:/b.png"]);
}

#[test]
fn first_sighting_reports_a_key_only_once() {
    let mut seen: HashSet<String> = HashSet::new();
    assert!(crate::asset_scope::first_sighting(&mut seen, "k".to_string()));
    assert!(!crate::asset_scope::first_sighting(&mut seen, "k".to_string()));
    assert!(crate::asset_scope::first_sighting(&mut seen, "other".to_string()));
}

fn scratch_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ps-scope-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn touch(path: &Path) -> String {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent");
    }
    fs::write(path, b"x").expect("file");
    path.to_string_lossy().to_string()
}

// An attachment source the renderer names is trusted only from the app's own folders, from a
// file the user picked in a dialog this session, or when a stored prompt already holds it.
#[test]
fn attachment_source_trust_needs_an_app_folder_a_pick_or_a_stored_path() {
    let base = scratch_dir("trust");
    let root = base.join("data");
    let roots = vec![root.clone()];
    let none: HashSet<String> = HashSet::new();
    let inside = touch(&root.join("files").join("a.png"));
    let outside = touch(&base.join("Documents").join("secret.pdf"));
    assert!(crate::asset_scope::attachment_source_trusted("", &roots, &none));
    assert!(crate::asset_scope::attachment_source_trusted(&inside, &roots, &none));
    assert!(!crate::asset_scope::attachment_source_trusted(&outside, &roots, &none));
    // A climb out of an allowed folder is not inside it.
    let climbing = root.join("..").join("Documents").join("secret.pdf").to_string_lossy().to_string();
    assert!(!crate::asset_scope::attachment_source_trusted(&climbing, &roots, &none));
    // A path a stored prompt already holds stays usable.
    let stored: HashSet<String> = [crate::asset_scope::stored_key(&outside)].into_iter().collect();
    assert!(crate::asset_scope::attachment_source_trusted(&outside, &roots, &stored));
    // A dialog pick is proof; a different file next to it is not.
    let picked = touch(&base.join("Pictures").join("logo.png"));
    let sibling = touch(&base.join("Pictures").join("other.png"));
    assert!(!crate::asset_scope::attachment_source_trusted(&picked, &roots, &none));
    crate::asset_scope::grant_picked_file(&picked);
    assert!(crate::asset_scope::attachment_source_trusted(&picked, &roots, &none));
    assert!(!crate::asset_scope::attachment_source_trusted(&sibling, &roots, &none));
    // A file this session's export wrote, in a folder picked once for it, is proof too.
    let exported = touch(&base.join("Exports").join("clip.mp4"));
    assert!(!crate::asset_scope::attachment_source_trusted(&exported, &roots, &none));
    crate::video_editor::remember_export_output(Path::new(&exported));
    assert!(crate::asset_scope::attachment_source_trusted(&exported, &roots, &none));
    let _ = fs::remove_dir_all(&base);
}

// Same bytes, other spelling of the folder: a case change or a short path does not slip past.
#[test]
fn attachment_source_trust_compares_resolved_paths() {
    let base = scratch_dir("resolved");
    let root = base.join("Data");
    let inside = touch(&root.join("x.png"));
    let none: HashSet<String> = HashSet::new();
    let upper = inside.to_uppercase();
    if Path::new(&upper).exists() {
        assert!(crate::asset_scope::attachment_source_trusted(&upper, std::slice::from_ref(&root), &none));
    }
    let _ = fs::remove_dir_all(&base);
}

// The scope is widened after the write went through, never for a refused or failed one.
#[test]
fn the_scope_widens_only_after_a_write_succeeded() {
    let src = include_str!("../store.rs").replace("\r\n", "\n");
    for name in ["pub(crate) fn save_prompts(", "pub(crate) fn save_prompts_and_trash(", "pub(crate) fn write_settings_outcome("] {
        let start = src.find(name).unwrap_or_else(|| panic!("{name} exists"));
        let body = &src[start..];
        let body = &body[..body.find("\n}\n").expect("function end")];
        let blocked = body.find("data_dir_write_blocked(app)").unwrap_or_else(|| panic!("{name} checks the read-only gate"));
        let widen = body.find("asset_scope::allow_").unwrap_or_else(|| panic!("{name} widens the scope"));
        assert!(widen > blocked, "{name} must widen the scope only after the read-only check");
        let first_return = body.find("return false").unwrap_or(usize::MAX).min(body.find("SettingsWrite::Refused").unwrap_or(usize::MAX));
        assert!(widen > first_return, "{name} must not widen the scope before a refusal returns");
    }
}

// The renderer never names an arbitrary local file for an attachment: the commands that take
// one vet it, and every backend door that hands a path to the user records it as picked.
#[test]
fn attachment_commands_vet_sources_and_pickers_record_grants() {
    let prompts = include_str!("../prompts.rs").replace("\r\n", "\n");
    for name in ["pub(crate) async fn add_prompt(", "pub(crate) async fn update_prompt("] {
        let start = prompts.find(name).unwrap_or_else(|| panic!("{name} exists"));
        let body = &prompts[start..];
        let body = &body[..body.find("\n}\n").expect("function end")];
        assert!(body.contains("source_trusted_now"), "{name} must vet attachment sources");
    }
    let images = include_str!("../images.rs").replace("\r\n", "\n");
    assert_eq!(images.matches("asset_scope::grant_picked_file").count(), 3, "both pickers and the clipboard file record the pick");
    assert!(include_str!("../main_window.rs").contains("DragDropEvent::Drop"), "a file dropped on the main window is recorded as picked");
}

// A saved editor state re-allows exactly the media files its overlays and clips name.
#[test]
fn edit_state_asset_paths_lists_only_existing_media_files_of_overlays_and_clips() {
    let base = scratch_dir("editstate");
    let logo = touch(&base.join("Branding").join("logo.png"));
    let take = touch(&base.join("Branding").join("take.MP4"));
    let secret = touch(&base.join("Documents").join("secret.pdf"));
    let missing = base.join("Branding").join("gone.png").to_string_lossy().to_string();
    let ops = serde_json::json!({
        "v": 1,
        "ops": [
            { "tool": "overlay", "kind": "image", "path": logo },
            { "tool": "clip", "path": take },
            { "tool": "overlay", "kind": "image", "path": logo },
            { "tool": "overlay", "kind": "image", "path": secret },
            { "tool": "overlay", "kind": "image", "path": missing },
            { "tool": "overlay", "kind": "image", "path": "//host/share/x.png" },
            { "tool": "overlay", "kind": "image", "path": base.join("Branding").to_string_lossy() },
            { "tool": "trim", "path": logo, "start": 0 },
            { "tool": "overlay", "kind": "image" }
        ]
    })
    .to_string();
    let got = crate::drafts::edit_state_asset_paths(&ops, &[]);
    assert_eq!(got, vec![logo.clone(), take.clone()]);
    assert!(crate::drafts::edit_state_asset_paths("not json", &[]).is_empty());
    assert!(crate::drafts::edit_state_asset_paths("{\"v\":1}", &[]).is_empty());
    let _ = fs::remove_dir_all(&base);
}

// The editor asks for the re-allow only after the user continued with the saved state.
#[test]
fn restoring_a_saved_editor_state_re_allows_its_files_first() {
    let src = include_str!("../../../ui/videoLoad.js").replace("\r\n", "\n");
    let ask = src.find("askRestoreEditState(parsed.ops.length)").expect("the restore question");
    let allow = src.find("allow_edit_state_files").expect("the re-allow call");
    let apply = src.find("VE.ops = parsed.ops").expect("the restore");
    assert!(ask < allow && allow < apply, "the files are allowed after the answer and before the ops are applied");
    assert!(include_str!("../lib.rs").contains("drafts::allow_edit_state_files"), "the command is registered");
}

fn imported(name: &str, file: &str, icon: &str) -> crate::import_export::ImportedPrompt {
    crate::import_export::ImportedPrompt {
        name: name.to_string(),
        text: format!("body of {name}"),
        color: String::new(),
        font: String::new(),
        font_size: 0,
        file_path: file.to_string(),
        pdf_page: 0,
        icon_path: icon.to_string(),
        caption: String::new(),
        caption_size: 0,
        show_image: false,
        copy_image: false,
        image: String::new(),
        tags: Vec::new(),
        notes: String::new(),
        variants: Vec::new(),
        rotate_mode: String::new(),
        favorite: false,
        secret: false,
    }
}

fn empty_store() -> Store {
    Store { prompts: Vec::new(), trash: Vec::new(), settings: Settings::default(), prompts_locked: false, written: Default::default() }
}

// A prompt file naming a local file outside the app's folders keeps the path, but the path is
// marked untrusted: the scope, the copy and the stored set all leave it out. A path in an app
// folder, or one a stored prompt already held, stays trusted.
#[test]
fn an_import_marks_a_foreign_attachment_path_untrusted() {
    let base = scratch_dir("import-mark");
    let root = base.join("data");
    let roots = vec![root.clone()];
    let own = touch(&root.join("files").join("own.png"));
    let foreign = touch(&base.join("Documents").join("secret.pdf"));
    let known = touch(&base.join("Pictures").join("known.png"));
    let mut store = empty_store();
    store.prompts.push(prompt_with(&known, ""));
    let items = vec![
        imported("a", &foreign, &own),
        imported("b", &known, ""),
        imported("c", "relative/x.png", ""),
        imported("d", "//host/share/x.png", ""),
    ];
    assert_eq!(crate::import_export::append_imported_prompts(&mut store, items, &roots), 4);
    let marked = &store.settings.untrusted_paths;
    assert!(marked.contains(&crate::asset_scope::stored_key(&foreign)));
    assert!(!marked.contains(&crate::asset_scope::stored_key(&own)), "an app folder path stays trusted");
    assert!(!marked.contains(&crate::asset_scope::stored_key(&known)), "a path already stored stays trusted");
    assert!(marked.contains(&crate::asset_scope::stored_key("relative/x.png")));
    assert_eq!(store.prompts[1].file_path, foreign, "the path itself is kept");
    assert!(crate::asset_scope::path_untrusted(&store.settings, &foreign));
    assert!(!crate::asset_scope::path_untrusted(&store.settings, &own));
    // The stored set the next trust check uses no longer vouches for the marked path.
    let trusted = crate::asset_scope::trusted_stored_keys(&store);
    assert!(!trusted.contains(&crate::asset_scope::stored_key(&foreign)));
    assert!(trusted.contains(&crate::asset_scope::stored_key(&own)));
    assert!(trusted.contains(&crate::asset_scope::stored_key(&known)));
    assert!(!crate::asset_scope::attachment_source_trusted(&foreign, &roots, &trusted));
    let _ = fs::remove_dir_all(&base);
}

// The mark survives a settings round trip and goes once the user hands the file over again.
#[test]
fn a_re_pick_clears_the_untrusted_mark_and_the_mark_is_saved() {
    let base = scratch_dir("repick");
    let foreign = touch(&base.join("Documents").join("manual.pdf"));
    let mut store = empty_store();
    crate::import_export::append_imported_prompts(&mut store, vec![imported("a", &foreign, "")], &[base.join("data")]);
    assert!(crate::asset_scope::path_untrusted(&store.settings, &foreign));
    let saved = serde_json::to_string(&store.settings).expect("settings serialize");
    let loaded: Settings = serde_json::from_str(&saved).expect("settings load");
    assert!(crate::asset_scope::path_untrusted(&loaded, &foreign), "the mark survives a restart");
    // Saving the prompt without picking the file again changes nothing.
    assert!(!crate::asset_scope::trust_picked_paths(&mut store.settings, &[&foreign, ""]));
    assert!(crate::asset_scope::path_untrusted(&store.settings, &foreign));
    crate::asset_scope::grant_picked_file(&foreign);
    assert!(crate::asset_scope::trust_picked_paths(&mut store.settings, &[&foreign, ""]));
    assert!(!crate::asset_scope::path_untrusted(&store.settings, &foreign));
    assert!(store.settings.untrusted_paths.is_empty());
    // A default settings value writes no key at all.
    assert!(!serde_json::to_string(&Settings::default()).expect("serialize").contains("untrusted_paths"));
    let _ = fs::remove_dir_all(&base);
}

// A mark nothing holds any more is dropped; one a prompt or a trash entry still holds stays.
#[test]
fn untrusted_marks_are_pruned_once_nothing_holds_the_path() {
    let mut store = empty_store();
    store.prompts.push(prompt_with("D:/Elsewhere/a.png", ""));
    let mut trashed = prompt_with("", "D:/Elsewhere/b.png");
    trashed.trashed_at = 1;
    store.trash.push(trashed);
    for raw in ["D:/Elsewhere/a.png", "D:/Elsewhere/b.png", "D:/Elsewhere/gone.png"] {
        store.settings.untrusted_paths.insert(crate::asset_scope::stored_key(raw));
    }
    assert!(crate::asset_scope::prune_untrusted(&mut store));
    assert_eq!(store.settings.untrusted_paths.len(), 2);
    assert!(!store.settings.untrusted_paths.contains(&crate::asset_scope::stored_key("D:/Elsewhere/gone.png")));
    assert!(!crate::asset_scope::prune_untrusted(&mut store), "nothing left to drop");
    store.prompts.clear();
    assert!(crate::asset_scope::prune_untrusted(&mut store));
    assert_eq!(store.settings.untrusted_paths.len(), 1, "the trash entry still holds its path");
}

// A prompt the synced folder brings in is judged the same way, against what the store trusted
// before the round.
#[test]
fn a_synced_prompt_with_a_foreign_attachment_path_is_marked_untrusted() {
    let base = scratch_dir("sync-mark");
    let foreign = touch(&base.join("Documents").join("sheet.pdf"));
    let mut incoming = sync_test_prompt();
    incoming.id = "p9".into();
    incoming.file_path = foreign.clone();
    let content = prompt_to_md(&incoming, &SyncPlacement::default());
    let mut store = sync_apply_store(Vec::new(), Vec::new());
    store.settings.views.push(super::hotkeys_2::test_view("v1"));
    store.settings.active_view = "v1".to_string();
    let (changed, settings_changed) = crate::md_sync::sync_poll_apply(
        &mut store,
        vec![("p9".to_string(), content, true)],
        &[base.join("data")],
        &mut |_, _, _| {},
    );
    assert!(changed && settings_changed);
    assert_eq!(store.prompts[0].file_path, foreign);
    assert!(crate::asset_scope::path_untrusted(&store.settings, &foreign));
    let _ = fs::remove_dir_all(&base);
}

// The marks belong to this machine: a settings restore from a backup keeps the local ones.
#[test]
fn a_settings_restore_keeps_the_local_untrusted_marks() {
    let mut local = Settings::default();
    local.untrusted_paths.insert("f\u{1}d:/x.png".to_string());
    let mut incoming = Settings::default();
    incoming.untrusted_paths.insert("f\u{1}d:/other.png".to_string());
    crate::vault::keep_machine_settings(&local, &mut incoming);
    assert_eq!(incoming.untrusted_paths, local.untrusted_paths);
}
