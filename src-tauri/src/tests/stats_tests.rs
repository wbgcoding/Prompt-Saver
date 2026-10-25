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
