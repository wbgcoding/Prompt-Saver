use super::*;

// ---- Folder import ----

#[test]
fn scan_import_folder_turns_subfolders_into_tags_and_reads_only_md_and_txt() {
    let dir = std::env::temp_dir().join(format!("ps-import-folder-tags-{}", std::process::id()));
    let sub = dir.join("Work").join("Clients");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join("Note.txt"), b"Hello").unwrap();
    fs::write(dir.join("Top.md"), b"Top level").unwrap();
    fs::write(dir.join("ignored.jpg"), b"not text").unwrap();
    let (items, truncated) = scan_import_folder(&dir, 5, 500);
    assert!(!truncated);
    assert_eq!(items.len(), 2, "only the two text files count, never the image");
    let nested = items.iter().find(|i| i.name == "Note").unwrap();
    assert_eq!(nested.tags, vec!["Work".to_string(), "Clients".to_string()]);
    let top = items.iter().find(|i| i.name == "Top").unwrap();
    assert!(top.tags.is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn split_folder_note_extracts_a_short_first_line_and_keeps_a_long_one_as_text() {
    let (text, note) = split_folder_note("Shopping list\nMilk\nEggs");
    assert_eq!(note, "Shopping list");
    assert_eq!(text, "Milk\nEggs");
    let long_first = "x".repeat(IMPORT_FOLDER_NOTE_MAX_CHARS + 1);
    let (text2, note2) = split_folder_note(&format!("{long_first}\nrest"));
    assert!(note2.is_empty());
    assert_eq!(text2, format!("{long_first}\nrest"));
    // A one-line file has no "rest" left to separate the note from.
    let (text3, note3) = split_folder_note("Just one line");
    assert!(note3.is_empty());
    assert_eq!(text3, "Just one line");
}

#[test]
fn import_folder_files_reads_a_rotation_separator_into_variants() {
    let dir = std::env::temp_dir().join(format!("ps-import-folder-rotate-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("Greeting.txt"), format!("Hi{VARIANT_SEP}Hello{VARIANT_SEP}Hey")).unwrap();
    let content = read_import_folder_file(&dir.join("Greeting.txt")).unwrap();
    let (raw_text, variants) = split_variants(&content);
    let (text, note) = split_folder_note(&raw_text);
    assert!(note.is_empty());
    assert_eq!(text, "Hi");
    assert_eq!(variants, vec!["Hello".to_string(), "Hey".to_string()]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_import_folder_leaves_out_a_folder_past_the_depth_limit_entirely() {
    let dir = std::env::temp_dir().join(format!("ps-import-folder-depth-{}", std::process::id()));
    let deep = dir.join("a").join("b").join("c");
    fs::create_dir_all(&deep).unwrap();
    fs::write(dir.join("a").join("shallow.txt"), b"shallow").unwrap();
    fs::write(deep.join("deep.txt"), b"deep").unwrap();
    // depth 2 = the base folder plus one level of subfolders: "a" is
    // included, "a/b" is already past the limit and never descended into.
    let (items, _) = scan_import_folder(&dir, 2, 500);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "shallow");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_import_folder_stops_at_the_maximum_and_reports_truncation() {
    let dir = std::env::temp_dir().join(format!("ps-import-folder-max-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for i in 0..5 {
        fs::write(dir.join(format!("n{i}.txt")), b"x").unwrap();
    }
    let (items, truncated) = scan_import_folder(&dir, 5, 3);
    assert_eq!(items.len(), 3);
    assert!(truncated);
    let (items2, truncated2) = scan_import_folder(&dir, 5, 5);
    assert_eq!(items2.len(), 5);
    assert!(!truncated2);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_import_folder_marks_an_oversized_or_binary_file_unreadable_with_a_reason() {
    let dir = std::env::temp_dir().join(format!("ps-import-folder-oversize-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let huge = vec![b'a'; (MAX_IMPORT_FOLDER_FILE_BYTES + 1) as usize];
    fs::write(dir.join("huge.txt"), &huge).unwrap();
    fs::write(dir.join("bad.txt"), [0xff, 0xfe, 0x00]).unwrap();
    fs::write(dir.join("ok.txt"), b"fine").unwrap();
    let (items, _) = scan_import_folder(&dir, 5, 500);
    let huge_item = items.iter().find(|i| i.name == "huge").unwrap();
    assert!(!huge_item.readable);
    assert_eq!(huge_item.skip_reason, "too-large");
    let bad_item = items.iter().find(|i| i.name == "bad").unwrap();
    assert!(!bad_item.readable);
    assert_eq!(bad_item.skip_reason, "bad-encoding");
    let ok_item = items.iter().find(|i| i.name == "ok").unwrap();
    assert!(ok_item.readable);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn import_folder_files_appends_a_prompt_with_tags_note_and_variants() {
    let dir = std::env::temp_dir().join(format!("ps-import-folder-confirm-{}", std::process::id()));
    let sub = dir.join("Recipes");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join("Cake.txt"), format!("A short title\nFlour, sugar, eggs{VARIANT_SEP}Butter, sugar, eggs")).unwrap();
    let mut store = Store {
        prompts: Vec::new(),
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: Default::default(),
    };
    let path = sub.join("Cake.txt");
    let content = read_import_folder_file(&path).unwrap();
    let tags = import_folder_tags(&dir, &path);
    let (raw_text, variants) = split_variants(&content);
    let (text, note) = split_folder_note(&raw_text);
    let items = vec![ImportedPrompt {
        name: "Cake".to_string(),
        text,
        color: String::new(),
        font: String::new(),
        font_size: 0,
        file_path: String::new(),
        pdf_page: 0,
        icon_path: String::new(),
        caption: String::new(),
        caption_size: 0,
        show_image: false,
        copy_image: false,
        image: String::new(),
        tags,
        notes: note,
        variants,
        rotate_mode: String::new(),
        favorite: false,
        secret: false,
    }];
    let count = append_imported_prompts(&mut store, items);
    assert_eq!(count, 1);
    let p = &store.prompts[0];
    assert_eq!(p.tags, vec!["Recipes".to_string()]);
    assert_eq!(p.notes, "A short title");
    assert_eq!(p.text, "Flour, sugar, eggs");
    assert_eq!(p.variants, vec!["Butter, sugar, eggs".to_string()]);
    let _ = fs::remove_dir_all(&dir);
}

// ---- Discard trash ----

#[test]
fn video_folder_stats_includes_the_verworfen_subfolder_in_its_totals() {
    let dir = std::env::temp_dir().join(format!("ps-video-folder-discard-stats-{}", std::process::id()));
    let trash = dir.join("verworfen");
    fs::create_dir_all(&trash).unwrap();
    fs::write(dir.join("rec-a.mp4"), b"1234").unwrap();
    fs::write(trash.join("rec-b.mp4"), b"12").unwrap();
    let (count, bytes) = video_folder_stats_impl(&dir.to_string_lossy(), &dir);
    assert_eq!(count, 2, "a discarded recording still occupies real disk space in this folder");
    assert_eq!(bytes, 4 + 2);
    let _ = fs::remove_dir_all(&dir);
}

// ---- Storage breakdown ("Speicher" section) ----

#[test]
fn scan_dir_stats_counts_files_one_level_deep_and_ignores_subfolders() {
    let dir = std::env::temp_dir().join(format!("ps-storage-scan-{}", std::process::id()));
    let nested = dir.join("nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(dir.join("a.txt"), b"12345").unwrap();
    fs::write(dir.join("b.txt"), b"12").unwrap();
    fs::write(nested.join("c.txt"), b"1234567890").unwrap();
    let (count, bytes, capped, denied) = scan_dir_stats(&dir, 100);
    assert_eq!(count, 2, "a file in a subfolder must not be counted (one level deep only)");
    assert_eq!(bytes, 5 + 2);
    assert!(!capped);
    assert!(!denied);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn backup_dir_stats_sums_the_snapshot_files_one_folder_down() {
    let dir = std::env::temp_dir().join(format!("ps-storage-backups-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for (stamp, bytes) in [("20260101-100000", 5usize), ("20260102-100000", 7)] {
        let snap = dir.join(stamp);
        fs::create_dir_all(&snap).unwrap();
        fs::write(snap.join("data.db.gz"), vec![0u8; bytes]).unwrap();
        fs::write(snap.join("names.json"), b"extra").unwrap();
    }
    fs::create_dir_all(dir.join("20260103-100000")).unwrap(); // no snapshot file: not a backup
    fs::write(dir.join("loose.txt"), b"1234567890").unwrap();
    let (count, bytes, capped, denied) = backup_dir_stats(&dir, 100);
    assert_eq!((count, bytes), (2, 22), "the row is the sum of every file inside the snapshot folders, not of the top-level folder or loose files");
    assert!(!capped && !denied);
    assert!(backup_dir_stats(&dir, 1).2, "the snapshot cap is reported");
    assert_eq!(backup_dir_stats(&dir.join("missing"), 10), (0, 0, false, false));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_dir_stats_stops_at_the_entry_cap() {
    let dir = std::env::temp_dir().join(format!("ps-storage-cap-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for i in 0..6 {
        fs::write(dir.join(format!("f{i}.txt")), b"x").unwrap();
    }
    let (count, _bytes, capped, denied) = scan_dir_stats(&dir, 3);
    assert_eq!(count, 3, "the scan must stop counting at the cap, not read every file first");
    assert!(capped);
    assert!(!denied);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_dir_stats_on_a_missing_folder_is_empty_not_denied() {
    let dir = std::env::temp_dir().join(format!("ps-storage-missing-{}", std::process::id()));
    assert_eq!(scan_dir_stats(&dir, 10), (0, 0, false, false));
}

// A permission failure is real Windows ACL behaviour this environment cannot
// reliably reproduce (needs elevation, and would leave a folder locked out
// for later tests): the read is injected instead, so the "must be
// reported, not concealed" contract is checked directly against the
// function's own logic.
#[test]
fn scan_dir_stats_with_reports_a_denied_folder_instead_of_hiding_it() {
    let denied_read = |_: &std::path::Path| -> std::io::Result<std::fs::ReadDir> {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    };
    let (count, bytes, capped, denied) =
        scan_dir_stats_with(std::path::Path::new("Z:/wherever"), 10, denied_read);
    assert_eq!((count, bytes, capped), (0, 0, false));
    assert!(denied, "a folder that exists but cannot be listed must be reported as denied");
}

#[test]
fn attachments_bytes_counts_a_shared_file_once_and_skips_missing_ones() {
    let dir = std::env::temp_dir().join(format!("ps-storage-attach-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let f = dir.join("report.pdf");
    fs::write(&f, b"1234567").unwrap();
    let path = f.to_string_lossy().to_string();
    let paths = vec![path.clone(), path, String::new(), "Z:/does-not-exist.pdf".to_string()];
    let (count, bytes) = attachments_bytes(&paths);
    assert_eq!(count, 1, "the same attachment referenced twice must be counted once");
    assert_eq!(bytes, 7);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn discard_recording_file_in_moves_into_verworfen_instead_of_deleting() {
    let dir = std::env::temp_dir().join(format!("ps-discard-move-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-2026-01-01-00-00-00.mp4");
    fs::write(&src, b"video").unwrap();
    let moved = discard_recording_file_in(std::slice::from_ref(&dir), &src).expect("must move, not fail");
    // discard_recording_file_in canonicalizes the parent internally (the \\?\
    // extended-length form on Windows): canonicalize the expected side too
    // rather than comparing a canonical path against a plain one.
    let want = dir.join("verworfen").join("rec-2026-01-01-00-00-00.mp4").canonicalize().unwrap();
    assert_eq!(moved, want);
    assert!(!src.exists(), "the original spot must be empty afterwards");
    assert!(moved.is_file(), "the file must really be at the new spot, not just claimed to be");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn discard_or_remove_take_keeps_a_cancelled_take_restorable() {
    let dir = std::env::temp_dir().join(format!("ps-cancel-take-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let take = dir.join("rec-2026-01-01-00-00-01.mp4");
    fs::write(&take, b"video").unwrap();
    fs::write(take.with_extension("mic.wav"), b"mic").unwrap();
    discard_or_remove_take(&take);
    assert!(!take.exists(), "the take leaves the recordings folder");
    assert!(dir.join("verworfen").join("rec-2026-01-01-00-00-01.mp4").is_file(), "it waits in the holding area");
    assert!(dir.join("verworfen").join("rec-2026-01-01-00-00-01.mic.wav").is_file(), "its mic track goes with it");
    // A file the holding area refuses (no rec- name) is still removed, never left behind.
    let other = dir.join("custom.mp4");
    fs::write(&other, b"video").unwrap();
    discard_or_remove_take(&other);
    assert!(!other.exists());
    assert!(!dir.join("verworfen").join("custom.mp4").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn discard_recording_file_in_refuses_a_name_without_the_rec_prefix() {
    let dir = std::env::temp_dir().join(format!("ps-discard-prefix-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("not-a-recording.mp4");
    fs::write(&src, b"video").unwrap();
    assert!(discard_recording_file_in(std::slice::from_ref(&dir), &src).is_none());
    assert!(src.is_file(), "a refused discard must leave the file untouched");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn discard_recording_file_in_refuses_a_file_outside_the_allowed_folders() {
    let dir = std::env::temp_dir().join(format!("ps-discard-outside-{}", std::process::id()));
    let other = std::env::temp_dir().join(format!("ps-discard-other-{}", std::process::id()));
    fs::create_dir_all(&other).unwrap();
    let src = other.join("rec-somewhere-else.mp4");
    fs::write(&src, b"video").unwrap();
    assert!(discard_recording_file_in(&[dir], &src).is_none(), "dir is not in the allowed list");
    assert!(src.is_file());
    let _ = fs::remove_dir_all(&other);
}

// ---------- security pass: picker-capability grants (set_*_dir gate) ----------

#[test]
fn folder_picker_title_rejects_an_unknown_purpose() {
    assert_eq!(folder_picker_title("exportVideoTypo", "en"), None);
    assert_eq!(folder_picker_title("", "en"), None);
    assert_eq!(folder_picker_title("data", "en"), Some("Choose the data folder"));
}

#[test]
fn folder_picker_title_maps_every_known_purpose_by_language() {
    assert_eq!(folder_picker_title("screenshot", "en"), Some("Choose the screenshot folder"));
    assert_eq!(folder_picker_title("screenshot", "de"), Some("Screenshot-Ordner wählen"));
    assert_eq!(folder_picker_title("videoExport", "en"), Some("Choose the video export folder"));
    assert_eq!(folder_picker_title("videoExport", "de"), Some("Exportordner für Videos wählen"));
    assert_eq!(folder_picker_title("recording", "en"), Some("Choose the recording folder"));
    assert_eq!(folder_picker_title("recording", "de"), Some("Aufnahmeordner wählen"));
    assert_eq!(folder_picker_title("sync", "en"), Some("Choose the sync folder"));
    assert_eq!(folder_picker_title("sync", "de"), Some("Sync-Ordner wählen"));
    assert_eq!(folder_picker_title("data", "en"), Some("Choose the data folder"));
    assert_eq!(folder_picker_title("data", "de"), Some("Datenordner wählen"));
    // Any other UI language falls back to English, same as the rest of
    // the app's backend-side strings.
    assert_eq!(folder_picker_title("sync", "fr"), Some("Choose the sync folder"));
}

#[test]
fn accept_dir_setting_rejects_a_path_that_was_never_picked() {
    let dir = std::env::temp_dir().join(format!("ps-grant-unpicked-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    let default = std::env::temp_dir().join(format!("ps-grant-unpicked-default-{}", std::process::id()));
    let err = accept_dir_setting(&dir_s, &default, "screenshot").expect_err("a dir the picker never returned must be refused");
    assert_eq!(err, "dir-not-picked");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn accept_dir_setting_accepts_a_granted_dir_exactly_once() {
    let dir = std::env::temp_dir().join(format!("ps-grant-once-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    let default = std::env::temp_dir().join(format!("ps-grant-once-default-{}", std::process::id()));
    grant_picked_dir(&dir, "screenshot");
    assert_eq!(accept_dir_setting(&dir_s, &default, "screenshot").unwrap(), dir_s, "the just-picked dir must be accepted");
    let err = accept_dir_setting(&dir_s, &default, "screenshot").expect_err("the same grant must not be usable twice");
    assert_eq!(err, "dir-not-picked");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn accept_dir_setting_rejects_a_grant_picked_for_a_different_purpose() {
    let dir = std::env::temp_dir().join(format!("ps-grant-wrong-purpose-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    let default = std::env::temp_dir().join(format!("ps-grant-wrong-purpose-default-{}", std::process::id()));
    grant_picked_dir(&dir, "sync");
    let err = accept_dir_setting(&dir_s, &default, "recording")
        .expect_err("a grant picked for one setting must not authorize a different one");
    assert_eq!(err, "dir-not-picked");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn accept_dir_setting_treats_empty_and_the_built_in_default_as_a_reset() {
    let default = std::env::temp_dir().join(format!("ps-grant-default-{}", std::process::id()));
    assert_eq!(accept_dir_setting("", &default, "screenshot").unwrap(), "", "empty must reset without needing a grant");
    assert_eq!(
        accept_dir_setting(&default.to_string_lossy(), &default, "screenshot").unwrap(),
        "",
        "the feature's own built-in default must reset without needing a grant"
    );
}

// ---------- security pass: recording-source ownership (convert/save/copy) ----------

#[test]
fn is_own_recording_source_accepts_a_rec_file_inside_an_allowed_folder() {
    let dir = std::env::temp_dir().join(format!("ps-recsrc-own-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-2026-01-01-00-00-00.mp4");
    fs::write(&src, b"video").unwrap();
    assert!(is_own_recording_source(std::slice::from_ref(&dir), &src));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn is_own_recording_source_refuses_a_foreign_file_outside_the_allowed_folders() {
    let dir = std::env::temp_dir().join(format!("ps-recsrc-in-{}", std::process::id()));
    let other = std::env::temp_dir().join(format!("ps-recsrc-out-{}", std::process::id()));
    fs::create_dir_all(&other).unwrap();
    let src = other.join("rec-elsewhere.mp4");
    fs::write(&src, b"video").unwrap();
    assert!(!is_own_recording_source(&[dir], &src), "the folder is not one this app writes recordings into");
    let _ = fs::remove_dir_all(&other);
}

#[test]
fn is_own_recording_source_refuses_a_file_without_the_rec_prefix_even_inside_an_allowed_folder() {
    let dir = std::env::temp_dir().join(format!("ps-recsrc-noprefix-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("not-a-recording.mp4");
    fs::write(&src, b"video").unwrap();
    assert!(!is_own_recording_source(std::slice::from_ref(&dir), &src), "a made-up name must not pass as proof");
    let _ = fs::remove_dir_all(&dir);
}

// ---------- security pass: bounded export-output set (save_recording_as) ----------

#[test]
fn is_known_export_output_accepts_only_a_path_export_video_actually_wrote() {
    let dir = std::env::temp_dir().join(format!("ps-exportout-known-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let exported = dir.join("clip-edit.mp4");
    fs::write(&exported, b"video").unwrap();
    let untouched = dir.join("clip-not-exported.mp4");
    fs::write(&untouched, b"video").unwrap();
    assert!(!is_known_export_output(&exported), "not remembered yet, must not pass");
    remember_export_output(&exported);
    assert!(is_known_export_output(&exported), "a file export_video actually produced must be recognized");
    assert!(!is_known_export_output(&untouched), "a neighbour file never exported must stay refused");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_moved_export_output_stays_known_at_its_new_place() {
    let dir = std::env::temp_dir().join(format!("ps-exportout-move-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let x = dir.join("clip-edit.mp4");
    let y = dir.join("moved.mp4");
    let z = dir.join("moved-again.mp4");
    let other = dir.join("other.mp4");
    fs::write(&x, b"video").unwrap();
    fs::write(&other, b"video").unwrap();
    remember_export_output(&x);
    move_tracked_export_output(&x, &y).unwrap();
    assert!(is_known_export_output(&y), "the moved file must still count as an export output");
    move_tracked_export_output(&y, &z).unwrap();
    assert!(is_known_export_output(&z), "a second move must keep the proof too");
    move_tracked_export_output(&other, &dir.join("other-moved.mp4")).unwrap();
    assert!(!is_known_export_output(&dir.join("other-moved.mp4")), "a file that was never an export stays unknown");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_output_to_remember_records_the_written_sibling_not_the_requested_dst() {
    let dir = std::env::temp_dir().join(format!("ps-exportout-sibling-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let requested = dir.join("clip-edit.mp4");
    fs::write(&requested, b"pre-existing file the export must not overwrite").unwrap();
    // The same reservation export_video_blocking makes once it finds the
    // requested name already taken, standing in for the file ffmpeg
    // actually wrote for this test.
    let sibling = reserve_unique_dest_path(&dir, "clip-edit", "mp4").unwrap();
    fs::write(&sibling, b"the file ffmpeg actually produced").unwrap();
    let result: Result<String, String> = Ok(sibling.to_string_lossy().to_string());
    if let Some(actual) = export_output_to_remember(&result) {
        remember_export_output(&actual);
    }
    assert!(!is_known_export_output(&requested), "the pre-existing requested path was never written by this export");
    assert!(is_known_export_output(&sibling), "the sibling ffmpeg actually wrote must be recognized");
    let _ = fs::remove_dir_all(&dir);
}

// ---------- export finalization: atomic name reservation ----------

#[test]
fn reserve_unique_dest_path_skips_to_the_next_free_name_and_claims_it() {
    let dir = std::env::temp_dir().join(format!("ps-reserve-basic-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let taken = dir.join("clip-edit.mp4");
    fs::write(&taken, b"already exported once").unwrap();
    let reserved = reserve_unique_dest_path(&dir, "clip-edit", "mp4").unwrap();
    assert_eq!(reserved, dir.join("clip-edit (2).mp4"));
    // The name is claimed, not just picked: it exists the instant the
    // call returns, before any caller has written or renamed into it.
    assert!(reserved.is_file());
    assert_eq!(fs::read(&taken).unwrap(), b"already exported once", "the original file must be untouched");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn reserve_unique_dest_path_never_replaces_a_file_that_wins_the_race() {
    let dir = std::env::temp_dir().join(format!("ps-reserve-race-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    // Stands in for another export (or another app) claiming the first
    // candidate name in the gap a plain exists() check
    // could not see: reserve_unique_dest_path must fail that name
    // outright and move on rather than overwrite it later.
    let rival = dir.join("clip-edit.mp4");
    fs::write(&rival, b"a different export wrote this first").unwrap();
    let reserved = reserve_unique_dest_path(&dir, "clip-edit", "mp4").unwrap();
    assert_ne!(reserved, rival, "the name already taken by another writer must never be reused");
    assert_eq!(fs::read(&rival).unwrap(), b"a different export wrote this first", "the rival's file must survive untouched");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn place_in_dir_keeps_both_files_and_leaves_no_placeholder_on_failure() {
    let root = std::env::temp_dir().join(format!("ps-place-in-dir-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let dest_dir = root.join("dest");
    let (a_dir, b_dir) = (root.join("a"), root.join("b"));
    fs::create_dir_all(&a_dir).unwrap();
    fs::create_dir_all(&b_dir).unwrap();
    let (a, b) = (a_dir.join("rec.mp4"), b_dir.join("rec.mp4"));
    fs::write(&a, b"first").unwrap();
    fs::write(&b, b"second").unwrap();
    let first = place_in_dir(&a, &dest_dir).unwrap();
    let second = place_in_dir(&b, &dest_dir).unwrap();
    assert_ne!(first, second);
    assert_eq!(fs::read(&first).unwrap(), b"first");
    assert_eq!(fs::read(&second).unwrap(), b"second");
    // A source that is gone: the move fails and no empty placeholder stays behind.
    let before = fs::read_dir(&dest_dir).unwrap().count();
    assert!(place_in_dir(&a_dir.join("missing.mp4"), &dest_dir).is_err());
    assert_eq!(fs::read_dir(&dest_dir).unwrap().count(), before);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn folder_import_skips_paths_that_leave_the_chosen_folder() {
    assert!(has_import_folder_ext(Path::new("sub/note.MD")));
    assert!(has_import_folder_ext(Path::new("note.txt")));
    assert!(!has_import_folder_ext(Path::new("config")));
    assert!(!has_import_folder_ext(Path::new("id_rsa.pub")));
    assert!(folder_rel_path_is_safe("note.md"));
    assert!(folder_rel_path_is_safe("sub/deeper/note.md"));
    assert!(!folder_rel_path_is_safe("../x.md"));
    assert!(!folder_rel_path_is_safe("sub/../../x.md"));
    assert!(!folder_rel_path_is_safe("C:\\Windows\\win.ini"));
    assert!(!folder_rel_path_is_safe("/etc/x.md"));
    assert!(!folder_rel_path_is_safe("\\\\server\\share\\x.md"));
    assert!(!folder_rel_path_is_safe(""));
}

#[test]
fn is_own_mic_sidecar_accepts_only_the_matching_videos_own_sidecar() {
    let video = PathBuf::from("C:/recs/rec-2026-01-01-00-00-00.mp4");
    let own_sidecar = PathBuf::from("C:/recs/rec-2026-01-01-00-00-00.mic.wav");
    assert!(is_own_mic_sidecar(&video, &own_sidecar));
}

#[test]
fn is_own_mic_sidecar_refuses_an_unrelated_sidecar_in_the_same_folder() {
    let video = PathBuf::from("C:/recs/rec-2026-01-01-00-00-00.mp4");
    let foreign_sidecar = PathBuf::from("C:/recs/rec-someone-elses-take.mic.wav");
    assert!(!is_own_mic_sidecar(&video, &foreign_sidecar), "a sidecar for a different recording must not pass");
}

#[test]
#[cfg(windows)]
fn copy_keys_sidecar_alongside_brings_the_sidecar_to_the_open_editor_copy() {
    let dir = std::env::temp_dir().join(format!("ps-openedit-keys-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-x.mp4");
    let dest = dir.join("rec-x-copy.mp4");
    fs::write(&src, b"video").unwrap();
    fs::write(sidecar_path(&src, KEYS_SIDECAR_EXT), br#"{"events":[],"gaps":[]}"#).unwrap();
    copy_keys_sidecar_alongside(&src, &dest);
    assert!(sidecar_path(&dest, KEYS_SIDECAR_EXT).is_file(), "open_file_in_editor's copy must bring the key-overlay sidecar with it");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[cfg(windows)]
fn copy_keys_sidecar_alongside_skips_a_sidecar_that_does_not_parse() {
    let dir = std::env::temp_dir().join(format!("ps-openedit-keys-bad-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-x.mp4");
    let dest = dir.join("rec-x-copy.mp4");
    fs::write(&src, b"video").unwrap();
    fs::write(sidecar_path(&src, KEYS_SIDECAR_EXT), b"not json at all").unwrap();
    copy_keys_sidecar_alongside(&src, &dest);
    assert!(!sidecar_path(&dest, KEYS_SIDECAR_EXT).exists(), "a sidecar that is not the expected shape must never be carried along");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[cfg(windows)]
fn copy_keys_sidecar_alongside_skips_a_sidecar_past_the_size_ceiling() {
    let dir = std::env::temp_dir().join(format!("ps-openedit-keys-huge-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-x.mp4");
    let dest = dir.join("rec-x-copy.mp4");
    fs::write(&src, b"video").unwrap();
    // Well past KEY_SIDECAR_MAX_COPY_BYTES, and never actually parsed --
    // the size check must turn it away before the file is even read.
    let oversized = vec![b' '; (KEY_SIDECAR_MAX_COPY_BYTES + 1) as usize];
    fs::write(sidecar_path(&src, KEYS_SIDECAR_EXT), &oversized).unwrap();
    copy_keys_sidecar_alongside(&src, &dest);
    assert!(!sidecar_path(&dest, KEYS_SIDECAR_EXT).exists(), "an oversized sidecar must never be carried along");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[cfg(windows)]
fn copy_keys_sidecar_alongside_is_a_no_op_when_the_source_never_captured_keys() {
    let dir = std::env::temp_dir().join(format!("ps-openedit-nokeys-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-x.mp4");
    let dest = dir.join("rec-x-copy.mp4");
    fs::write(&src, b"video").unwrap();
    copy_keys_sidecar_alongside(&src, &dest);
    assert!(!sidecar_path(&dest, KEYS_SIDECAR_EXT).exists(), "no sidecar to begin with must not conjure one for the copy");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn restore_discarded_recording_moves_the_file_back_to_its_original_folder() {
    let dir = std::env::temp_dir().join(format!("ps-restore-{}", std::process::id()));
    let trash = dir.join("verworfen");
    fs::create_dir_all(&trash).unwrap();
    fs::write(trash.join("rec-x.mp4"), b"video").unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    assert!(restore_discarded_recording_impl(&dir_s, "rec-x.mp4", &dir));
    assert!(dir.join("rec-x.mp4").is_file());
    assert!(!trash.join("rec-x.mp4").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn restore_discarded_recording_brings_the_key_overlay_sidecar_back_too() {
    let dir = std::env::temp_dir().join(format!("ps-restore-keys-{}", std::process::id()));
    let trash = dir.join("verworfen");
    fs::create_dir_all(&trash).unwrap();
    fs::write(trash.join("rec-x.mp4"), b"video").unwrap();
    fs::write(trash.join("rec-x.keys.json"), b"{}").unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    assert!(restore_discarded_recording_impl(&dir_s, "rec-x.mp4", &dir));
    assert!(dir.join("rec-x.keys.json").is_file(), "the key-overlay sidecar must travel with its recording");
    assert!(!trash.join("rec-x.keys.json").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn restore_discarded_recording_never_overwrites_a_file_already_at_the_old_spot() {
    let dir = std::env::temp_dir().join(format!("ps-restore-collide-{}", std::process::id()));
    let trash = dir.join("verworfen");
    fs::create_dir_all(&trash).unwrap();
    fs::write(trash.join("rec-x.mp4"), b"discarded copy").unwrap();
    fs::write(dir.join("rec-x.mp4"), b"a newer recording already took this name").unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    assert!(!restore_discarded_recording_impl(&dir_s, "rec-x.mp4", &dir));
    assert_eq!(fs::read(dir.join("rec-x.mp4")).unwrap(), b"a newer recording already took this name");
    assert!(trash.join("rec-x.mp4").is_file(), "the discarded copy must stay put, not vanish on a failed restore");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn delete_discarded_recording_now_removes_it_for_good() {
    let dir = std::env::temp_dir().join(format!("ps-discard-delete-now-{}", std::process::id()));
    let trash = dir.join("verworfen");
    fs::create_dir_all(&trash).unwrap();
    fs::write(trash.join("rec-x.mp4"), b"video").unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    assert!(delete_discarded_recording_now_impl(&dir_s, "rec-x.mp4", &dir));
    assert!(!trash.join("rec-x.mp4").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn delete_discarded_recording_now_removes_its_key_overlay_sidecar_too() {
    let dir = std::env::temp_dir().join(format!("ps-discard-delete-keys-{}", std::process::id()));
    let trash = dir.join("verworfen");
    fs::create_dir_all(&trash).unwrap();
    fs::write(trash.join("rec-x.mp4"), b"video").unwrap();
    fs::write(trash.join("rec-x.keys.json"), b"{}").unwrap();
    let dir_s = dir.to_string_lossy().to_string();
    assert!(delete_discarded_recording_now_impl(&dir_s, "rec-x.mp4", &dir));
    assert!(!trash.join("rec-x.keys.json").exists(), "a purge for good must not leave the key-overlay sidecar behind");
    let _ = fs::remove_dir_all(&dir);
}

// A cancel after finish-and-continue used to delete an earlier
// part's video and mic audio but leave its key-overlay sidecar behind --
// the one file a "fully discarded" take would otherwise still keep.
#[cfg(windows)]
#[test]
fn remove_recording_part_files_takes_the_key_overlay_sidecar_with_it() {
    let dir = std::env::temp_dir().join(format!("ps-cancel-part-keys-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let video = dir.join("rec-part-2.mp4");
    let mic = dir.join("rec-part-2.mic.wav");
    let keys = dir.join("rec-part-2.keys.json");
    fs::write(&video, b"video").unwrap();
    fs::write(&mic, b"mic").unwrap();
    fs::write(&keys, b"{}").unwrap();
    let part = RecordingPartInfo { path: video.to_string_lossy().to_string(), mic_wav_path: Some(mic.to_string_lossy().to_string()) };
    remove_recording_part_files(&part);
    assert!(!video.exists());
    assert!(!mic.exists());
    assert!(!keys.exists(), "cancelling a take must not leave an earlier part's recorded keys behind");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn is_bare_filename_rejects_a_path_traversal_attempt() {
    assert!(is_bare_filename("rec-x.mp4"));
    assert!(!is_bare_filename("../rec-x.mp4"));
    assert!(!is_bare_filename("sub/rec-x.mp4"));
    assert!(!is_bare_filename(""));
}

#[test]
fn discarded_stale_in_removes_only_files_past_the_configured_age() {
    let dir = std::env::temp_dir().join(format!("ps-discarded-stale-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let old = dir.join("rec-old.mp4");
    let recent = dir.join("rec-recent.mp4");
    fs::write(&old, b"old").unwrap();
    fs::write(&recent, b"recent").unwrap();
    backdate(&old, Duration::from_secs(25 * 3600));
    let (count, bytes) = discarded_stale_in(&dir, Duration::from_secs(24 * 3600));
    assert_eq!(count, 1, "only the file past the retention window is removed");
    assert_eq!(bytes, 3);
    assert!(!old.exists());
    assert!(recent.is_file(), "a recent discard must survive this sweep");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn discarded_stale_in_removes_the_key_overlay_sidecar_alongside_an_old_discard() {
    let dir = std::env::temp_dir().join(format!("ps-discarded-stale-keys-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let old = dir.join("rec-old.mp4");
    let old_keys = dir.join("rec-old.keys.json");
    fs::write(&old, b"old").unwrap();
    fs::write(&old_keys, b"{}").unwrap();
    backdate(&old, Duration::from_secs(25 * 3600));
    let (count, _bytes) = discarded_stale_in(&dir, Duration::from_secs(24 * 3600));
    assert_eq!(count, 1);
    assert!(!old_keys.exists(), "the key-overlay sidecar of a purged discard must not be left orphaned");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn video_folder_cleanup_preview_and_cleanup_agree_and_only_touch_stale_video_files() {
    let dir = std::env::temp_dir().join(format!("ps-video-folder-cleanup-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let stale = dir.join("rec-old-recording.mp4");
    let fresh = dir.join("rec-new-recording.mp4");
    let unrelated = dir.join("readme.txt");
    fs::write(&stale, b"old").unwrap();
    fs::write(&fresh, b"new").unwrap();
    fs::write(&unrelated, b"not a video").unwrap();
    backdate(&stale, Duration::from_secs(40 * 24 * 3600));
    backdate(&unrelated, Duration::from_secs(40 * 24 * 3600));

    let dir_s = dir.to_string_lossy().to_string();
    let preview = video_folder_cleanup_preview_impl(&dir_s, 30, &dir);
    assert_eq!(preview.len(), 1, "only the stale video file is older than 30 days");
    assert_eq!(preview[0].name, "rec-old-recording.mp4");
    assert_eq!(preview[0].size, 3);

    let deleted = video_folder_cleanup_impl(&dir_s, 30, &[preview[0].name.clone()], &dir);
    assert_eq!(deleted, 1, "cleanup must delete exactly what the preview listed");
    assert!(!stale.exists(), "the stale video must be gone");
    assert!(fresh.exists(), "a fresh video must survive");
    assert!(unrelated.exists(), "a non-video file must never be deleted, however old");
    let _ = fs::remove_dir_all(&dir);
}

// Finding 3 (security pass 348): a stale file with a matching extension but
// no "rec-" stem (some other app's video the user's chosen folder already
// had in it) must survive the cleanup that only ever meant to sweep this
// app's own recordings.
#[test]
fn video_folder_cleanup_never_touches_a_stale_file_without_the_rec_prefix() {
    let dir = std::env::temp_dir().join(format!("ps-video-folder-cleanup-noprefix-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let foreign = dir.join("someone-elses-video.mp4");
    fs::write(&foreign, b"not ours").unwrap();
    backdate(&foreign, Duration::from_secs(40 * 24 * 3600));

    let dir_s = dir.to_string_lossy().to_string();
    assert!(video_folder_cleanup_preview_impl(&dir_s, 30, &dir).is_empty(), "a non-rec- file must never be previewed for cleanup");
    assert_eq!(video_folder_cleanup_impl(&dir_s, 30, &["someone-elses-video.mp4".to_string()], &dir), 0, "a non-rec- file must never be deleted by cleanup");
    assert!(foreign.is_file(), "the foreign video must survive untouched");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn video_folder_cleanup_deletes_only_the_listed_names_that_are_still_stale() {
    let dir = std::env::temp_dir().join(format!("ps-video-folder-cleanup-listed-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let listed = dir.join("rec-listed.mp4");
    let unlisted = dir.join("rec-crossed-later.mp4");
    let fresh = dir.join("rec-fresh.mp4");
    for f in [&listed, &unlisted, &fresh] {
        fs::write(f, b"x").unwrap();
    }
    backdate(&listed, Duration::from_secs(40 * 24 * 3600));
    backdate(&unlisted, Duration::from_secs(40 * 24 * 3600));
    let dir_s = dir.to_string_lossy().to_string();
    let names = vec![
        "rec-listed.mp4".to_string(),
        "rec-fresh.mp4".to_string(),
        "..\\rec-outside.mp4".to_string(),
        "sub/rec-x.mp4".to_string(),
    ];
    assert_eq!(video_folder_cleanup_impl(&dir_s, 30, &names, &dir), 1);
    assert!(!listed.exists(), "the listed stale file goes");
    assert!(unlisted.exists(), "a stale file that was not on the confirmed list stays");
    assert!(fresh.exists(), "a listed file that is no longer stale stays");
    assert_eq!(video_folder_cleanup_impl(&dir_s, 30, &[], &dir), 0, "an empty list deletes nothing");
    assert!(unlisted.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn video_folder_cleanup_refuses_a_dir_that_is_not_the_configured_recording_folder() {
    let configured = std::env::temp_dir().join(format!("ps-video-folder-configured-{}", std::process::id()));
    let foreign = std::env::temp_dir().join(format!("ps-video-folder-foreign-{}", std::process::id()));
    fs::create_dir_all(&configured).unwrap();
    fs::create_dir_all(&foreign).unwrap();
    let victim = foreign.join("not-a-recording.mp4");
    fs::write(&victim, b"old").unwrap();
    backdate(&victim, Duration::from_secs(40 * 24 * 3600));
    let foreign_s = foreign.to_string_lossy().to_string();

    assert!(video_folder_cleanup_preview_impl(&foreign_s, 30, &configured).is_empty(), "a foreign folder must never be previewed");
    assert_eq!(video_folder_cleanup_impl(&foreign_s, 30, &["not-a-recording.mp4".to_string()], &configured), 0, "a foreign folder must never be cleaned");
    assert!(victim.is_file(), "the file in the foreign folder must survive untouched");
    assert_eq!(video_folder_stats_impl(&foreign_s, &configured), (0, 0), "stats for a foreign folder must read as empty");

    let _ = fs::remove_dir_all(&configured);
    let _ = fs::remove_dir_all(&foreign);
}

#[test]
fn discarded_recording_commands_refuse_a_dir_that_is_not_the_configured_recording_folder() {
    let configured = std::env::temp_dir().join(format!("ps-discard-configured-{}", std::process::id()));
    let foreign = std::env::temp_dir().join(format!("ps-discard-foreign-{}", std::process::id()));
    let foreign_trash = foreign.join("verworfen");
    fs::create_dir_all(&configured).unwrap();
    fs::create_dir_all(&foreign_trash).unwrap();
    fs::write(foreign_trash.join("rec-x.mp4"), b"video").unwrap();
    let foreign_s = foreign.to_string_lossy().to_string();

    assert!(list_discarded_recordings_impl(&foreign_s, &configured).is_empty(), "a foreign folder's trash must never be listed");
    assert!(!restore_discarded_recording_impl(&foreign_s, "rec-x.mp4", &configured), "a foreign folder must never be restored from");
    assert!(!delete_discarded_recording_now_impl(&foreign_s, "rec-x.mp4", &configured), "a foreign folder must never be deleted from");
    assert!(foreign_trash.join("rec-x.mp4").is_file(), "the file in the foreign folder must survive untouched");

    let _ = fs::remove_dir_all(&configured);
    let _ = fs::remove_dir_all(&foreign);
}

// Reusing a zoomed-in filmstrip's cache for a zoomed-out request served
// thumbnails spaced for the WRONG count, silently covering only part of the
// clip: count must be part of the cache key, same as path and mtime.
#[test]
fn filmstrip_cache_dir_keys_on_requested_count_too() {
    let dir = std::env::temp_dir();
    let src = dir.join(format!("ps-filmstrip-key-test-{}.mp4", std::process::id()));
    fs::write(&src, b"x").unwrap();
    let a = filmstrip_cache_dir(&src, 12).unwrap();
    let b = filmstrip_cache_dir(&src, 40).unwrap();
    assert_ne!(a, b, "different requested counts must not collide on the same cache dir");
    assert_eq!(filmstrip_cache_dir(&src, 12).unwrap(), a, "the same (path, mtime, count) must be stable");
    let _ = fs::remove_file(&src);
}

// filmstrip_cache_dir's own comment used to claim an existing sweep
// already covered this folder: it never actually recursed into it, so a
// cache entry for a since-deleted/renamed/re-encoded recording sat forever.
#[test]
fn sweep_stale_temp_files_also_reclaims_stale_filmstrip_cache_dirs() {
    let dir = sweep_test_root("filmstrip");
    let pid = std::process::id();
    let thumb_root = dir.join(brand::TEMP_DIR).join("video-thumbs");
    let stale_entry = thumb_root.join(format!("stale-{pid}"));
    let fresh_entry = thumb_root.join(format!("fresh-{pid}"));
    fs::create_dir_all(&stale_entry).unwrap();
    fs::write(stale_entry.join("thumb-0000.jpg"), b"jpeg").unwrap();
    fs::create_dir_all(&fresh_entry).unwrap();
    fs::write(fresh_entry.join("thumb-0000.jpg"), b"jpeg").unwrap();
    backdate(&stale_entry, STALE_TEMP_FILE_AGE + Duration::from_secs(60));

    sweep_stale_temp_files_in(&dir);

    assert!(!stale_entry.exists(), "an old, unrevisited filmstrip cache dir must be swept");
    assert!(fresh_entry.exists(), "a recently-touched filmstrip cache dir must survive");
    let _ = fs::remove_dir_all(&dir);
}

// A recording nobody decided on (attach/save/discard) yet, old enough that
// it can only be a crash/abnormal-exit leftover, is swept; one still fresh
// (the post-capture menu could genuinely still be open) and one a prompt
// already references (storeFiles off, so file_path points straight into
// this folder) both survive.
#[test]
fn sweep_stale_recordings_removes_only_old_unreferenced_files() {
    let dir = std::env::temp_dir().join(format!("ps-test-rec-sweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let abandoned = dir.join("2026-09-04_09-00-00.mp4");
    let fresh = dir.join("2026-09-04_09-30-00.mp4");
    let referenced = dir.join("2026-09-04_08-00-00.mp4");
    fs::write(&abandoned, b"video").unwrap();
    fs::write(&fresh, b"video").unwrap();
    fs::write(&referenced, b"video").unwrap();
    backdate(&abandoned, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&referenced, STALE_TEMP_FILE_AGE + Duration::from_secs(60));

    let mut wanted = std::collections::HashSet::new();
    wanted.insert(referenced.to_string_lossy().replace('/', "\\").to_lowercase());
    sweep_stale_recordings_in(&dir, &wanted, &[], true, Duration::from_secs(300), None);

    assert!(!abandoned.exists(), "an old, unreferenced recording must be swept");
    assert!(fresh.exists(), "a fresh recording (menu possibly still open) must survive");
    assert!(referenced.exists(), "a recording a prompt still references must survive however old");

    let _ = fs::remove_dir_all(&dir);
}

// recordingRetentionDays at 0 (None, keep forever) lets a kept but
// unreferenced recording age well past STALE_TEMP_FILE_AGE untouched --
// once a caller passes a retention window, an unreferenced file past it is
// swept even though it is nowhere near that fixed constant, and a
// REFERENCED file survives regardless of how far past the window it is.
#[test]
fn sweep_stale_recordings_in_retention_age_only_removes_unreferenced_files_past_it() {
    let dir = std::env::temp_dir().join(format!("ps-test-rec-retention-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let old_unreferenced = dir.join("2026-09-04_07-00-00.mp4");
    let old_referenced = dir.join("2026-09-04_07-30-00.mp4");
    let young_unreferenced = dir.join("2026-09-04_07-45-00.mp4");
    fs::write(&old_unreferenced, b"video").unwrap();
    fs::write(&old_referenced, b"video").unwrap();
    fs::write(&young_unreferenced, b"video").unwrap();
    // Both ages stay well under STALE_TEMP_FILE_AGE (one hour), and the
    // broken-file sweep is off below: none of these three plain (not a
    // real mp4) files can be swept for any reason other than retention,
    // which is exactly what this test means to isolate.
    backdate(&old_unreferenced, Duration::from_secs(45 * 60));
    backdate(&old_referenced, Duration::from_secs(45 * 60));
    backdate(&young_unreferenced, Duration::from_secs(10 * 60));

    let mut referenced = std::collections::HashSet::new();
    referenced.insert(old_referenced.to_string_lossy().replace('/', "\\").to_lowercase());

    // Off (None): nothing here is anywhere near STALE_TEMP_FILE_AGE, and
    // broken_sweep_on is false, so all three survive.
    let (count_off, _) = sweep_stale_recordings_in(&dir, &referenced, &[], false, Duration::from_secs(600), None);
    assert_eq!(count_off, 0, "retention off must not remove anything by age alone");
    assert!(old_unreferenced.exists());

    // On, at thirty minutes: only the old, unreferenced file is past it.
    let (count_on, bytes_on) =
        sweep_stale_recordings_in(&dir, &referenced, &[], false, Duration::from_secs(600), Some(Duration::from_secs(30 * 60)));
    assert_eq!(count_on, 1);
    assert!(bytes_on > 0);
    assert!(!old_unreferenced.exists(), "an old, unreferenced recording past the retention window must be swept");
    assert!(old_referenced.exists(), "a recording a prompt still references must survive however old");
    assert!(young_unreferenced.exists(), "a recording under the retention window must survive");

    let _ = fs::remove_dir_all(&dir);
}

// The box parser against a valid file, a truncated (crash) one, and a
// non-mp4: exactly the three cases that matter for detection.
#[test]
fn mp4_has_moov_detects_present_and_missing_atom() {
    let dir = std::env::temp_dir().join(format!("ps-test-moov-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let mut valid = Vec::new();
    valid.extend_from_slice(&16u32.to_be_bytes());
    valid.extend_from_slice(b"ftyp");
    valid.extend_from_slice(&[0u8; 8]);
    valid.extend_from_slice(&8u32.to_be_bytes());
    valid.extend_from_slice(b"moov");
    let valid_path = dir.join("valid.mp4");
    fs::write(&valid_path, &valid).unwrap();
    assert!(mp4_has_moov(&valid_path), "a moov box present at the top level must be found");

    let mut truncated = Vec::new();
    truncated.extend_from_slice(&16u32.to_be_bytes());
    truncated.extend_from_slice(b"ftyp");
    truncated.extend_from_slice(&[0u8; 8]);
    truncated.extend_from_slice(&8u32.to_be_bytes());
    truncated.extend_from_slice(b"mdat");
    let truncated_path = dir.join("truncated.mp4");
    fs::write(&truncated_path, &truncated).unwrap();
    assert!(!mp4_has_moov(&truncated_path), "a crash leftover with no moov must be reported as such");

    let garbage_path = dir.join("not-a-video.txt");
    fs::write(&garbage_path, b"just plain text, no box structure at all here").unwrap();
    assert!(!mp4_has_moov(&garbage_path), "non-box-shaped bytes must not be mistaken for a moov box");

    // The header claims a large moov box, but the file ends right
    // after the header itself: a crash mid-write, not a healthy file.
    let mut cut_short = Vec::new();
    cut_short.extend_from_slice(&16u32.to_be_bytes());
    cut_short.extend_from_slice(b"ftyp");
    cut_short.extend_from_slice(&[0u8; 8]);
    cut_short.extend_from_slice(&5000u32.to_be_bytes());
    cut_short.extend_from_slice(b"moov");
    let cut_short_path = dir.join("cut-short.mp4");
    fs::write(&cut_short_path, &cut_short).unwrap();
    assert!(!mp4_has_moov(&cut_short_path), "a moov header whose declared size overruns the file must be reported as broken");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn recording_is_broken_only_flags_moov_less_mp4() {
    let dir = std::env::temp_dir().join(format!("ps-test-broken-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let mut valid = Vec::new();
    valid.extend_from_slice(&8u32.to_be_bytes());
    valid.extend_from_slice(b"moov");
    let mp4_ok = dir.join("ok.mp4");
    fs::write(&mp4_ok, &valid).unwrap();
    assert!(!recording_is_broken(&mp4_ok), "an mp4 with a moov box is not broken");

    let mp4_broken = dir.join("broken.mp4");
    fs::write(&mp4_broken, b"not a real box structure").unwrap();
    assert!(recording_is_broken(&mp4_broken), "an mp4 with no moov box is broken");

    let other = dir.join("clip.webm");
    fs::write(&other, b"not a real box structure").unwrap();
    assert!(!recording_is_broken(&other), "only mp4 is judged by the moov check");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rec_wav_writer_roundtrips_a_valid_header_and_sample_data() {
    let dir = std::env::temp_dir().join(format!("ps-test-wav-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mic.wav");

    let mut w = RecWavWriter::create(&path, 48_000, 2).expect("create");
    let samples: Vec<i16> = vec![1, -1, 100, -100];
    w.write_samples(&samples).expect("write");
    w.write_samples(&[42]).expect("write more");
    w.finish().expect("finish");

    let bytes = fs::read(&path).unwrap();
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    assert_eq!(&bytes[12..16], b"JUNK", "room for the 64-bit size table");
    assert_eq!(u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]), 28);
    assert_eq!(&bytes[48..52], b"fmt ");
    assert_eq!(u16::from_le_bytes([bytes[56], bytes[57]]), 1, "PCM format tag");
    assert_eq!(u16::from_le_bytes([bytes[58], bytes[59]]), 2, "channel count");
    assert_eq!(u32::from_le_bytes([bytes[60], bytes[61], bytes[62], bytes[63]]), 48_000, "sample rate");
    assert_eq!(&bytes[72..76], b"data");
    let data_size = u32::from_le_bytes([bytes[76], bytes[77], bytes[78], bytes[79]]);
    assert_eq!(data_size, (samples.len() as u32 + 1) * 2, "data chunk size must cover every written sample");
    let riff_size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    assert_eq!(riff_size, 72 + data_size, "RIFF size is the fixed 72-byte remainder of the header plus the data chunk");
    assert_eq!(bytes.len() as u32, 80 + data_size, "file length must match header + data exactly");

    let _ = fs::remove_dir_all(&dir);
}

// Past 4 GiB the header turns into RF64: the ds64 table carries the 64-bit sizes and
// the 32-bit size fields hold the all-ones marker.
#[test]
fn rec_wav_writer_writes_an_rf64_header_past_four_gib() {
    let dir = std::env::temp_dir().join(format!("ps-test-wav-rf64-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mic.wav");

    let mut w = RecWavWriter::create(&path, 48_000, 2).expect("create");
    w.write_samples(&[1, 2]).expect("write");
    let data: u64 = u32::MAX as u64 + 1_000;
    w.data_bytes = data;
    w.finish().expect("finish");

    let bytes = fs::read(&path).unwrap();
    let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    assert_eq!(&bytes[0..4], b"RF64");
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), u32::MAX);
    assert_eq!(&bytes[8..12], b"WAVE");
    assert_eq!(&bytes[12..16], b"ds64");
    assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 28);
    assert_eq!(u64_at(20), 72 + data, "riff size");
    assert_eq!(u64_at(28), data, "data size");
    assert_eq!(u64_at(36), data / 4, "sample count (4 bytes per stereo frame)");
    assert_eq!(u32::from_le_bytes(bytes[44..48].try_into().unwrap()), 0, "no table entries");
    assert_eq!(&bytes[48..52], b"fmt ");
    assert_eq!(&bytes[72..76], b"data");
    assert_eq!(u32::from_le_bytes(bytes[76..80].try_into().unwrap()), u32::MAX);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn mic_sidecar_video_path_only_matches_the_mic_wav_suffix() {
    assert_eq!(
        mic_sidecar_video_path(Path::new("C:/rec/2026-09-04_10-00-00.mic.wav")),
        Some(PathBuf::from("C:/rec/2026-09-04_10-00-00.mp4"))
    );
    assert_eq!(mic_sidecar_video_path(Path::new("C:/rec/2026-09-04_10-00-00.mp4")), None);
    assert_eq!(mic_sidecar_video_path(Path::new("C:/rec/other.wav")), None);
}

#[test]
fn keys_sidecar_video_path_only_matches_the_keys_json_suffix() {
    assert_eq!(
        keys_sidecar_video_path(Path::new("C:/rec/2026-09-04_10-00-00.keys.json")),
        Some(PathBuf::from("C:/rec/2026-09-04_10-00-00.mp4"))
    );
    assert_eq!(keys_sidecar_video_path(Path::new("C:/rec/2026-09-04_10-00-00.mp4")), None);
    assert_eq!(keys_sidecar_video_path(Path::new("C:/rec/other.json")), None);
}

// A key-overlay sidecar must never be swept away while its own recording
// is still referenced: the same rule sweep_stale_recordings_in already
// gives the mic sidecar, tested below.
#[test]
fn sweep_stale_recordings_in_keeps_keys_sidecar_of_a_referenced_recording() {
    let dir = std::env::temp_dir().join(format!("ps-test-keys-sidecar-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let video = dir.join("2026-09-04_12-00-00.mp4");
    let sidecar = dir.join("2026-09-04_12-00-00.keys.json");
    fs::write(&video, b"not a real box structure").unwrap();
    fs::write(&sidecar, b"{}").unwrap();
    backdate(&video, Duration::from_secs(3600));
    backdate(&sidecar, Duration::from_secs(3600));

    let mut referenced = std::collections::HashSet::new();
    referenced.insert(video.to_string_lossy().replace('/', "\\").to_lowercase());
    let (count, _bytes) = sweep_stale_recordings_in(&dir, &referenced, &[], true, Duration::from_secs(60), None);

    assert_eq!(count, 0, "neither the referenced mp4 nor its key-overlay sidecar may be swept");
    assert!(video.exists());
    assert!(sidecar.exists(), "the key-overlay sidecar rides along with its referenced recording");

    let _ = fs::remove_dir_all(&dir);
}

// A mic sidecar must never be swept away while its own recording is still
// referenced (e.g. already attached to a prompt) even though the sidecar
// itself is never directly referenced by anything.
#[test]
fn sweep_stale_recordings_in_keeps_mic_sidecar_of_a_referenced_recording() {
    let dir = std::env::temp_dir().join(format!("ps-test-mic-sidecar-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let video = dir.join("2026-09-04_12-00-00.mp4");
    let sidecar = dir.join("2026-09-04_12-00-00.mic.wav");
    fs::write(&video, b"not a real box structure").unwrap();
    fs::write(&sidecar, b"RIFF....WAVE").unwrap();
    backdate(&video, Duration::from_secs(3600));
    backdate(&sidecar, Duration::from_secs(3600));

    let mut referenced = std::collections::HashSet::new();
    referenced.insert(video.to_string_lossy().replace('/', "\\").to_lowercase());
    let (count, _bytes) = sweep_stale_recordings_in(&dir, &referenced, &[], true, Duration::from_secs(60), None);

    assert_eq!(count, 0, "neither the referenced mp4 nor its sidecar may be swept");
    assert!(video.exists());
    assert!(sidecar.exists(), "the sidecar rides along with its referenced recording");

    let _ = fs::remove_dir_all(&dir);
}

// A recording open in the video editor (or the mic sidecar of any
// protected recording, muxed in or not) must survive the sweep exactly
// like the actively-recording session's own output already did: none of
// these four carry a db reference and all are well past every threshold.
#[test]
fn sweep_stale_recordings_in_never_touches_editor_source_or_any_mic_sidecar() {
    let dir = std::env::temp_dir().join(format!("ps-test-editor-sweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let active = dir.join("2026-09-04_09-00-00.mp4");
    let active_sidecar = dir.join("2026-09-04_09-00-00.mic.wav");
    let editor_src = dir.join("2026-09-04_08-00-00.mp4");
    let editor_sidecar = dir.join("2026-09-04_08-00-00.mic.wav");
    for p in [&active, &active_sidecar, &editor_src, &editor_sidecar] {
        fs::write(p, b"not a real box structure").unwrap();
        backdate(p, Duration::from_secs(3600));
    }

    let empty = std::collections::HashSet::new();
    let protected = [active.clone(), active_sidecar.clone(), editor_src.clone(), editor_sidecar.clone()];
    let (count, bytes) = sweep_stale_recordings_in(&dir, &empty, &protected, true, Duration::from_secs(60), None);

    assert_eq!(count, 0);
    assert_eq!(bytes, 0);
    for p in [&active, &active_sidecar, &editor_src, &editor_sidecar] {
        assert!(p.exists(), "protected path must survive the sweep: {p:?}");
    }

    let _ = fs::remove_dir_all(&dir);
}
