use super::*;

// ---- Prompt folders as Markdown ----

pub(super) fn sync_test_prompt() -> Prompt {
    let mut p = sync_blank_prompt("p1".into());
    p.name = "Test prompt".into();
    p.color = "#7c5cff".into();
    p.tags = vec!["Work".into(), "draft".into()];
    p.notes = "a private note".into();
    p.text = "Hello\nWorld".into();
    p
}

#[test]
fn md_roundtrip_preserves_editable_fields() {
    let p = sync_test_prompt();
    let content = prompt_to_md(&p, &SyncPlacement::default());
    let updated = apply_md_to_prompt(&p, &content).expect("valid frontmatter");
    assert_eq!(updated.name, p.name);
    assert_eq!(updated.color, p.color);
    assert_eq!(updated.tags, p.tags);
    assert_eq!(updated.notes, p.notes);
    // The closing newline the writer adds is the file's, not the prompt's: a
    // round trip must hand back exactly the text that went in.
    assert_eq!(updated.text, "Hello\nWorld");
    // Re-serializing the round-tripped prompt must be byte-identical: otherwise
    // sync_write_all()'s "only write if content differs" guard would loop forever.
    assert_eq!(prompt_to_md(&updated, &SyncPlacement::default()), content);
}

// image_text is a search index, never synchronized: the
// Markdown mirror writes a fixed set of named fields, so a value that is
// simply never mentioned there cannot leak into the file on disk.
#[test]
fn md_sync_never_carries_image_text() {
    let mut p = sync_test_prompt();
    p.image_text = "invoice total due friday".into();
    let content = prompt_to_md(&p, &SyncPlacement::default());
    assert!(!content.contains("invoice"), "OCR-recognized text leaked into the synced file: {content}");
}

#[test]
fn sync_placement_for_is_empty_when_the_prompt_has_no_placement() {
    let settings = Settings::default();
    let placement = sync_placement_for(&sync_test_prompt(), &settings);
    assert_eq!(placement.view, "");
    assert_eq!(placement.grid, "");
    assert_eq!(placement.cell, "");
    assert_eq!(placement.tile_folder, "");
    assert_eq!(placement.float_folder, "");
}

#[test]
fn sync_placement_for_reports_a_plain_grid_cell() {
    let mut settings = Settings::default();
    let mut view = test_view("v1");
    view.layouts.insert(grid_key(view.cols, view.rows), HashMap::from([("p1".to_string(), [2, 3])]));
    settings.views.push(view);
    let placement = sync_placement_for(&sync_test_prompt(), &settings);
    assert_eq!(placement.view, "V1");
    assert_eq!(placement.grid, grid_key(default_cols(), default_rows()));
    assert_eq!(placement.cell, "2,3");
    assert_eq!(placement.tile_folder, "");
}

#[test]
fn sync_placement_for_reports_tile_folder_membership_over_a_plain_cell() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    let mut folder = test_tile_folder("f1", "v1", [4, 5], &["p1"]);
    folder.name = "Team".to_string();
    folder.color = "#ff0000".to_string();
    settings.tile_folders.push(folder);
    let placement = sync_placement_for(&sync_test_prompt(), &settings);
    assert_eq!(placement.view, "V1");
    assert_eq!(placement.cell, "4,5");
    assert_eq!(placement.tile_folder, "Team");
    assert_eq!(placement.tile_folder_color, "#ff0000");
}

#[test]
fn sync_placement_for_reports_float_folder_membership_independently() {
    let mut settings = Settings::default();
    settings.float_folders.push(FloatFolder {
        id: "ff1".into(),
        name: "Pills".into(),
        pos: Pos { x: 0, y: 0, ..Default::default() },
        scale: 1.0,
        members: vec!["p1".to_string(), "p2".to_string()],
    });
    let placement = sync_placement_for(&sync_test_prompt(), &settings);
    assert_eq!(placement.float_folder, "Pills");
    assert_eq!(placement.view, "", "a floating pill with no grid tile has no grid placement");
}

#[test]
fn apply_md_placement_creates_a_view_by_name_and_places_the_cell() {
    let mut settings = Settings::default();
    let fields = HashMap::from([
        ("view".to_string(), "Work".to_string()),
        ("grid".to_string(), "5x4".to_string()),
        ("cell".to_string(), "1,2".to_string()),
    ]);
    assert_eq!(apply_md_placement(&mut settings, "p1", &fields), (true, false));
    assert_eq!(settings.views.len(), 1);
    let view = &settings.views[0];
    assert_eq!(view.name, "Work");
    assert_eq!(view.cols, 5);
    assert_eq!(view.rows, 4);
    assert_eq!(view.layouts.get(&grid_key(5, 4)).and_then(|m| m.get("p1")), Some(&[1, 2]));
}

#[test]
fn apply_md_placement_reuses_an_existing_view_with_the_same_name() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1")); // name "V1"
    let existing_count = settings.views.len();
    let fields = HashMap::from([
        ("view".to_string(), "V1".to_string()),
        ("grid".to_string(), grid_key(default_cols(), default_rows())),
        ("cell".to_string(), "0,0".to_string()),
    ]);
    assert_eq!(apply_md_placement(&mut settings, "p1", &fields), (true, false));
    assert_eq!(settings.views.len(), existing_count, "must not create a second view for a name that already exists");
}

#[test]
fn apply_md_placement_falls_back_to_a_free_cell_on_collision() {
    let mut settings = Settings::default();
    let mut view = test_view("v1");
    view.cols = 2;
    view.rows = 1;
    view.layouts.insert(grid_key(2, 1), HashMap::from([("other".to_string(), [0, 0])]));
    settings.views.push(view);
    let fields = HashMap::from([
        ("view".to_string(), "V1".to_string()),
        ("grid".to_string(), "2x1".to_string()),
        ("cell".to_string(), "0,0".to_string()),
    ]);
    assert_eq!(apply_md_placement(&mut settings, "p1", &fields), (true, false));
    let key = grid_key(2, 1);
    assert_eq!(settings.views[0].layouts.get(&key).and_then(|m| m.get("p1")), Some(&[1, 0]));
}

#[test]
fn apply_md_placement_adopts_a_cleared_folder_colour() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    let mut fields = HashMap::from([
        ("view".to_string(), "V1".to_string()),
        ("grid".to_string(), grid_key(default_cols(), default_rows())),
        ("cell".to_string(), "0,0".to_string()),
        ("tile_folder".to_string(), "Team".to_string()),
        ("tile_folder_color".to_string(), "#00ff00".to_string()),
    ]);
    apply_md_placement(&mut settings, "p1", &fields);
    assert_eq!(settings.tile_folders[0].color, "#00ff00");
    fields.insert("tile_folder_color".to_string(), String::new());
    apply_md_placement(&mut settings, "p2", &fields);
    assert_eq!(settings.tile_folders.len(), 1);
    assert_eq!(settings.tile_folders[0].color, "", "the other machine cleared the colour");
}

#[test]
fn apply_md_placement_creates_a_tile_folder_and_drops_the_members_own_layout_entry() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    let fields = HashMap::from([
        ("view".to_string(), "V1".to_string()),
        ("grid".to_string(), grid_key(default_cols(), default_rows())),
        ("cell".to_string(), "0,0".to_string()),
        ("tile_folder".to_string(), "Team".to_string()),
        ("tile_folder_color".to_string(), "#00ff00".to_string()),
    ]);
    assert_eq!(apply_md_placement(&mut settings, "p1", &fields), (true, true));
    assert_eq!(settings.tile_folders.len(), 1);
    assert_eq!(settings.tile_folders[0].name, "Team");
    assert!(settings.tile_folders[0].members.iter().any(|m| m == "p1"));
    let key = grid_key(default_cols(), default_rows());
    assert!(
        settings.views[0].layouts.get(&key).map(|m| !m.contains_key("p1")).unwrap_or(true),
        "a folder member is not a tile of its own"
    );
}

#[test]
fn apply_md_placement_never_turns_a_grid_only_prompt_into_a_floating_pill() {
    let mut settings = Settings::default();
    let fields = HashMap::from([("float_folder".to_string(), "Pills".to_string())]);
    assert_eq!(apply_md_placement(&mut settings, "p1", &fields), (false, false));
    assert!(settings.float_folders.is_empty(), "joining a pill folder from sync must never conjure a new pill window");
}

#[test]
fn apply_md_placement_joins_an_existing_floating_pill_to_a_pill_folder() {
    let mut settings = Settings::default();
    settings.floating.insert("p1".to_string(), Pos { x: 10, y: 10, ..Default::default() });
    let fields = HashMap::from([("float_folder".to_string(), "Pills".to_string())]);
    let (grid_placed, folders_touched) = apply_md_placement(&mut settings, "p1", &fields);
    assert!(!grid_placed, "no grid fields were supplied, so the caller should still fall back for the grid");
    assert!(folders_touched);
    assert_eq!(settings.float_folders.len(), 1);
    assert!(settings.float_folders[0].members.iter().any(|m| m == "p1"));
}

#[test]
fn apply_md_placement_returns_nothing_placed_when_every_field_is_absent() {
    let mut settings = Settings::default();
    let fields = HashMap::new();
    assert_eq!(apply_md_placement(&mut settings, "p1", &fields), (false, false));
    assert!(settings.views.is_empty());
}

#[test]
fn prompt_to_md_and_apply_md_placement_round_trip_a_tile_folder() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    let mut folder = test_tile_folder("f1", "v1", [2, 2], &["p1"]);
    folder.name = "Team".to_string();
    folder.color = "#7c5cff".to_string();
    settings.tile_folders.push(folder);
    let p = sync_test_prompt();
    let placement = sync_placement_for(&p, &settings);
    let md = prompt_to_md(&p, &placement);
    let (fields, _) = parse_frontmatter(&md).unwrap();
    let mut target = Settings::default();
    assert_eq!(apply_md_placement(&mut target, "p1", &fields), (true, true));
    assert_eq!(target.tile_folders[0].name, "Team");
    assert_eq!(target.tile_folders[0].color, "#7c5cff");
    assert_eq!(target.views[0].name, "V1");
}

// Re-importing a placement that already matches (only the prompt's TEXT
// changed on the other machine, not its position) must leave the cell alone:
// sync_resolve_cell must not see the prompt's own existing entry as a
// collision with itself.
#[test]
fn apply_md_placement_is_idempotent_for_the_prompts_own_current_cell() {
    let mut settings = Settings::default();
    let mut view = test_view("v1");
    view.layouts.insert(grid_key(view.cols, view.rows), HashMap::from([("p1".to_string(), [1, 1])]));
    settings.views.push(view);
    let fields = HashMap::from([
        ("view".to_string(), "V1".to_string()),
        ("grid".to_string(), grid_key(default_cols(), default_rows())),
        ("cell".to_string(), "1,1".to_string()),
    ]);
    apply_md_placement(&mut settings, "p1", &fields);
    let key = grid_key(default_cols(), default_rows());
    assert_eq!(
        settings.views[0].layouts.get(&key).and_then(|m| m.get("p1")),
        Some(&[1, 1]),
        "the prompt's own current cell must never read as occupied by itself"
    );
}

// A field that is PRESENT but empty ("tile_folder: ") is a deliberate "left
// this folder" from the writer, distinct from the key being absent entirely
// (an older file, untouched). It must actually remove existing local
// membership, or a folder left on one machine is silently restored by the
// other machine's own echo on the next poll.
#[test]
fn apply_md_placement_leaves_a_tile_folder_on_an_explicit_empty_field() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    settings.tile_folders.push(test_tile_folder("f1", "v1", [0, 0], &["p1", "p2"]));
    let fields = HashMap::from([
        ("view".to_string(), "V1".to_string()),
        ("grid".to_string(), grid_key(default_cols(), default_rows())),
        ("cell".to_string(), "3,3".to_string()),
        ("tile_folder".to_string(), String::new()),
    ]);
    let (placed, folders_touched) = apply_md_placement(&mut settings, "p1", &fields);
    assert!(placed);
    assert!(folders_touched);
    assert!(!settings.tile_folders[0].members.iter().any(|m| m == "p1"));
    let key = grid_key(default_cols(), default_rows());
    assert_eq!(settings.views[0].layouts.get(&key).and_then(|m| m.get("p1")), Some(&[3, 3]));
}

#[test]
fn apply_md_placement_leaves_a_float_folder_on_an_explicit_empty_field() {
    let mut settings = Settings::default();
    settings.floating.insert("p1".to_string(), Pos { x: 0, y: 0, ..Default::default() });
    settings.float_folders.push(FloatFolder {
        id: "ff1".into(),
        name: "Pills".into(),
        pos: Pos { x: 0, y: 0, ..Default::default() },
        scale: 1.0,
        members: vec!["p1".to_string(), "p2".to_string()],
    });
    let fields = HashMap::from([("float_folder".to_string(), String::new())]);
    let (_, folders_touched) = apply_md_placement(&mut settings, "p1", &fields);
    assert!(folders_touched);
    assert!(!settings.float_folders[0].members.iter().any(|m| m == "p1"));
}

// A field the key is entirely ABSENT for (as opposed to present-but-empty)
// means "this file predates the six placement fields": must NOT be read as a
// leave instruction, or every legacy sync file would strip every prompt out
// of every folder the moment this version first polls it.
#[test]
fn apply_md_placement_never_treats_a_missing_field_as_a_leave() {
    let mut settings = Settings::default();
    settings.tile_folders.push(test_tile_folder("f1", "v1", [0, 0], &["p1", "p2"]));
    let fields = HashMap::new(); // no tile_folder / float_folder key at all
    let (_, folders_touched) = apply_md_placement(&mut settings, "p1", &fields);
    assert!(!folders_touched);
    assert!(settings.tile_folders[0].members.iter().any(|m| m == "p1"));
}

// An unusable folder name (rejected by folder_name_of) must fall through to
// plain grid placement instead of claiming grid_placed=true for nothing: a
// brand-new prompt imported this way would otherwise land nowhere at all,
// invisible in every view.
#[test]
fn apply_md_placement_falls_back_to_the_grid_when_the_folder_name_is_rejected() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    let too_long = "x".repeat(MAX_FOLDER_NAME + 1);
    let fields = HashMap::from([
        ("view".to_string(), "V1".to_string()),
        ("grid".to_string(), grid_key(default_cols(), default_rows())),
        ("cell".to_string(), "0,0".to_string()),
        ("tile_folder".to_string(), too_long),
    ]);
    let (placed, folders_touched) = apply_md_placement(&mut settings, "p1", &fields);
    assert!(placed, "grid fields were valid - must not report nothing placed");
    assert!(!folders_touched);
    assert!(settings.tile_folders.is_empty());
    let key = grid_key(default_cols(), default_rows());
    assert_eq!(settings.views[0].layouts.get(&key).and_then(|m| m.get("p1")), Some(&[0, 0]));
}

// A folder name is only unique WITHIN its own view: matching by name alone
// merged two same-named folders from different views into whichever the scan
// found first.
#[test]
fn sync_join_tile_folder_scopes_by_view_not_just_name() {
    let mut settings = Settings {
        views: vec![test_view("v1"), test_view("v2")],
        ..Default::default()
    };
    settings.tile_folders = vec![test_tile_folder("f1", "v1", [0, 0], &["a"])];
    settings.tile_folders[0].name = "Work".to_string();

    // Same name, DIFFERENT view -> must NOT merge into v1's folder.
    let ok = sync_join_tile_folder(&mut settings, "b", "Work", "", "v2", Some([0, 0]));
    assert!(ok);
    assert_eq!(settings.tile_folders.len(), 2, "same name in a different view must not merge");
    let v1_folder = settings.tile_folders.iter().find(|f| f.view_id == "v1").unwrap();
    let v2_folder = settings.tile_folders.iter().find(|f| f.view_id == "v2").unwrap();
    assert_eq!(v1_folder.members, vec!["a".to_string()]);
    assert_eq!(v2_folder.members, vec!["b".to_string()]);

    // Same name, SAME view -> must merge.
    let ok2 = sync_join_tile_folder(&mut settings, "c", "Work", "", "v1", Some([0, 0]));
    assert!(ok2);
    let v1_folder2 = settings.tile_folders.iter().find(|f| f.view_id == "v1").unwrap();
    assert!(v1_folder2.members.contains(&"c".to_string()));
    assert_eq!(settings.tile_folders.len(), 2, "the same-view join must still merge, not add a third folder");
}

// A folder whose members arrive in separate poll cycles (a cloud provider has
// no ordering/batching guarantee between two machines) must survive at 1
// member instead of being dissolved before the second file ever lands.
#[test]
fn sync_normalize_tile_folders_tolerates_a_still_forming_one_member_folder() {
    let ids = vec!["p1".to_string()];
    let view_ids = vec!["v1".to_string()];
    let mut folders = vec![test_tile_folder("f1", "v1", [0, 0], &["p1"])];
    sync_normalize_tile_folders(&mut folders, &ids, &view_ids);
    assert_eq!(folders.len(), 1, "a still-forming folder must survive a sync-triggered normalize");
    // But a genuinely empty folder (its only member vanished) still goes:
    // and the general, UI-facing normalize still dissolves anything under 2.
    let mut empty = vec![test_tile_folder("f2", "v1", [1, 0], &["gone"])];
    sync_normalize_tile_folders(&mut empty, &ids, &view_ids);
    assert!(empty.is_empty());
    let mut general = vec![test_tile_folder("f3", "v1", [0, 0], &["p1"])];
    normalize_tile_folders(&mut general, &ids, &view_ids);
    assert!(general.is_empty(), "the UI-facing normalize must keep dissolving a real 1-member folder");
}

#[test]
fn sync_normalize_float_folders_tolerates_a_still_forming_one_member_folder() {
    let ids = vec!["p1".to_string()];
    let mut folders = vec![FloatFolder {
        id: "f1".into(),
        name: "Team".into(),
        pos: Pos { x: 0, y: 0, ..Default::default() },
        scale: 1.0,
        members: vec!["p1".to_string()],
    }];
    sync_normalize_float_folders(&mut folders, &ids);
    assert_eq!(folders.len(), 1);
    let mut general = vec![FloatFolder {
        id: "f2".into(),
        name: "Team".into(),
        pos: Pos { x: 0, y: 0, ..Default::default() },
        scale: 1.0,
        members: vec!["p1".to_string()],
    }];
    normalize_float_folders(&mut general, &ids);
    assert!(general.is_empty());
}

// ---- Multi-instance crash simulation ----
//
// The whole round, driven against two instances sharing a folder, lives in
// md_sync_multi.rs. This helper is the cheaper per-file variant: it replays the poll's own
// per-file sequence (parse -> apply content -> apply placement -> normalize) against a real
// temp directory, so interleaving it under two independent (Vec<Prompt>, Settings) pairs
// stands in for two instances syncing through the same cloud folder. Normalizing after
// every single file (finer grain than the real once-per-batch) makes this the STRICTER
// case.
pub(super) fn poll_apply(prompts: &mut Vec<Prompt>, settings: &mut Settings, content: &str, stem: &str) {
    let fields = parse_frontmatter(content).filter(|(f, _)| md_is_ours(f)).map(|(f, _)| f);
    match prompts.iter().position(|p| p.id == stem) {
        Some(idx) => {
            if let Some(updated) = apply_md_to_prompt(&prompts[idx], content) {
                prompts[idx] = updated;
            }
            if let Some(f) = fields.as_ref() {
                apply_md_placement(settings, stem, f);
            }
        }
        None => {
            let Some(np) = apply_md_to_prompt(&sync_blank_prompt(stem.to_string()), content) else { return };
            prompts.push(np);
            if let Some(f) = fields.as_ref() {
                apply_md_placement(settings, stem, f);
            }
        }
    }
    let ids: Vec<String> = prompts.iter().map(|p| p.id.clone()).collect();
    let view_ids: Vec<String> = settings.views.iter().map(|v| v.id.clone()).collect();
    sync_normalize_tile_folders(&mut settings.tile_folders, &ids, &view_ids);
    sync_normalize_float_folders(&mut settings.float_folders, &ids);
}

// A scenario worth calling out by name: a cloud provider gives no
// ordering/batching guarantee between two files it propagates to a second
// machine. Instance A groups two prompts into a folder and writes both files;
// instance B's poll only sees the first one, then the second "arrives" a cycle
// later. Must end up with ONE folder holding both members, on a view B never
// had until this ran - never two folders, and never a folder that got
// dissolved before the second member could join.
#[test]
fn multi_instance_staggered_folder_arrival_never_loses_or_duplicates_the_folder() {
    let dir = std::env::temp_dir().join(format!("ps-multi-inst-{}", gen_id()));
    fs::create_dir_all(&dir).unwrap();

    let mut a_prompts = vec![sync_test_prompt(), {
        let mut p = sync_test_prompt();
        p.id = "p2".into();
        p
    }];
    for p in a_prompts.iter_mut() {
        p.name = format!("Prompt {}", p.id);
    }
    let mut a_settings = Settings::default();
    a_settings.views.push(test_view("v1"));
    a_settings.tile_folders.push({
        let mut f = test_tile_folder("f1", "v1", [0, 0], &["p1", "p2"]);
        f.name = "Team".to_string();
        f
    });
    for p in &a_prompts {
        let content = prompt_to_md(p, &sync_placement_for(p, &a_settings));
        fs::write(dir.join(format!("{}.md", p.id)), content).unwrap();
    }

    // Instance B starts from nothing - no matching view either, so this also
    // exercises auto-creating the view by name.
    let mut b_prompts: Vec<Prompt> = Vec::new();
    let mut b_settings = Settings::default();

    let p1_content = fs::read_to_string(dir.join("p1.md")).unwrap();
    poll_apply(&mut b_prompts, &mut b_settings, &p1_content, "p1");
    assert_eq!(b_settings.views.len(), 1, "the missing view must be auto-created by name");
    assert_eq!(b_settings.views[0].name, "V1");
    assert_eq!(b_settings.tile_folders.len(), 1, "a still-forming folder must survive its first poll");
    assert_eq!(b_settings.tile_folders[0].members, vec!["p1".to_string()]);

    // p2.md "arrives" one poll cycle later.
    let p2_content = fs::read_to_string(dir.join("p2.md")).unwrap();
    poll_apply(&mut b_prompts, &mut b_settings, &p2_content, "p2");
    assert_eq!(b_settings.views.len(), 1, "must reuse the view it already created, not duplicate it");
    assert_eq!(b_settings.tile_folders.len(), 1, "the second member must join the SAME folder, never create a second one");
    assert_eq!(b_settings.tile_folders[0].name, "Team");
    let mut members = b_settings.tile_folders[0].members.clone();
    members.sort();
    assert_eq!(members, vec!["p1".to_string(), "p2".to_string()]);

    let _ = fs::remove_dir_all(&dir);
}

// The other half of the crash-simulation requirement: a file
// cut off mid-write (crash between lock acquire and the write completing)
// never has a closing "---" at all. Must be skipped outright, never applied
// partially - no half-updated name, no fabricated placement from whatever
// fragment of the header made it to disk.
#[test]
fn a_file_truncated_by_a_simulated_crash_mid_write_is_skipped_not_misapplied() {
    let p = sync_test_prompt();
    let settings = Settings::default();
    let full = prompt_to_md(&p, &sync_placement_for(&p, &settings));
    let truncated = &full[..full.len() / 3];
    assert!(parse_frontmatter(truncated).is_none(), "an unclosed header must never parse as valid frontmatter");

    let mut prompts = vec![p.clone()];
    let mut target_settings = Settings::default();
    poll_apply(&mut prompts, &mut target_settings, truncated, "p1");
    assert_eq!(prompts[0].name, p.name, "a truncated file must never overwrite the prompt with garbage");
    assert!(target_settings.views.is_empty(), "a truncated file must never fabricate a placement either");
}

#[test]
fn md_escape_line_strips_embedded_newlines() {
    assert_eq!(md_escape_line("a\nb\r\nc"), "a b  c");
}

#[test]
fn parse_frontmatter_rejects_a_file_with_no_header() {
    assert!(parse_frontmatter("just some text\nwith no header").is_none());
    assert!(apply_md_to_prompt(&sync_blank_prompt("x".into()), "no header here").is_none());
}

#[test]
fn parse_frontmatter_rejects_an_unclosed_header() {
    // Opens with --- but never closes it: must not silently swallow the rest
    // of the file as a giant, wrong "field".
    assert!(parse_frontmatter("---\nname: x\nstill going...").is_none());
}

#[test]
fn frontmatter_tags_dedupe_case_insensitively_and_drop_blanks() {
    let base = sync_blank_prompt("p2".into());
    let content = "---\nname: x\ncolor: \ntags: Work, work, , Draft\nnotes: \n---\nbody\n";
    let updated = apply_md_to_prompt(&base, content).unwrap();
    assert_eq!(updated.tags, vec!["Work".to_string(), "Draft".to_string()]);
}

#[test]
fn sync_eligible_takes_media_prompts_but_still_refuses_an_unsafe_id() {
    let mut p = sync_test_prompt();
    assert!(sync_eligible(&p));
    // Attachments no longer disqualify a prompt: they sync as path references.
    p.image = "data:image/png;base64,x".into();
    p.file_path = "C:\\x.pdf".into();
    p.icon_path = "C:\\x.gif".into();
    assert!(sync_eligible(&p));
    // The id is still the file name, so it still has to be a safe token.
    let mut bad = sync_test_prompt();
    bad.id = "../evil".into();
    assert!(!sync_eligible(&bad));
}

#[test]
fn media_paths_round_trip_through_the_markdown_file() {
    let mut p = sync_test_prompt();
    p.file_path = "C:\\docs\\manual.pdf".into();
    p.icon_path = "C:\\icons\\book.png".into();
    p.pdf_page = 7;
    // The embedded copy has no path to write, so it must not appear in the file
    // at all: and must survive the round trip untouched.
    p.image = "data:image/png;base64,AAA".into();
    let md = prompt_to_md(&p, &SyncPlacement::default());
    assert!(md.contains("\nfile: C:\\docs\\manual.pdf\n"));
    assert!(md.contains("\nicon: C:\\icons\\book.png\n"));
    assert!(md.contains("\npdf_page: 7\n"));
    assert!(!md.contains("base64"));
    let back = apply_md_to_prompt(&p, &md).unwrap();
    assert_eq!(back.file_path, p.file_path);
    assert_eq!(back.icon_path, p.icon_path);
    assert_eq!(back.pdf_page, 7);
    assert_eq!(back.image, p.image);
}

#[test]
fn a_media_field_is_only_cleared_when_the_file_actually_says_so() {
    let mut p = sync_test_prompt();
    p.file_path = "C:\\docs\\manual.pdf".into();
    p.pdf_page = 3;
    // An older/hand-written file without the media headers leaves them alone…
    let legacy = "---\nname: x\ncolor: \ntags: \nnotes: \n---\nbody\n";
    let kept = apply_md_to_prompt(&p, legacy).unwrap();
    assert_eq!(kept.file_path, "C:\\docs\\manual.pdf");
    assert_eq!(kept.pdf_page, 3);
    // …while an explicitly empty one really does detach the file.
    let cleared = "---\nname: x\ncolor: \ntags: \nnotes: \nfile: \nicon: \n---\nbody\n";
    let out = apply_md_to_prompt(&p, cleared).unwrap();
    assert_eq!(out.file_path, "");
    assert_eq!(out.icon_path, "");
    // A garbled page number is ignored, never reset to the front page.
    let garbled = "---\nname: x\ncolor: \ntags: \nnotes: \npdf_page: seven\n---\nbody\n";
    assert_eq!(apply_md_to_prompt(&p, garbled).unwrap().pdf_page, 3);
}

#[test]
fn sync_write_all_is_a_no_op_when_the_feature_is_off() {
    let dir = std::env::temp_dir().join(format!("ps-sync-test-off-{}", gen_id()));
    let mut settings = Settings::default();
    settings.ui_texts.insert("syncDir".into(), dir.to_string_lossy().into_owned());
    // sync_enabled() reads ui_flags, left empty here -> off by default.
    let store = Store {
        prompts: vec![sync_test_prompt()],
        trash: Vec::new(),
        settings,
        prompts_locked: false,
        written: Default::default(),
    };
    // No AppHandle in a unit test, so exercise the same guard sync_write_all()
    // itself checks first: this is what keeps every non-sync-user's save_prompts
    // call free of any filesystem work.
    assert!(!sync_enabled(&store.settings));
    assert!(!dir.exists());
}

#[test]
fn sync_lock_is_exclusive_until_released() {
    let dir = std::env::temp_dir().join(format!("ps-sync-lock-test-{}", gen_id()));
    fs::create_dir_all(&dir).unwrap();
    let md_path = dir.join("p1.md");
    fs::write(&md_path, "x").unwrap();
    assert!(sync_lock_acquire(&md_path), "first acquire must succeed");
    assert!(!sync_lock_acquire(&md_path), "a fresh lock must never look stale");
    sync_lock_release(&md_path);
    assert!(sync_lock_acquire(&md_path), "must be acquirable again after release");
    sync_lock_release(&md_path);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_agents_md_is_written_once_and_never_overwritten() {
    let dir = std::env::temp_dir().join(format!("ps-sync-agents-test-{}", gen_id()));
    fs::create_dir_all(&dir).unwrap();
    sync_write_agents_md(&dir);
    let first = fs::read_to_string(dir.join("AGENTS.md")).unwrap();
    // Whatever this build is called: a literal here would have to be rewritten
    // for every fork, which is exactly what the brand layer exists to avoid.
    assert!(first.contains(brand::DISPLAY_NAME));
    assert!(first.contains(brand::ITEM));
    fs::write(dir.join("AGENTS.md"), "user's own edit, must survive").unwrap();
    sync_write_agents_md(&dir);
    assert_eq!(fs::read_to_string(dir.join("AGENTS.md")).unwrap(), "user's own edit, must survive");
    let _ = fs::remove_dir_all(&dir);
}

// ---- The folder the files live in, and letting go of an old one ----

pub(super) fn sync_tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ps-sync-{tag}-{}", gen_id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

// Writes a file with exactly the header md_is_ours() demands.
pub(super) fn write_ours(dir: &Path, name: &str, text: &str) {
    fs::write(dir.join(name), format!("---\nname: n\ncolor: \ntags: \nnotes: \n---\n{text}\n")).unwrap();
}

#[test]
fn sync_default_folder_is_named_after_the_brand_not_a_fixed_word() {
    // Fed a different slug it must produce that slug's folder: a hard-written
    // name would come out the same for every brand and fail right here.
    assert_eq!(sync_dir_leaf("other-brand"), "other-brand-sync");
    assert!(sync_dir_leaf(brand::SLUG).starts_with(brand::SLUG));
    assert_ne!(sync_dir_leaf(brand::SLUG), SYNC_LEGACY_DIR_NAME);
}

// The main window's JavaScript as one text: the module list in tests/ui_sources.json
// (concatenation order), read at run time so a module added there is picked up.
pub(super) fn main_window_js() -> String {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let list = std::fs::read_to_string(root.join("tests").join("ui_sources.json")).expect("tests/ui_sources.json");
    let list: serde_json::Value = serde_json::from_str(&list).expect("ui_sources.json parses");
    list["main_window_modules"]
        .as_array()
        .expect("main_window_modules is a list")
        .iter()
        .map(|name| {
            let name = name.as_str().expect("module name");
            std::fs::read_to_string(root.join("ui").join(name)).unwrap_or_else(|e| panic!("ui/{name}: {e}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn sync_adopts_the_old_default_folder_once_and_only_for_an_unset_path() {
    let legacy = sync_tmp_dir("legacy");
    write_ours(&legacy, "p1.md", "body");
    let mut settings = Settings::default();
    let adopted = sync_adopt_legacy_dir(&mut settings, &legacy).expect("an existing folder with our files is adopted");
    assert_eq!(adopted, legacy.to_string_lossy());
    assert_eq!(settings.ui_texts.get("syncDir").map(String::as_str), Some(adopted.as_str()));
    // Once. A later run must never point the folder somewhere the user has since
    // moved away from.
    settings.ui_texts.insert("syncDir".into(), "D:\\elsewhere".into());
    assert!(sync_adopt_legacy_dir(&mut settings, &legacy).is_none());
    assert_eq!(settings.ui_texts.get("syncDir").map(String::as_str), Some("D:\\elsewhere"));

    // Somebody who already picked their own folder is left alone.
    let mut own = Settings::default();
    own.ui_texts.insert("syncDir".into(), "D:\\mine".into());
    assert!(sync_adopt_legacy_dir(&mut own, &legacy).is_none());
    assert_eq!(own.ui_texts.get("syncDir").map(String::as_str), Some("D:\\mine"));

    // An empty leftover folder is not something to adopt.
    let empty = sync_tmp_dir("legacy-empty");
    let mut fresh = Settings::default();
    assert!(sync_adopt_legacy_dir(&mut fresh, &empty).is_none());
    assert_eq!(fresh.ui_texts.get("syncDir").map(String::as_str), None);
    // A folder that is not there at all is the normal case for a new install.
    let mut newest = Settings::default();
    assert!(sync_adopt_legacy_dir(&mut newest, &empty.join("gone")).is_none());

    let _ = fs::remove_dir_all(&legacy);
    let _ = fs::remove_dir_all(&empty);
}

#[test]
fn sync_sweep_leaves_a_users_own_copy_of_one_of_our_files() {
    let dir = sync_tmp_dir("sweep-copy");
    write_ours(&dir, "p1.md", "kept, still in the library");
    // A copy the user made by hand. It carries our header (it is a copy of our
    // file), but its name could never be a prompt id, so it is not an orphan of
    // ours, and deleting it would destroy a file the user put there deliberately.
    write_ours(&dir, "p1 - Copy.md", "the user's own copy");
    write_ours(&dir, "gone.md", "belongs to a prompt that no longer exists");
    let wanted: std::collections::HashSet<String> = ["p1.md".to_string()].into_iter().collect();
    sync_known_note(&dir.join("p1 - Copy.md"));
    sync_known_note(&dir.join("gone.md"));
    sync_seen_record_own_write(&dir.join("gone.md"));
    sync_sweep_orphans(&dir, &wanted);
    assert!(dir.join("p1.md").exists(), "a live prompt's file stays");
    assert!(dir.join("p1 - Copy.md").exists(), "a name this app could never have written is never swept");
    assert!(!dir.join("gone.md").exists(), "a real orphan of ours is still cleaned up");
    let _ = fs::remove_dir_all(&dir);
}

pub(super) fn sync_apply_store(prompts: Vec<Prompt>, trash: Vec<Prompt>) -> Store {
    Store { prompts, trash, settings: Settings::default(), prompts_locked: false, written: Default::default() }
}

pub(super) fn sync_apply(store: &mut Store, stem: &str, content: String, is_new: bool) -> (bool, bool) {
    sync_poll_apply(store, vec![(stem.to_string(), content, is_new)], &mut |_, _, _| {})
}

#[test]
fn sync_poll_never_writes_a_files_plaintext_into_a_confidential_prompt() {
    let mut secret = sync_test_prompt();
    secret.secret = true;
    secret.text = "CIPHERTEXT".into();
    let mut plain = sync_test_prompt();
    plain.text = "plaintext from another machine".into();
    let content = prompt_to_md(&plain, &SyncPlacement::default());
    let mut store = sync_apply_store(vec![secret], Vec::new());
    let (changed, _) = sync_apply(&mut store, "p1", content, false);
    assert!(!changed);
    assert_eq!(store.prompts[0].text, "CIPHERTEXT");
    assert!(store.prompts[0].secret);
}

#[test]
fn sync_write_leaves_a_file_the_app_has_never_known() {
    let dir = sync_tmp_dir("foreign-file");
    write_ours(&dir, "foreign.md", "another machine's prompt, not imported yet");
    let mut keep = sync_test_prompt();
    keep.id = "mine".into();
    sync_write_dir(&dir, &[keep], &Settings::default());
    assert!(dir.join("foreign.md").exists(), "a file the app never knew must survive the sweep");
    assert!(dir.join("mine.md").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_write_still_sweeps_the_file_of_a_prompt_that_was_deleted() {
    let dir = sync_tmp_dir("deleted-file");
    let mut a = sync_test_prompt();
    a.id = "a".into();
    let mut b = sync_test_prompt();
    b.id = "b".into();
    sync_write_dir(&dir, &[a.clone(), b], &Settings::default());
    assert!(dir.join("b.md").exists());
    sync_write_dir(&dir, &[a], &Settings::default());
    assert!(!dir.join("b.md").exists(), "a deleted prompt takes its own file along");
    assert!(dir.join("a.md").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_write_keeps_a_conflict_copy_of_an_edit_the_poll_noted_but_not_read() {
    let dir = sync_tmp_dir("unread-edit");
    let path = dir.join("p1.md");
    let mut edited = sync_test_prompt();
    edited.text = "edited outside the app".into();
    fs::write(&path, prompt_to_md(&edited, &SyncPlacement::default())).unwrap();
    // The poll has seen this stamp (Settle) but has not read the file yet.
    let stamp = sync_stamp_of(&path).unwrap();
    sync_seen_lock().insert(sync_seen_key(&path), SyncSeen { stamp, handled: false, hash: 0 });
    let mut app_side = sync_test_prompt();
    app_side.text = "changed inside the app".into();
    sync_write_dir(&dir, &[app_side], &Settings::default());
    let copies = fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("p1.conflict-"))
        .count();
    assert_eq!(copies, 1, "the unread outside edit must be kept as a conflict copy");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_write_copies_an_undecodable_outside_edit_byte_for_byte() {
    let mut edited = sync_test_prompt();
    edited.text = "edited outside the app".into();
    let md = prompt_to_md(&edited, &SyncPlacement::default());
    let mut utf16 = vec![0xFFu8, 0xFE];
    for u in md.encode_utf16() {
        utf16.extend_from_slice(&u.to_le_bytes());
    }
    let mut ansi = md.clone().into_bytes();
    ansi.extend_from_slice(b"caf");
    ansi.push(0xE4);
    ansi.push(b'\n');
    // (tag, bytes, the poll already marked the stamp handled)
    for (tag, bytes, handled) in [("utf16", utf16, false), ("ansi", ansi, true)] {
        let dir = sync_tmp_dir(tag);
        let path = dir.join("p1.md");
        fs::write(&path, &bytes).unwrap();
        let stamp = sync_stamp_of(&path).unwrap();
        sync_seen_lock().insert(sync_seen_key(&path), SyncSeen { stamp, handled, hash: 0 });
        let mut app_side = sync_test_prompt();
        app_side.text = "changed inside the app".into();
        sync_write_dir(&dir, &[app_side], &Settings::default());
        let copies: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("p1.conflict-"))
            .collect();
        assert_eq!(copies.len(), 1, "{tag}: the outside edit must be kept as a conflict copy");
        assert_eq!(fs::read(copies[0].path()).unwrap(), bytes, "{tag}: the copy must be byte-identical");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn conflict_copies_in_the_same_second_never_overwrite_each_other() {
    let dir = sync_tmp_dir("conflict-names");
    let a = sync_write_conflict_copy(&dir, "p1", "", b"one").unwrap();
    let b = sync_write_conflict_copy(&dir, "p1", "", b"two").unwrap();
    let c = sync_write_conflict_copy(&dir, "p1", "-late", b"three").unwrap();
    let d = sync_write_conflict_copy(&dir, "p1", "-late", b"four").unwrap();
    let all = [&a, &b, &c, &d];
    for (i, x) in all.iter().enumerate() {
        for y in &all[i + 1..] {
            assert_ne!(x, y);
        }
    }
    assert_eq!(fs::read(&a).unwrap(), b"one");
    assert_eq!(fs::read(&b).unwrap(), b"two");
    assert_eq!(fs::read(&d).unwrap(), b"four");
    let _ = fs::remove_dir_all(&dir);
}

pub(super) fn sync_conflict_copies(dir: &Path, id: &str) -> usize {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&format!("{id}.conflict-")))
        .count()
}

pub(super) fn sync_legacy_md(p: &Prompt) -> String {
    prompt_to_md_impl(p, &SyncPlacement::default(), true)
}

#[test]
fn sync_startup_seed_keeps_an_edit_made_while_the_app_was_closed() {
    let dir = sync_tmp_dir("startup-seed");
    let mut stored = sync_test_prompt();
    stored.text = "the text this library holds".into();
    let mut other = sync_test_prompt();
    other.id = "p2".into();
    other.text = "ends with a newline\n".into();
    other.notes = "two\nlines".into();
    // p1 was edited by hand while the app was closed; p2 is an older build's file.
    let mut hand = stored.clone();
    hand.text = "typed by hand in the editor".into();
    fs::write(dir.join("p1.md"), prompt_to_md(&hand, &SyncPlacement::default())).unwrap();
    fs::write(dir.join("p2.md"), sync_legacy_md(&other)).unwrap();
    sync_seed_seen(&dir, &[stored.clone(), other.clone()], &Settings::default());
    let mut edited = stored.clone();
    edited.text = "changed inside the app".into();
    let mut edited2 = other.clone();
    edited2.text = "changed inside the app too".into();
    sync_write_dir(&dir, &[edited, edited2], &Settings::default());
    assert_eq!(sync_conflict_copies(&dir, "p1"), 1, "the hand edit must survive as a copy");
    assert_eq!(sync_conflict_copies(&dir, "p2"), 0, "a file matching the stored prompt needs no copy");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_first_write_into_a_folder_keeps_a_differing_file_and_not_a_matching_one() {
    let dir = sync_tmp_dir("activation");
    let mut stored = sync_test_prompt();
    stored.text = "the text this library holds".into();
    let mut other = stored.clone();
    other.id = "p2".into();
    other.text = "ends with a newline\n".into();
    other.notes = "two\nlines".into();
    let mut foreign = stored.clone();
    foreign.text = "another machine's version".into();
    fs::write(dir.join("p1.md"), prompt_to_md(&foreign, &SyncPlacement::default())).unwrap();
    fs::write(dir.join("p2.md"), sync_legacy_md(&other)).unwrap();
    // No seed, no SYNC_SEEN entry: activation, or a switch to a folder with files.
    sync_write_dir(&dir, &[stored, other], &Settings::default());
    assert_eq!(sync_conflict_copies(&dir, "p1"), 1, "a differing file must be kept as a copy");
    assert_eq!(sync_conflict_copies(&dir, "p2"), 0, "a file in an older format of the same prompt is no conflict");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_write_keeps_a_copy_of_a_file_that_arrived_while_its_prompt_was_trashed() {
    let dir = sync_tmp_dir("trash-restore");
    let path = dir.join("p1.md");
    let mut theirs = sync_test_prompt();
    theirs.text = "written by another machine meanwhile".into();
    fs::write(&path, prompt_to_md(&theirs, &SyncPlacement::default())).unwrap();
    // The poll saw it and skipped the trashed id: handled, but never known.
    let stamp = sync_stamp_of(&path).unwrap();
    sync_seen_lock().insert(sync_seen_key(&path), SyncSeen { stamp, handled: true, hash: 0 });
    sync_known_lock().remove(&sync_seen_key(&path));
    let mut restored = sync_test_prompt();
    restored.text = "the restored prompt".into();
    sync_write_dir(&dir, &[restored], &Settings::default());
    assert_eq!(sync_conflict_copies(&dir, "p1"), 1, "restoring must not overwrite the newer file silently");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_sweep_leaves_a_file_with_an_edit_not_read_yet() {
    let dir = sync_tmp_dir("sweep-unread");
    let mut a = sync_test_prompt();
    a.id = "a".into();
    let mut b = sync_test_prompt();
    b.id = "b".into();
    sync_write_dir(&dir, &[a.clone(), b.clone()], &Settings::default());
    // An outside edit lands on b.md; the poll has noted its stamp, not read it.
    let path = dir.join("b.md");
    let mut edited = b.clone();
    edited.text = "edited outside the app".into();
    fs::write(&path, prompt_to_md(&edited, &SyncPlacement::default())).unwrap();
    let stamp = sync_stamp_of(&path).unwrap();
    sync_seen_lock().insert(sync_seen_key(&path), SyncSeen { stamp, handled: false, hash: 0 });
    sync_write_dir(&dir, &[a], &Settings::default());
    assert!(path.exists(), "an edit nobody has read must not be swept");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_sweep_spares_a_file_whose_prompt_came_back_meanwhile() {
    let dir = sync_tmp_dir("sweep-live");
    let mut a = sync_test_prompt();
    a.id = "a".into();
    let mut b = sync_test_prompt();
    b.id = "b".into();
    sync_write_dir(&dir, &[a.clone(), b], &Settings::default());
    // A stale snapshot without b, while the library holds b by now (imported).
    sync_write_dir_checked(&dir, &[a], &Settings::default(), &|id| id == "b");
    assert!(dir.join("b.md").exists(), "a file whose prompt is live again must not be swept");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_sweep_removes_the_file_of_an_entry_that_became_confidential() {
    let dir = sync_tmp_dir("sweep-secret");
    let mut a = sync_test_prompt();
    a.id = "a".into();
    let mut b = sync_test_prompt();
    b.id = "b".into();
    sync_write_dir(&dir, &[a.clone(), b.clone()], &Settings::default());
    assert!(dir.join("b.md").exists());
    b.secret = true;
    let library = vec![a.clone(), b];
    sync_write_dir_checked(&dir, &[a], &Settings::default(), &|id| sync_id_is_live(&library, id));
    assert!(!dir.join("b.md").exists(), "the plaintext file of a now confidential entry must go");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_write_ignores_a_touch_that_left_the_bytes_alone() {
    let dir = sync_tmp_dir("touch-only");
    let path = dir.join("p1.md");
    let first = sync_test_prompt();
    sync_write_dir(&dir, std::slice::from_ref(&first), &Settings::default());
    // A cloud client re-touches the file: same bytes, newer modification time.
    let bytes = fs::read(&path).unwrap();
    let f = fs::OpenOptions::new().write(true).open(&path).unwrap();
    f.set_modified(std::time::SystemTime::now() + Duration::from_secs(30)).unwrap();
    drop(f);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let mut edited = first;
    edited.text = "changed inside the app".into();
    sync_write_dir(&dir, &[edited], &Settings::default());
    assert_eq!(sync_conflict_copies(&dir, "p1"), 0, "the app's own earlier content is no outside edit");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_requeued_candidate_is_read_again_and_not_overwritten_unseen() {
    let dir = sync_tmp_dir("requeue");
    let path = dir.join("p1.md");
    let mut edited = sync_test_prompt();
    edited.text = "edited outside the app".into();
    fs::write(&path, prompt_to_md(&edited, &SyncPlacement::default())).unwrap();
    let stamp = sync_stamp_of(&path).unwrap();
    let key = sync_seen_key(&path);
    // The poll read the file and marked it handled, but could not apply it.
    sync_seen_lock().insert(key.clone(), SyncSeen { stamp, handled: true, hash: sync_hash(&fs::read(&path).unwrap()) });
    sync_requeue_candidates(&dir, &[("p1".to_string(), String::new(), false)]);
    let entry = sync_seen_lock().get(&key).copied().unwrap();
    assert!(!entry.handled);
    assert_eq!(sync_scan_decision(Some(entry), stamp), SyncScan::Read);
    let mut app_side = sync_test_prompt();
    app_side.text = "changed inside the app".into();
    sync_write_dir(&dir, &[app_side], &Settings::default());
    assert_eq!(sync_conflict_copies(&dir, "p1"), 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_scan_keeps_the_records_of_other_folders() {
    let a = sync_tmp_dir("scan-a");
    let b = sync_tmp_dir("scan-b");
    let in_b = b.join("p1.md");
    fs::write(&in_b, "x").unwrap();
    sync_seen_record_own_write(&in_b);
    let gone = a.join("gone.md");
    sync_seen_lock().insert(sync_seen_key(&gone), SyncSeen { stamp: SyncStamp { mtime_ns: 1, len: 1 }, handled: true, hash: 0 });
    sync_scan_folder(&a).unwrap();
    assert!(sync_seen_lock().contains_key(&sync_seen_key(&in_b)), "another folder's record must survive a scan");
    assert!(!sync_seen_lock().contains_key(&sync_seen_key(&gone)), "a vanished file in the scanned folder is dropped");
    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
}

#[test]
fn sync_same_dir_compares_by_identity_not_spelling() {
    let dir = sync_tmp_dir("same-dir");
    let name = dir.file_name().unwrap().to_string_lossy().to_uppercase();
    let shouting = dir.with_file_name(name);
    assert_ne!(dir, shouting, "a plain path compare (the old check) calls them different");
    assert!(sync_same_dir(&dir, &shouting), "the same folder in another letter case is the same folder");
    assert!(sync_same_dir(&dir, &dir.join(".")));
    let other = sync_tmp_dir("same-dir-other");
    assert!(!sync_same_dir(&dir, &other));
    assert!(sync_same_dir(&dir, &dir.join("inner")), "a folder inside the other counts too");
    assert!(
        sync_same_dir(&dir, &dir.with_file_name("ps-sync-no-such-folder-anywhere")),
        "a path that cannot be resolved is treated as maybe the same, so nothing gets deleted"
    );
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&other);
}

#[test]
fn sync_poll_does_not_import_a_file_whose_id_sits_in_the_trash() {
    let trashed = sync_test_prompt();
    let content = prompt_to_md(&trashed, &SyncPlacement::default());
    let mut store = sync_apply_store(Vec::new(), vec![trashed]);
    let (changed, _) = sync_apply(&mut store, "p1", content, true);
    assert!(!changed);
    assert!(store.prompts.is_empty(), "a trashed id must not come back as a second, live prompt");
    assert_eq!(store.trash.len(), 1);
}

#[test]
fn sync_poll_apply_reports_settings_changed_for_a_placement_only_file() {
    let p = sync_test_prompt();
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    let key = grid_key(settings.views[0].cols, settings.views[0].rows);
    settings.views[0].layouts.entry(key.clone()).or_default().insert("p1".into(), [0, 0]);
    let mut moved = settings.clone();
    moved.views[0].layouts.get_mut(&key).unwrap().insert("p1".into(), [2, 1]);
    let content = prompt_to_md(&p, &sync_placement_for(&p, &moved));
    let mut store = sync_apply_store(vec![p], Vec::new());
    store.settings = settings;
    let (changed, settings_changed) = sync_apply(&mut store, "p1", content, false);
    assert!(!changed, "a file that only moved the tile must not count as a changed prompt");
    assert!(settings_changed, "the flag the store-synced event keys on must be set for a moved tile");
    assert_eq!(store.settings.views[0].layouts[&key]["p1"], [2, 1]);
}

#[test]
fn sync_join_adopts_the_cell_and_colour_of_an_existing_grid_folder() {
    let p = sync_test_prompt();
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    settings.tile_folders.push({
        let mut f = test_tile_folder("f1", "v1", [0, 0], &["p1"]);
        f.name = "F".to_string();
        f
    });
    let grid = grid_key(settings.views[0].cols, settings.views[0].rows);
    let incoming_placement = SyncPlacement {
        view: "V1".into(),
        view_id: "v1".into(),
        grid,
        cell: "3,1".into(),
        tile_folder: "F".into(),
        tile_folder_color: "#ef4444".into(),
        float_folder: String::new(),
    };
    let incoming = prompt_to_md(&p, &incoming_placement);
    let (_, fields_body) = parse_frontmatter(&incoming).unwrap();
    let _ = fields_body;
    let fields = parse_frontmatter(&incoming).unwrap().0;
    apply_md_placement(&mut settings, "p1", &fields);
    assert_eq!(settings.tile_folders.len(), 1);
    assert_eq!(settings.tile_folders[0].cell, [3, 1]);
    assert_eq!(settings.tile_folders[0].color, "#ef4444");
    assert_eq!(
        prompt_to_md(&p, &sync_placement_for(&p, &settings)),
        incoming,
        "after adopting, this machine writes exactly what it read, so the two stop rewriting each other"
    );
}

#[test]
fn sync_join_keeps_a_folders_cell_when_the_file_carries_none() {
    let mut settings = Settings::default();
    settings.views.push(test_view("v1"));
    settings.tile_folders.push({
        let mut f = test_tile_folder("f1", "v1", [2, 2], &["a"]);
        f.name = "F".to_string();
        f
    });
    assert!(sync_join_tile_folder(&mut settings, "b", "F", "", "v1", None));
    assert_eq!(settings.tile_folders[0].cell, [2, 2]);
}

#[test]
fn a_byte_order_mark_does_not_hide_an_edited_file() {
    let base = sync_test_prompt();
    let mut edited = sync_test_prompt();
    edited.text = "edited in an editor that adds a BOM".into();
    let content = format!("\u{feff}{}", prompt_to_md(&edited, &SyncPlacement::default()));
    let got = apply_md_to_prompt(&base, &content).expect("a BOM must not make the header unreadable");
    assert_eq!(got.text, edited.text);
}

#[test]
fn sync_decode_text_reads_utf8_bom_and_utf16_files() {
    let md = prompt_to_md(&sync_test_prompt(), &SyncPlacement::default());
    assert_eq!(sync_decode_text(md.clone().into_bytes()).as_deref(), Some(md.as_str()));
    let mut bom8 = vec![0xEF, 0xBB, 0xBF];
    bom8.extend_from_slice(md.as_bytes());
    assert_eq!(sync_decode_text(bom8).as_deref(), Some(md.as_str()));
    let mut le = vec![0xFF, 0xFE];
    le.extend(md.encode_utf16().flat_map(|u| u.to_le_bytes()));
    assert_eq!(sync_decode_text(le).as_deref(), Some(md.as_str()));
    let mut be = vec![0xFE, 0xFF];
    be.extend(md.encode_utf16().flat_map(|u| u.to_be_bytes()));
    assert_eq!(sync_decode_text(be).as_deref(), Some(md.as_str()));
    assert!(sync_decode_text(vec![0xFF, 0xFE, 0x41]).is_none(), "an odd UTF-16 length is not text");
    assert!(sync_decode_text(vec![0xC3, 0x28]).is_none());
}

#[test]
fn a_round_trip_keeps_multi_line_notes_and_a_trailing_newline() {
    let mut p = sync_test_prompt();
    p.notes = "a\nb\\c\r\nd".into();
    p.text = "abc\n".into();
    let back = apply_md_to_prompt(&p, &prompt_to_md(&p, &SyncPlacement::default())).unwrap();
    assert_eq!(back.notes, p.notes);
    assert_eq!(back.text, p.text);
    p.text = "one\n\n".into();
    p.variants = vec!["two".into()];
    let back = apply_md_to_prompt(&p, &prompt_to_md(&p, &SyncPlacement::default())).unwrap();
    assert_eq!(back.text, p.text);
    assert_eq!(back.variants, p.variants);
    // A note without a line break keeps its exact old form: no churn for files that round-tripped fine.
    let mut plain = sync_test_prompt();
    plain.notes = "C:\\new\\file is fine".into();
    let md = prompt_to_md(&plain, &SyncPlacement::default());
    assert!(!md.contains("notes_enc"), "{md}");
    assert_eq!(apply_md_to_prompt(&plain, &md).unwrap().notes, "C:\\new\\file is fine");
}

#[test]
fn sync_startup_seed_lets_the_next_write_sweep_a_plaintext_file_of_a_now_confidential_prompt() {
    let dir = sync_tmp_dir("secret-left-behind");
    let mut p = sync_test_prompt();
    fs::write(dir.join("p1.md"), prompt_to_md(&p, &SyncPlacement::default())).unwrap();
    let foreign = dir.join("p9.md");
    fs::write(&foreign, "just my own notes, no header").unwrap();
    // The prompt became confidential while sync was off; the file stayed behind.
    p.secret = true;
    sync_seed_seen(&dir, &[p.clone()], &Settings::default());
    sync_write_dir(&dir, &[p], &Settings::default());
    assert!(!dir.join("p1.md").exists(), "the plaintext copy of a confidential prompt must not survive");
    assert!(foreign.exists(), "a file that is not ours is never touched");
    let _ = fs::remove_dir_all(&dir);
}

// The checks below compare multi-line source text; a checkout with Windows line endings
// must read the same as one with Unix endings.
macro_rules! source {
    ($path:literal) => {
        include_str!($path).replace("\r\n", "\n")
    };
}

#[test]
fn a_synced_file_keeps_a_network_path_as_written() {
    let mut p = sync_blank_prompt("p1".into());
    p.file_path = String::new();
    let md = format!("---\nname: x\ncolor: \ntags: \nnotes: \nfile: {}\nicon: //nas/ps/i.png\n---\nbody\n", r"\\nas\ps\a.pdf");
    let out = apply_md_to_prompt(&p, &md).unwrap();
    assert_eq!(out.file_path, r"\\nas\ps\a.pdf");
    assert_eq!(out.icon_path, "//nas/ps/i.png");
    let src = source!("../md_sync.rs");
    assert!(!src.contains("is_network_path"), "the sync reader no longer blanks a path");
}

#[test]
fn a_synced_file_keeps_the_paths_it_names() {
    let p = md_sync_1::sync_test_prompt();
    let md = prompt_to_md(&p, &SyncPlacement::default());
    assert!(md.contains("file: \n") && md.contains("icon: \n"), "the fixture writes empty media lines");
    let net = md.replace("file: \n", "file: \\\\host\\share\\x.mp4\n").replace("icon: \n", "icon: //host/share/i.png\n");
    let got = apply_md_to_prompt(&p, &net).expect("valid frontmatter");
    assert_eq!(got.file_path, r"\\host\share\x.mp4", "a network file path from the folder is kept as written");
    assert_eq!(got.icon_path, "//host/share/i.png", "a network icon path from the folder is kept as written");
    let local = md.replace("file: \n", "file: C:/clips/a.mp4\n").replace("icon: \n", "icon: C:/clips/i.png\n");
    let got = apply_md_to_prompt(&p, &local).unwrap();
    assert_eq!((got.file_path.as_str(), got.icon_path.as_str()), ("C:/clips/a.mp4", "C:/clips/i.png"));
}

#[test]
fn the_startup_seed_removes_a_plaintext_file_of_a_confidential_prompt_itself() {
    let dir = md_sync_1::sync_tmp_dir("seed-removes-secret");
    let mut p = md_sync_1::sync_test_prompt();
    std::fs::write(dir.join("p1.md"), prompt_to_md(&p, &SyncPlacement::default())).unwrap();
    let foreign = dir.join("p9.md");
    std::fs::write(&foreign, "just my own notes, no header").unwrap();
    p.secret = true;
    let mut other = md_sync_1::sync_test_prompt();
    other.id = "p9".into();
    other.secret = true;
    sync_seed_seen(&dir, &[p, other], &Settings::default());
    assert!(!dir.join("p1.md").exists(), "the seed itself removes our plaintext copy");
    assert!(foreign.exists(), "a file that is not ours is never touched");
    let _ = std::fs::remove_dir_all(&dir);
}
