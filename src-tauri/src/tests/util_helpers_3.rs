// Pure helpers, third part: capture pixel checks, clipboard HTML offsets, shuffle bags, the
// variable window width, recording folder file rules, grid placement and folder
// normalising, and the small store codecs.

use super::hotkeys_2::{test_tile_folder, test_view};
use super::util_helpers::util_dir;
use super::*;

// ---------- capture ----------

fn solid(w: u32, h: u32, rgb: [u8; 3]) -> image::RgbaImage {
    image::RgbaImage::from_pixel(w, h, image::Rgba([rgb[0], rgb[1], rgb[2], 255]))
}

#[test]
fn luma_range_of_an_empty_image_is_none() {
    assert_eq!(luma_range(&image::RgbaImage::new(0, 5)), None);
    assert_eq!(luma_range(&image::RgbaImage::new(5, 0)), None);
    assert_eq!(luma_range(&image::RgbaImage::new(0, 0)), None);
}

#[test]
fn luma_range_of_a_solid_image_is_one_value() {
    assert_eq!(luma_range(&solid(10, 10, [90, 90, 90])), Some((90, 90)));
    // The three channels are averaged with integer division: (255 + 0 + 0) / 3 = 85.
    assert_eq!(luma_range(&solid(10, 10, [255, 0, 0])), Some((85, 85)));
    assert_eq!(luma_range(&solid(1, 1, [30, 31, 32])), Some((31, 31)));
}

#[test]
fn luma_range_reads_the_corners_of_the_image() {
    let mut img = solid(8, 8, [100, 100, 100]);
    img.put_pixel(0, 0, image::Rgba([0, 0, 0, 255]));
    img.put_pixel(7, 7, image::Rgba([255, 255, 255, 255]));
    assert_eq!(luma_range(&img), Some((0, 255)));
}

#[test]
fn a_black_capture_is_bad_and_blank() {
    let img = solid(20, 20, [0, 0, 0]);
    assert!(capture_looks_bad(&img));
    assert!(capture_is_blank(&img));
}

#[test]
fn a_dark_but_not_black_capture_is_bad_only_when_uniform() {
    // Luma 11 everywhere: under the black threshold of 12 on both checks.
    let img = solid(20, 20, [11, 11, 11]);
    assert!(capture_is_blank(&img));
    assert!(capture_looks_bad(&img));
    // Luma 12 is the first value that is no longer blank, yet a perfectly uniform image
    // still looks bad because its range is under 4.
    let img = solid(20, 20, [12, 12, 12]);
    assert!(!capture_is_blank(&img));
    assert!(capture_looks_bad(&img));
}

#[test]
fn a_uniform_light_window_is_not_blank_but_looks_bad() {
    let img = solid(20, 20, [240, 240, 240]);
    assert!(!capture_is_blank(&img));
    assert!(capture_looks_bad(&img));
}

#[test]
fn a_capture_with_real_contrast_is_good() {
    let mut img = solid(20, 20, [200, 200, 200]);
    img.put_pixel(0, 0, image::Rgba([100, 100, 100, 255]));
    assert!(!capture_is_blank(&img));
    assert!(!capture_looks_bad(&img));
}

#[test]
fn the_contrast_threshold_is_a_range_of_four() {
    let mut img = solid(20, 20, [100, 100, 100]);
    img.put_pixel(0, 0, image::Rgba([103, 103, 103, 255]));
    assert_eq!(luma_range(&img), Some((100, 103)));
    assert!(capture_looks_bad(&img), "a range of 3 is still uniform");
    img.put_pixel(0, 0, image::Rgba([104, 104, 104, 255]));
    assert_eq!(luma_range(&img), Some((100, 104)));
    assert!(!capture_looks_bad(&img), "a range of 4 counts as real contrast");
}

#[test]
fn an_empty_capture_is_bad_and_blank() {
    let img = image::RgbaImage::new(0, 0);
    assert!(capture_looks_bad(&img));
    assert!(capture_is_blank(&img));
}

#[test]
fn clean_app_name_drops_the_path_and_the_exe_suffix() {
    assert_eq!(clean_app_name("C:\\Program Files\\Notepad\\notepad.exe"), "notepad");
    assert_eq!(clean_app_name("/usr/bin/tool"), "tool");
    assert_eq!(clean_app_name("CHROME.EXE"), "CHROME");
    assert_eq!(clean_app_name("plain"), "plain");
}

#[test]
fn clean_app_name_strips_characters_a_file_name_cannot_hold() {
    assert_eq!(clean_app_name("a<b>c:d\"e|f?g*h.exe"), "abcdefgh");
    assert_eq!(clean_app_name("  spaced name  .exe"), "spaced name");
}

#[test]
fn clean_app_name_is_empty_when_nothing_usable_is_left() {
    assert_eq!(clean_app_name(""), "");
    assert_eq!(clean_app_name("C:\\dir\\"), "");
    assert_eq!(clean_app_name("<>:|?*"), "");
    assert_eq!(clean_app_name(".exe"), "");
}

#[cfg(windows)]
#[test]
fn local_stamp_is_a_file_name_safe_date_and_time() {
    let s = local_stamp();
    assert_eq!(s.len(), 19, "{s}");
    let b = s.as_bytes();
    for (i, c) in b.iter().enumerate() {
        match i {
            4 | 7 | 13 | 16 => assert_eq!(*c, b'-', "{s}"),
            10 => assert_eq!(*c, b'_', "{s}"),
            _ => assert!(c.is_ascii_digit(), "{s}"),
        }
    }
    let month: u32 = s[5..7].parse().unwrap();
    let day: u32 = s[8..10].parse().unwrap();
    let hour: u32 = s[11..13].parse().unwrap();
    assert!((1..=12).contains(&month) && (1..=31).contains(&day) && hour < 24, "{s}");
}

// ---------- clipboard html ----------

#[test]
fn cf_html_header_pads_every_offset_to_ten_digits() {
    assert_eq!(
        cf_html_header(105, 183, 141, 147),
        "Version:0.9\r\nStartHTML:0000000105\r\nEndHTML:0000000183\r\nStartFragment:0000000141\r\nEndFragment:0000000147\r\n"
    );
}

#[test]
fn cf_html_header_has_the_same_length_for_any_offsets() {
    assert_eq!(cf_html_header(0, 0, 0, 0).len(), 105);
    assert_eq!(cf_html_header(1, 22, 333, 4444).len(), 105);
    assert_eq!(cf_html_header(9_999_999_999, 0, 0, 0).len(), 105);
}

fn cf_offset(doc: &str, label: &str) -> usize {
    let start = doc.find(label).unwrap() + label.len();
    doc[start..start + 10].parse().unwrap()
}

#[test]
fn build_cf_html_offsets_point_at_the_real_bytes() {
    // The fragment holds a two-byte character, so a character count would be off by one.
    let doc = build_cf_html("h\u{e9}llo");
    let start_html = cf_offset(&doc, "StartHTML:");
    let end_html = cf_offset(&doc, "EndHTML:");
    let start_fragment = cf_offset(&doc, "StartFragment:");
    let end_fragment = cf_offset(&doc, "EndFragment:");
    assert_eq!((start_html, start_fragment, end_fragment, end_html), (105, 141, 147, 183));
    assert_eq!(doc.len(), end_html);
    assert_eq!(&doc[start_html..start_html + 6], "<html>");
    assert_eq!(&doc[start_fragment..end_fragment], "h\u{e9}llo");
    assert!(doc[end_fragment..].starts_with("<!--EndFragment-->"));
    assert!(doc.ends_with("</html>"));
}

#[test]
fn build_cf_html_of_an_empty_fragment_has_adjacent_fragment_offsets() {
    let doc = build_cf_html("");
    assert_eq!(cf_offset(&doc, "StartFragment:"), cf_offset(&doc, "EndFragment:"));
    assert_eq!(cf_offset(&doc, "StartFragment:"), 141);
    assert_eq!(doc.len(), 177);
    assert_eq!(cf_offset(&doc, "EndHTML:"), 177);
}

// ---------- shuffle bag ----------

#[test]
fn a_shuffle_bag_must_be_non_empty_and_fit_the_entries() {
    assert!(!bag_is_valid(&[], 3));
    assert!(bag_is_valid(&[2, 0, 1], 3));
    assert!(bag_is_valid(&[1], 3));
}

#[test]
fn a_shuffle_bag_with_too_many_or_out_of_range_entries_is_invalid() {
    assert!(!bag_is_valid(&[0, 1, 2, 0], 3), "longer than the entry count");
    assert!(!bag_is_valid(&[0, 3], 3), "index equal to the count");
    assert!(!bag_is_valid(&[u32::MAX], 3));
    assert!(!bag_is_valid(&[0], 0), "no entries at all");
}

// ---------- variable window width ----------

#[test]
fn the_variable_window_width_stays_between_its_limits() {
    assert_eq!(var_fit_width(0.0), 220.0);
    assert_eq!(var_fit_width(219.9), 220.0);
    assert_eq!(var_fit_width(220.0), 220.0);
    assert_eq!(var_fit_width(300.0), 300.0);
    assert_eq!(var_fit_width(340.0), 340.0);
    assert_eq!(var_fit_width(1000.0), 340.0);
    assert_eq!(var_fit_width(-5.0), 220.0);
}

// ---------- recording folder files ----------

#[test]
fn a_configured_folder_matches_itself_in_any_spelling() {
    let d = util_dir("cfgdir");
    let sub = d.join("rec");
    fs::create_dir_all(&sub).unwrap();
    assert!(path_matches_configured_dir(&sub, &sub));
    // A trailing separator and a detour through the parent resolve to the same folder.
    let detour = d.join("rec").join("..").join("rec");
    assert!(path_matches_configured_dir(&detour, &sub));
    assert!(!path_matches_configured_dir(&d, &sub));
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_folder_that_does_not_exist_yet_matches_by_lowercase_text() {
    let a = Path::new("Z:\\no-such-root\\Videos\\");
    let b = Path::new("z:\\NO-SUCH-ROOT\\videos");
    assert!(path_matches_configured_dir(a, b));
    assert!(path_matches_configured_dir(Path::new("Z:/no-such-root/x/"), Path::new("Z:/no-such-root/x")));
    assert!(!path_matches_configured_dir(a, Path::new("Z:\\no-such-root\\Other")));
}

#[test]
fn an_existing_folder_never_matches_a_missing_one() {
    let d = util_dir("cfgdir2");
    assert!(!path_matches_configured_dir(&d, &d.join("missing")));
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn only_our_own_recording_names_and_formats_count_as_video_folder_files() {
    let d = util_dir("vidfolder");
    for (name, expect) in [
        ("rec-1.mp4", true),
        ("rec-2.WEBM", true),
        ("rec-3.Gif", true),
        ("rec-4.avi", true),
        ("rec-5.mov", true),
        ("rec-6.mkv", true),
        ("rec-7.webp", true),
        ("rec-8.apng", true),
        ("rec-9.txt", false),
        ("rec-10", false),
        ("holiday.mp4", false),
        ("Rec-11.mp4", false),
        ("rec-12.mp4.keys.json", false),
    ] {
        let p = d.join(name);
        fs::write(&p, b"x").unwrap();
        assert_eq!(is_video_folder_file(&p), expect, "{name}");
    }
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_folder_or_missing_path_is_not_a_video_folder_file() {
    let d = util_dir("vidfolder2");
    let dir = d.join("rec-1.mp4");
    fs::create_dir_all(&dir).unwrap();
    assert!(!is_video_folder_file(&dir));
    assert!(!is_video_folder_file(&d.join("rec-2.mp4")));
    let _ = fs::remove_dir_all(&d);
}

#[cfg(windows)]
#[test]
fn a_well_formed_key_sidecar_is_safe_to_copy() {
    let d = util_dir("sidecar-ok");
    let p = d.join("a.keys.json");
    fs::write(&p, r#"{"events":[{"vk":65,"down":true,"t_ms":10}],"gaps":[{"start_ms":1,"end_ms":2}]}"#).unwrap();
    assert!(key_sidecar_is_safe_to_copy(&p));
    fs::write(&p, r#"{"events":[],"gaps":[],"hold_ms":250}"#).unwrap();
    assert!(key_sidecar_is_safe_to_copy(&p));
    let _ = fs::remove_dir_all(&d);
}

#[cfg(windows)]
#[test]
fn a_malformed_missing_or_folder_key_sidecar_is_refused() {
    let d = util_dir("sidecar-bad");
    let p = d.join("a.keys.json");
    assert!(!key_sidecar_is_safe_to_copy(&p), "missing file");
    fs::write(&p, "not json at all").unwrap();
    assert!(!key_sidecar_is_safe_to_copy(&p));
    fs::write(&p, r#"{"events":"nope","gaps":[]}"#).unwrap();
    assert!(!key_sidecar_is_safe_to_copy(&p), "wrong field type");
    fs::write(&p, r#"{"events":[]}"#).unwrap();
    assert!(!key_sidecar_is_safe_to_copy(&p), "gaps is required");
    let dir = d.join("folder.keys.json");
    fs::create_dir_all(&dir).unwrap();
    assert!(!key_sidecar_is_safe_to_copy(&dir));
    let _ = fs::remove_dir_all(&d);
}

#[cfg(windows)]
#[test]
fn an_oversized_key_sidecar_is_refused_even_if_it_is_valid_json() {
    let d = util_dir("sidecar-big");
    let p = d.join("big.keys.json");
    // A valid document padded with whitespace past the 16 MiB ceiling.
    let mut body = String::from(r#"{"events":[],"gaps":[]}"#);
    body.push_str(&" ".repeat(16 * 1024 * 1024 + 1));
    fs::write(&p, body).unwrap();
    assert!(!key_sidecar_is_safe_to_copy(&p));
    // Exactly at the ceiling it is still accepted.
    let mut body = String::from(r#"{"events":[],"gaps":[]}"#);
    let pad = 16 * 1024 * 1024 - body.len();
    body.push_str(&" ".repeat(pad));
    assert_eq!(body.len(), 16 * 1024 * 1024);
    fs::write(&p, body).unwrap();
    assert!(key_sidecar_is_safe_to_copy(&p));
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn an_editor_work_copy_sits_directly_inside_a_prefixed_folder_of_the_temp_root() {
    let root = util_dir("editroot");
    let work = root.join("promptsaver-openedit-42");
    fs::create_dir_all(&work).unwrap();
    let file = work.join("clip.mp4");
    fs::write(&file, b"x").unwrap();
    assert!(is_editor_work_copy_in(&root, &file));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_file_outside_a_work_folder_is_not_an_editor_work_copy() {
    let root = util_dir("editroot2");
    // Wrong folder name.
    let other = root.join("something-else");
    fs::create_dir_all(&other).unwrap();
    let f1 = other.join("clip.mp4");
    fs::write(&f1, b"x").unwrap();
    assert!(!is_editor_work_copy_in(&root, &f1));
    // Directly in the root, with no work folder.
    let f2 = root.join("clip.mp4");
    fs::write(&f2, b"x").unwrap();
    assert!(!is_editor_work_copy_in(&root, &f2));
    // Nested one level too deep.
    let deep = root.join("promptsaver-openedit-1").join("inner");
    fs::create_dir_all(&deep).unwrap();
    let f3 = deep.join("clip.mp4");
    fs::write(&f3, b"x").unwrap();
    assert!(!is_editor_work_copy_in(&root, &f3));
    // A missing file and a folder are refused too.
    assert!(!is_editor_work_copy_in(&root, &root.join("promptsaver-openedit-1").join("gone.mp4")));
    assert!(!is_editor_work_copy_in(&root, &deep));
    // A temp root that does not exist.
    assert!(!is_editor_work_copy_in(&root.join("missing-root"), &f1));
    let _ = fs::remove_dir_all(&root);
}

// ---------- grid placement ----------

fn view_with(layout: &[(&str, [u32; 2])]) -> View {
    let mut v = test_view("v1");
    let grid = v.layouts.entry("3x2".to_string()).or_default();
    for (id, cell) in layout {
        grid.insert(id.to_string(), *cell);
    }
    v
}

#[test]
fn a_free_cell_inside_the_grid_is_used_as_asked() {
    let mut v = view_with(&[("a", [0, 0])]);
    place_on_cell_or_first_free(&mut v, "3x2", "b", [2, 1], &[]);
    assert_eq!(v.layouts["3x2"]["b"], [2, 1]);
}

#[test]
fn a_taken_cell_falls_back_to_the_first_free_one_in_reading_order() {
    let mut v = view_with(&[("a", [0, 0]), ("c", [1, 0])]);
    place_on_cell_or_first_free(&mut v, "3x2", "b", [0, 0], &[]);
    assert_eq!(v.layouts["3x2"]["b"], [2, 0]);
}

#[test]
fn a_cell_under_a_folder_counts_as_taken() {
    let mut v = view_with(&[]);
    place_on_cell_or_first_free(&mut v, "3x2", "b", [1, 0], &[[1, 0], [0, 0]]);
    assert_eq!(v.layouts["3x2"]["b"], [2, 0]);
}

#[test]
fn a_cell_outside_the_grid_falls_back_to_the_first_free_one() {
    let mut v = view_with(&[("a", [0, 0])]);
    place_on_cell_or_first_free(&mut v, "3x2", "b", [3, 0], &[]);
    assert_eq!(v.layouts["3x2"]["b"], [1, 0]);
    place_on_cell_or_first_free(&mut v, "3x2", "c", [0, 2], &[]);
    assert_eq!(v.layouts["3x2"]["c"], [2, 0]);
}

#[test]
fn a_full_grid_places_nothing() {
    // A grid whose every cell is covered by a tile and a folder.
    let mut v = test_view("v2");
    v.layouts.entry("2x1".to_string()).or_default().insert("a".to_string(), [0, 0]);
    place_on_cell_or_first_free(&mut v, "2x1", "z", [0, 0], &[[1, 0]]);
    assert!(!v.layouts["2x1"].contains_key("z"));
}

#[test]
fn an_unparsable_grid_key_leaves_the_view_untouched() {
    for key in ["", "3", "3x", "x2", "axb", "3x2x1", "3 x 2", "-1x2"] {
        let mut v = test_view("v1");
        place_on_cell_or_first_free(&mut v, key, "b", [0, 0], &[]);
        assert!(v.layouts.is_empty(), "key {key:?}");
    }
}

// ---------- grid folder normalising ----------

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_min_member_count_decides_which_folders_survive() {
    let prompts = ids(&["a", "b", "c"]);
    let views = ids(&["v1"]);
    let make = || vec![test_tile_folder("f1", "v1", [0, 0], &["a"]), test_tile_folder("f2", "v1", [1, 0], &["b", "c"])];

    let mut f = make();
    normalize_tile_folders_min(&mut f, &prompts, &views, 2);
    assert_eq!(f.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["f2"]);

    let mut f = make();
    normalize_tile_folders_min(&mut f, &prompts, &views, 1);
    assert_eq!(f.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["f1", "f2"]);

    // A minimum of zero still never keeps an empty folder.
    let mut f = vec![test_tile_folder("f0", "v1", [0, 0], &[]), test_tile_folder("f1", "v1", [1, 0], &["a"])];
    normalize_tile_folders_min(&mut f, &prompts, &views, 0);
    assert_eq!(f.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["f1"]);
}

#[test]
fn a_prompt_in_two_folders_stays_in_the_first_only() {
    let mut f = vec![test_tile_folder("f1", "v1", [0, 0], &["a", "b"]), test_tile_folder("f2", "v1", [1, 0], &["b", "c"])];
    normalize_tile_folders_min(&mut f, &ids(&["a", "b", "c"]), &ids(&["v1"]), 1);
    assert_eq!(f[0].members, ids(&["a", "b"]));
    assert_eq!(f[1].members, ids(&["c"]));
}

#[test]
fn unknown_prompts_and_unknown_views_are_dropped() {
    let mut f = vec![
        test_tile_folder("f1", "v1", [0, 0], &["a", "ghost"]),
        test_tile_folder("f2", "gone", [1, 0], &["b"]),
    ];
    normalize_tile_folders_min(&mut f, &ids(&["a", "b"]), &ids(&["v1"]), 1);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].id, "f1");
    assert_eq!(f[0].members, ids(&["a"]));
}

#[test]
fn a_member_of_a_dropped_folder_still_blocks_a_later_duplicate() {
    // The first folder lists "a" but sits on an unknown view and is removed afterwards; its
    // claim on "a" was already taken, so the second folder does not get "a" either.
    let mut f = vec![test_tile_folder("f1", "gone", [0, 0], &["a"]), test_tile_folder("f2", "v1", [1, 0], &["a", "b"])];
    normalize_tile_folders_min(&mut f, &ids(&["a", "b"]), &ids(&["v1"]), 1);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].members, ids(&["b"]));
}

#[test]
fn two_folders_on_one_cell_push_the_later_one_to_the_right() {
    let mut f = vec![
        test_tile_folder("f1", "v1", [2, 1], &["a"]),
        test_tile_folder("f2", "v1", [2, 1], &["b"]),
        test_tile_folder("f3", "v1", [2, 1], &["c"]),
        test_tile_folder("f4", "v2", [2, 1], &["d"]),
    ];
    normalize_tile_folders_min(&mut f, &ids(&["a", "b", "c", "d"]), &ids(&["v1", "v2"]), 1);
    assert_eq!(f[0].cell, [2, 1]);
    assert_eq!(f[1].cell, [3, 1]);
    assert_eq!(f[2].cell, [4, 1]);
    assert_eq!(f[3].cell, [2, 1], "another view has its own cells");
}

#[test]
fn a_huge_cell_is_clamped_to_the_grid_limit() {
    let mut f = vec![test_tile_folder("f1", "v1", [u32::MAX, u32::MAX], &["a"]), test_tile_folder("f2", "v1", [u32::MAX, 7], &["b"])];
    normalize_tile_folders_min(&mut f, &ids(&["a", "b"]), &ids(&["v1"]), 1);
    assert_eq!(f[0].cell, [200, 200]);
    assert_eq!(f[1].cell, [200, 7]);
}

#[test]
fn a_collision_at_the_clamp_edge_terminates() {
    let mut f = vec![test_tile_folder("f1", "v1", [u32::MAX, 0], &["a"]), test_tile_folder("f2", "v1", [u32::MAX, 0], &["b"])];
    normalize_tile_folders_min(&mut f, &ids(&["a", "b"]), &ids(&["v1"]), 1);
    assert_eq!(f[0].cell, [200, 0]);
    assert_eq!(f[1].cell, [201, 0]);
}

#[test]
fn the_folder_list_is_cut_to_its_maximum() {
    let prompts: Vec<String> = (0..205).map(|i| format!("p{i}")).collect();
    let mut f: Vec<TileFolder> = (0..205).map(|i| test_tile_folder(&format!("f{i}"), "v1", [i, 0], &[prompts[i as usize].as_str()])).collect();
    normalize_tile_folders_min(&mut f, &prompts, &ids(&["v1"]), 1);
    assert_eq!(f.len(), 200);
    assert_eq!(f[199].id, "f199");
}

#[test]
fn normalising_twice_changes_nothing_more() {
    let mut f = vec![
        test_tile_folder("f1", "v1", [0, 0], &["a", "b"]),
        test_tile_folder("f2", "v1", [0, 0], &["b", "c"]),
        test_tile_folder("f3", "v1", [0, 0], &["c", "d"]),
    ];
    let prompts = ids(&["a", "b", "c", "d"]);
    normalize_tile_folders_min(&mut f, &prompts, &ids(&["v1"]), 1);
    let once: Vec<(String, [u32; 2], Vec<String>)> = f.iter().map(|x| (x.id.clone(), x.cell, x.members.clone())).collect();
    normalize_tile_folders_min(&mut f, &prompts, &ids(&["v1"]), 1);
    let twice: Vec<(String, [u32; 2], Vec<String>)> = f.iter().map(|x| (x.id.clone(), x.cell, x.members.clone())).collect();
    assert_eq!(once, twice);
}

// ---------- store codecs ----------

#[test]
fn a_takeover_marker_is_recognised_by_its_lowercase_prefix() {
    assert!(is_takeover_marker_name("datadir.lock.json.claim-0123abcd"));
    assert!(is_takeover_marker_name("datadir.lock.json.claim-"));
    assert!(!is_takeover_marker_name("datadir.lock.json"));
    assert!(!is_takeover_marker_name("xdatadir.lock.json.claim-1"));
    assert!(!is_takeover_marker_name(""));
}

#[test]
fn the_marker_path_the_store_makes_is_recognised_as_a_marker() {
    let lock = Path::new("D:\\data\\datadir.lock.json");
    let marker = takeover_marker_path(lock, "dead-key");
    let name = marker.file_name().unwrap().to_string_lossy().to_lowercase();
    assert!(is_takeover_marker_name(&name), "{name}");
    assert_eq!(marker.parent(), lock.parent());
    assert_eq!(takeover_marker_path(lock, "dead-key"), marker, "the same dead lock gives the same marker");
    assert_ne!(takeover_marker_path(lock, "other-key"), marker);
}

#[test]
fn row_hash_is_stable_per_text_and_never_the_stale_value() {
    assert_eq!(row_hash("{\"a\":1}"), row_hash("{\"a\":1}"));
    assert_ne!(row_hash("{\"a\":1}"), row_hash("{\"a\":2}"));
    assert_ne!(row_hash(""), STALE_ROW_HASH);
    assert_ne!(row_hash("x"), 0);
}

#[test]
fn variants_survive_an_encode_decode_round_trip() {
    let list = vec!["one".to_string(), "two \"quoted\"".to_string(), "line\nbreak".to_string(), "caf\u{e9} \u{1f600}".to_string(), String::new()];
    let raw = encode_variants(&list);
    assert_eq!(decode_variants(&raw), list);
    assert_eq!(encode_variants(&[]), "[]");
    assert_eq!(decode_variants("[]"), Vec::<String>::new());
    assert_eq!(encode_variants(&["a".to_string(), "b".to_string()]), "[\"a\",\"b\"]");
}

#[test]
fn decoding_damaged_variants_gives_an_empty_list() {
    for raw in ["", "not json", "{\"a\":1}", "[1,2]", "[\"a\",", "null"] {
        assert_eq!(decode_variants(raw), Vec::<String>::new(), "{raw:?}");
    }
}

#[test]
fn prompt_history_is_on_unless_switched_off() {
    let mut s = Settings::default();
    assert!(history_enabled(&s));
    s.ui_flags.insert("promptHistory".to_string(), false);
    assert!(!history_enabled(&s));
    s.ui_flags.insert("promptHistory".to_string(), true);
    assert!(history_enabled(&s));
}

#[test]
fn the_history_cap_defaults_to_twenty_and_is_clamped() {
    let mut s = Settings::default();
    assert_eq!(history_cap(&s), 20);
    for (v, want) in [(1.0, 5), (5.0, 5), (7.0, 7), (50.0, 50), (100.0, 100), (5000.0, 100), (-3.0, 5), (12.9, 12)] {
        s.ui_values.insert("historyPerPrompt".to_string(), v);
        assert_eq!(history_cap(&s), want, "value {v}");
    }
}
