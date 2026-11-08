use super::*;

#[test]
fn the_stats_and_theme_file_dialogs_wait_on_a_blocking_thread() {
    let stats = include_str!("../stats.rs");
    let theme = include_str!("../theme.rs");
    for (src, name) in [(stats, "export_stats"), (theme, "export_custom_theme"), (theme, "import_custom_theme")] {
        let body = diff_writes::fn_text(src, name);
        assert!(body.contains("spawn_blocking"), "{name} must not hold an async worker for the dialog");
        assert!(body.contains("file_dialog("), "{name} still opens the dialog");
    }
}

#[test]
fn duplicate_report_view_names_come_from_members_not_old_layouts() {
    let mut a = test_view("a");
    a.members = vec!["p1".into(), "p2".into(), "p1".into()];
    // p3 only sits in an old saved layout of a grid size nobody uses now.
    a.layouts.entry("2x2".into()).or_default().insert("p3".into(), [0, 0]);
    let mut b = test_view("b");
    b.members = vec!["p2".into()];
    let views = vec![a, b];
    let names = view_names_by_prompt(&views);
    assert_eq!(names.get("p1").cloned(), Some(vec!["A".to_string()]));
    assert_eq!(names.get("p2").cloned(), Some(vec!["A".to_string(), "B".to_string()]));
    assert!(!names.contains_key("p3"));
}

#[test]
fn usage_summary_counts_copies_favorites_and_archive() {
    let p = |id: &str, extra: &str| -> Prompt {
        serde_json::from_str(&format!(r#"{{"id":"{id}","name":"N","text":"T"{extra}}}"#)).unwrap()
    };
    let prompts = vec![
        p("a", r#","favorite":true"#),
        p("b", r#","file_path":"C:/x/clip.mp4""#),
        p("c", r#","file_path":"C:/x/doc.pdf","tags":["Archive"]"#),
        p("d", ""),
    ];
    let mut usage = std::collections::HashMap::new();
    usage.insert("a".to_string(), 5u32);
    usage.insert("b".to_string(), 2u32);
    usage.insert("gone".to_string(), 9u32); // a deleted prompt's counter is not part of the library
    let s = usage_summary(&prompts, &usage, Some("archive"));
    assert_eq!(s.copies_total, 7);
    assert_eq!(s.used_prompts, 2);
    assert_eq!(s.favorites, 1);
    assert_eq!(s.archived, 1);
}

#[test]
fn used_tools_lists_only_used_tools_most_used_first() {
    let mut usage = std::collections::HashMap::new();
    let mut last = std::collections::HashMap::new();
    usage.insert(tool_usage_key("ocr"), 3u32);
    usage.insert(tool_usage_key("screenshot"), 7u32);
    usage.insert(tool_usage_key("chain"), 3u32);
    usage.insert(tool_usage_key("dictation"), 0u32);
    usage.insert("tool:unknown".to_string(), 50u32); // not a counted tool
    usage.insert("p1".to_string(), 99u32); // a prompt, not a tool
    last.insert(tool_usage_key("screenshot"), 1_700_000_000u64);
    let t = used_tools(&usage, &last);
    let names: Vec<&str> = t.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names, ["screenshot", "chain", "ocr"]);
    assert_eq!(t[0].count, 7);
    assert_eq!(t[0].last_used, 1_700_000_000);
    assert_eq!(t[1].last_used, 0);
}

#[test]
fn live_daily_counts_skip_deleted_prompts_and_old_days() {
    let live: std::collections::HashSet<&str> = ["a", "b"].into_iter().collect();
    let rows = vec![
        (100u64, "a".to_string(), 2usize),
        (100, "gone".to_string(), 9), // a deleted prompt's copies do not count
        (102, "b".to_string(), 1),
        (102, "a".to_string(), 3),
        (99, "a".to_string(), 5), // before the window
        (110, "a".to_string(), 7), // after the window
    ];
    let d = live_daily_counts(100, 3, &rows, &live);
    let counts: Vec<usize> = d.iter().map(|x| x.count).collect();
    assert_eq!(counts, [2, 0, 4]);
    assert_eq!(d[0].ts, 100 * 86400);
    assert_eq!(d[2].ts, 102 * 86400);
}
