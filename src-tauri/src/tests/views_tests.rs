use super::*;

#[test]
fn shrinking_a_grid_never_moves_a_folder_onto_one_that_still_fits() {
    // The folder that no longer fits comes FIRST in the list: it must not take the
    // cell the in-bounds one holds.
    let mut folders = vec![
        test_tile_folder("far", "v1", [5, 5], &["a", "b"]),
        test_tile_folder("near", "v1", [0, 0], &["c", "d"]),
        test_tile_folder("other-view", "v2", [9, 9], &["e", "f"]),
    ];
    fit_folders_to_grid(&mut folders, "v1", 3, 3);
    assert_eq!(folders[1].cell, [0, 0], "the folder that fits stays where it was");
    assert_eq!(folders[0].cell, [1, 0], "the moved one takes the first free cell");
    assert_eq!(folders[2].cell, [9, 9], "another view's folder is not touched");
}

#[test]
fn taking_a_prompt_out_never_drops_it_on_a_cell_a_folder_keeps() {
    let mut view = test_view("v");
    view.cols = 3;
    view.rows = 1;
    let folders = vec![
        test_tile_folder("origin", "v", [0, 0], &["x", "a", "b"]),
        test_tile_folder("other", "v", [1, 0], &["c", "d"]),
    ];
    let cells: Vec<[u32; 2]> = folders.iter().map(|f| f.cell).collect();
    // Onto another folder's cell, and onto the origin folder that keeps two members.
    assert_eq!(take_out_target_cell(&view, &folders, &cells, "x", [1, 0]), Ok([2, 0]));
    assert_eq!(take_out_target_cell(&view, &folders, &cells, "x", [0, 0]), Ok([2, 0]));
    // A free cell is used as asked.
    assert_eq!(take_out_target_cell(&view, &folders, &cells, "x", [2, 0]), Ok([2, 0]));
    // The origin folder that would dissolve may be dropped on (its last member is settled aside).
    let small = vec![test_tile_folder("origin", "v", [0, 0], &["x", "a"])];
    assert_eq!(take_out_target_cell(&view, &small, &[[0, 0]], "x", [0, 0]), Ok([0, 0]));
    // No free cell at all: refused.
    let mut full = test_view("v");
    full.cols = 2;
    full.rows = 1;
    let two = vec![
        test_tile_folder("origin", "v", [0, 0], &["x", "a", "b"]),
        test_tile_folder("other", "v", [1, 0], &["c", "d"]),
    ];
    let cells: Vec<[u32; 2]> = two.iter().map(|f| f.cell).collect();
    assert_eq!(take_out_target_cell(&full, &two, &cells, "x", [1, 0]), Err(()));
}

#[test]
fn a_prompt_placed_on_the_grid_becomes_a_view_member() {
    let mut view: View = serde_json::from_str(r#"{"id":"v","name":"V"}"#).expect("a minimal view");
    view.members = vec!["a".into()];
    let layout: HashMap<String, [u32; 2]> =
        [("a".to_string(), [0, 0]), ("b".to_string(), [1, 0]), ("gone".to_string(), [2, 0])].into_iter().collect();
    let known: std::collections::HashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
    seat_layout_ids_as_members(&mut view, &layout, &known);
    assert_eq!(view.members, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn a_stale_layout_drops_the_ids_of_deleted_prompts() {
    let layout: HashMap<String, [u32; 2]> =
        [("a".to_string(), [0, 0]), ("gone".to_string(), [1, 0])].into_iter().collect();
    let known: std::collections::HashSet<String> = ["a".to_string()].into_iter().collect();
    let kept = layout_of_known(layout, &known);
    assert_eq!(kept.len(), 1);
    assert!(kept.contains_key("a"));
}

#[test]
fn renaming_a_tag_to_another_casing_keeps_its_colour() {
    let mut prompts = vec![test_prompt("a")];
    prompts[0].tags = vec!["css".to_string()];
    assert!(rename_tag_in(&mut prompts, "css", "CSS"));
    assert_eq!(prompts[0].tags, vec!["CSS".to_string()]);
    let mut colors = HashMap::from([("css".to_string(), "#ff0000".to_string())]);
    transfer_tag_color(&mut colors, "css", "css");
    assert_eq!(colors.get("css").map(String::as_str), Some("#ff0000"));
}

#[test]
fn hot_path_commands_run_off_the_main_thread() {
    // Contrast: a sync command runs on the event-loop thread and freezes the
    // window while a backup holds the store. No red test exists for this (it
    // needs a live event loop), so the attribute itself is asserted.
    for (src, name) in [
        (include_str!("../logging.rs"), "log_error"),
        (include_str!("../settings_cmds.rs"), "get_settings"),
        (include_str!("../autostart.rs"), "set_always_on_top"),
        (include_str!("../theme.rs"), "current_theme"),
        (include_str!("../update.rs"), "set_ui_value"),
        (include_str!("../views.rs"), "add_view"),
        (include_str!("../views.rs"), "remap_colors"),
        (include_str!("../views.rs"), "delete_view"),
        (include_str!("../views.rs"), "save_tile_folder"),
        (include_str!("../views.rs"), "take_out_of_tile_folder"),
        (include_str!("../views.rs"), "delete_tile_folder"),
        (include_str!("../theme.rs"), "save_custom_theme"),
        (include_str!("../theme.rs"), "delete_custom_theme"),
    ] {
        let at = src.find(&format!("fn {name}(")).expect(name);
        let head = &src[..at];
        let attr = head.rfind("#[tauri::command").expect(name);
        assert!(
            head[attr..].starts_with("#[tauri::command(async)]"),
            "{name} must be an async command"
        );
    }
}
