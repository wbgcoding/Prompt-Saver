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
