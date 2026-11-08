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
