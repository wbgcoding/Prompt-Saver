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
        assert!(crate::asset_scope::attachment_source_trusted(&upper, &[root.clone()], &none));
    }
    let _ = fs::remove_dir_all(&base);
}

// The scope is widened after the write went through, never for a refused or failed one.
#[test]
fn the_scope_widens_only_after_a_write_succeeded() {
    let src = include_str!("../store.rs");
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
    let prompts = include_str!("../prompts.rs");
    for name in ["pub(crate) async fn add_prompt(", "pub(crate) async fn update_prompt("] {
        let start = prompts.find(name).unwrap_or_else(|| panic!("{name} exists"));
        let body = &prompts[start..];
        let body = &body[..body.find("\n}\n").expect("function end")];
        assert!(body.contains("source_trusted_now"), "{name} must vet attachment sources");
    }
    let images = include_str!("../images.rs");
    assert_eq!(images.matches("asset_scope::grant_picked_file").count(), 3, "both pickers and the clipboard file record the pick");
    assert!(include_str!("../main_window.rs").contains("DragDropEvent::Drop"), "a file dropped on the main window is recorded as picked");
}
