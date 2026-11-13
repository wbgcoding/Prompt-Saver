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

// ---- Statistics periods ----
fn schema_conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    assert!(db_conn_init_schema(&conn));
    conn
}

fn stat_prompt(id: &str, name: &str) -> Prompt {
    serde_json::from_str(&format!(r#"{{"id":"{id}","name":"{name}","text":"T"}}"#)).unwrap()
}

fn day_of(y: i64, m: i64, d: i64) -> u64 {
    crate::backups::days_from_civil(y, m, d) as u64
}

fn usage_rows(conn: &Connection) -> Vec<(u64, String, usize)> {
    let mut rows = db_usage_daily_rows(conn, 0);
    rows.sort();
    rows
}

#[test]
fn usage_daily_is_part_of_the_schema_and_an_old_database_gains_it() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL); CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);").unwrap();
    assert!(db_conn_init_schema(&conn));
    assert!(!db_usage_daily_has_rows(&conn));
    db_usage_daily_add(&conn, 5, "a").unwrap();
    assert!(db_usage_daily_has_rows(&conn));
    // Running the schema again over a filled table changes nothing.
    assert!(db_conn_init_schema(&conn));
    assert_eq!(usage_rows(&conn), vec![(5, "a".to_string(), 1)]);
}

#[test]
fn usage_daily_upsert_counts_per_day_and_prompt() {
    let conn = schema_conn();
    db_usage_daily_add(&conn, 100, "a").unwrap();
    db_usage_daily_add(&conn, 100, "a").unwrap();
    db_usage_daily_add(&conn, 100, "b").unwrap();
    db_usage_daily_add(&conn, 101, "a").unwrap();
    assert_eq!(
        usage_rows(&conn),
        vec![(100, "a".to_string(), 2), (100, "b".to_string(), 1), (101, "a".to_string(), 1)]
    );
    assert_eq!(db_usage_daily_min_day(&conn), Some(100));
}

#[test]
fn usage_daily_prune_keeps_exactly_400_days() {
    let conn = schema_conn();
    let today = 20_000u64;
    for day in [today - 401, today - 400, today - 1, today] {
        db_usage_daily_add(&conn, day, "a").unwrap();
    }
    db_usage_daily_prune(&conn, today).unwrap();
    let days: Vec<u64> = usage_rows(&conn).iter().map(|r| r.0).collect();
    assert_eq!(days, vec![today - 400, today - 1, today]);
}

#[test]
fn usage_daily_record_counts_the_local_day_of_the_moment() {
    let conn = schema_conn();
    // Half past eleven at night UTC, two hours ahead: already the next local day.
    let now = day_of(2026, 3, 10) * 86400 + 23 * 3600 + 1800;
    db_usage_daily_record(&conn, "a", now, 2 * 3600).unwrap();
    db_usage_daily_record(&conn, "a", now, 2 * 3600).unwrap();
    assert_eq!(usage_rows(&conn), vec![(day_of(2026, 3, 11), "a".to_string(), 2)]);
}

#[test]
fn usage_daily_seeds_once_from_the_copy_history() {
    let conn = schema_conn();
    let d = day_of(2026, 2, 1) * 86400;
    for (ts, id) in [(d + 100, "a"), (d + 200, "a"), (d + 86400 + 5, "b"), (0, "a")] {
        conn.execute("INSERT INTO copy_log(ts, id) VALUES(?1, ?2)", params![ts as i64, id]).unwrap();
    }
    db_seed_usage_daily_once(&conn, 0).unwrap();
    assert_eq!(
        usage_rows(&conn),
        vec![(day_of(2026, 2, 1), "a".to_string(), 2), (day_of(2026, 2, 2), "b".to_string(), 1)],
        "a row without a timestamp has no day"
    );
    // History rows that arrive later are not folded in a second time.
    conn.execute("INSERT INTO copy_log(ts, id) VALUES(?1, 'a')", params![(d + 300) as i64]).unwrap();
    db_seed_usage_daily_once(&conn, 0).unwrap();
    assert_eq!(usage_rows(&conn).iter().map(|r| r.2).sum::<usize>(), 3);
    // Even after the per-day rows are cleared the marker keeps it from seeding again.
    conn.execute("DELETE FROM usage_daily", []).unwrap();
    db_seed_usage_daily_once(&conn, 0).unwrap();
    assert!(!db_usage_daily_has_rows(&conn));
}

#[test]
fn usage_daily_seed_leaves_a_filled_table_alone() {
    let conn = schema_conn();
    conn.execute("INSERT INTO copy_log(ts, id) VALUES(86400 * 50, 'old')", []).unwrap();
    db_usage_daily_add(&conn, 70, "a").unwrap();
    db_seed_usage_daily_once(&conn, 0).unwrap();
    assert_eq!(usage_rows(&conn), vec![(70, "a".to_string(), 1)]);
}

#[test]
fn clearing_the_history_removes_the_per_day_rows_and_undo_adds_them_back() {
    let conn = schema_conn();
    conn.execute("INSERT INTO copy_log(ts, id) VALUES(5, 'a')", []).unwrap();
    db_usage_daily_add(&conn, 10, "a").unwrap();
    db_usage_daily_add(&conn, 10, "a").unwrap();
    let snapshot = usage_rows(&conn);
    db_clear_copy_tables(&conn).unwrap();
    assert!(!db_usage_daily_has_rows(&conn));
    assert!(!copy_log_has_rows(&conn));
    // A copy counted between the clear and the Undo stays and the snapshot adds to it.
    db_usage_daily_add(&conn, 10, "a").unwrap();
    db_restore_usage_daily(&conn, &snapshot).unwrap();
    assert_eq!(usage_rows(&conn), vec![(10, "a".to_string(), 3)]);
}

#[test]
fn period_start_covers_the_asked_days_and_52_weeks() {
    let today = day_of(2026, 1, 7); // a Wednesday
    assert_eq!(period_start_day(today, StatsPeriod::Days(7)), Some(today - 6));
    assert_eq!(period_start_day(today, StatsPeriod::Days(30)), Some(today - 29));
    assert_eq!(period_start_day(today, StatsPeriod::All), None);
    // 52 calendar weeks ending with this one: Monday 2025-01-13 is 51 weeks before 2026-01-05.
    assert_eq!(period_start_day(today, StatsPeriod::Year), Some(day_of(2025, 1, 13)));
    assert_eq!(stats_period(None), StatsPeriod::All);
    assert_eq!(stats_period(Some(0)), StatsPeriod::All);
    assert_eq!(stats_period(Some(30)), StatsPeriod::Days(30));
    assert_eq!(stats_period(Some(365)), StatsPeriod::Year);
}

#[test]
fn period_top_ranks_by_the_copies_inside_the_period() {
    let today = 1000u64;
    let prompts = vec![stat_prompt("a", "Alpha"), stat_prompt("b", "Beta"), stat_prompt("c", "Gamma")];
    let rows: Vec<(u64, String, usize)> = vec![
        (today - 20, "a".into(), 100), // long before the 7-day period
        (today - 3, "a".into(), 2),
        (today - 1, "b".into(), 5),
        (today, "c".into(), 2),
        (today, "gone".into(), 50), // deleted prompt
    ];
    let live: std::collections::HashSet<&str> = ["a", "b", "c"].into_iter().collect();
    let totals = period_totals(&rows, period_start_day(today, StatsPeriod::Days(7)), &live);
    let top = period_top(&prompts, &totals, 10);
    let names: Vec<(&str, u32)> = top.iter().map(|b| (b.name.as_str(), b.count)).collect();
    assert_eq!(names, [("Beta", 5), ("Alpha", 2), ("Gamma", 2)], "ties by name, the old 100 copies do not count");
    assert_eq!(totals.values().sum::<u64>(), 9);
    assert_eq!(period_top(&prompts, &totals, 1).len(), 1);
    // The 30-day window takes in the long-ago copies.
    let totals30 = period_totals(&rows, period_start_day(today, StatsPeriod::Days(30)), &live);
    assert_eq!(period_top(&prompts, &totals30, 10)[0].name, "Alpha");
}

#[test]
fn period_never_used_lists_prompts_without_a_copy_in_the_period() {
    let today = 1000u64;
    let prompts = vec![stat_prompt("a", "Alpha"), stat_prompt("b", "Beta"), stat_prompt("c", "Gamma")];
    let rows: Vec<(u64, String, usize)> = vec![(today - 20, "a".into(), 4), (today - 1, "b".into(), 1)];
    let live: std::collections::HashSet<&str> = ["a", "b", "c"].into_iter().collect();
    let t7 = period_totals(&rows, period_start_day(today, StatsPeriod::Days(7)), &live);
    let names7: Vec<String> = period_never_used(&prompts, &t7, None, 10).into_iter().map(|b| b.name).collect();
    assert_eq!(names7, ["Alpha", "Gamma"]);
    let t30 = period_totals(&rows, period_start_day(today, StatsPeriod::Days(30)), &live);
    let names30: Vec<String> = period_never_used(&prompts, &t30, None, 10).into_iter().map(|b| b.name).collect();
    assert_eq!(names30, ["Gamma"]);
    assert_eq!(period_never_used(&prompts, &t30, None, 0).len(), 0);
}

#[test]
fn the_year_chart_counts_per_week_across_a_year_boundary() {
    let today = day_of(2026, 1, 7);
    let live: std::collections::HashSet<&str> = ["a"].into_iter().collect();
    let rows: Vec<(u64, String, usize)> = vec![
        (day_of(2025, 12, 28), "a".into(), 1), // the Sunday that ends the week before
        (day_of(2025, 12, 31), "a".into(), 2),
        (day_of(2026, 1, 1), "a".into(), 3),
        (day_of(2026, 1, 5), "a".into(), 4),
        (today, "a".into(), 1),
        (day_of(2024, 1, 1), "a".into(), 9), // before the 52 weeks
    ];
    let weeks = period_daily(&rows, today, StatsPeriod::Year, &live);
    assert_eq!(weeks.len(), 52);
    let n = weeks.len();
    assert_eq!(weeks[n - 1].count, 5, "Mon 2026-01-05 and Wed 2026-01-07");
    assert_eq!(weeks[n - 1].ts, day_of(2026, 1, 5) * 86400);
    assert_eq!(weeks[n - 2].count, 5, "Wed 2025-12-31 and Thu 2026-01-01 share a week");
    assert_eq!(weeks[n - 2].ts, day_of(2025, 12, 29) * 86400);
    assert_eq!(weeks[n - 3].count, 1);
    assert_eq!(weeks.iter().map(|w| w.count).sum::<usize>(), 11);
    assert_eq!(weeks[0].ts, day_of(2025, 1, 13) * 86400);
}

#[test]
fn the_all_time_chart_counts_per_month_from_the_oldest_row() {
    let today = day_of(2026, 1, 7);
    let live: std::collections::HashSet<&str> = ["a", "b"].into_iter().collect();
    let rows: Vec<(u64, String, usize)> = vec![
        (day_of(2025, 11, 20), "a".into(), 2),
        (day_of(2025, 12, 31), "a".into(), 1),
        (day_of(2026, 1, 1), "b".into(), 4),
        (day_of(2026, 1, 7), "gone".into(), 8),
    ];
    let months = period_daily(&rows, today, StatsPeriod::All, &live);
    let counts: Vec<usize> = months.iter().map(|m| m.count).collect();
    assert_eq!(counts, [2, 1, 4]);
    assert_eq!(months[0].ts, day_of(2025, 11, 1) * 86400);
    assert_eq!(months[2].ts, day_of(2026, 1, 1) * 86400);
    assert!(period_daily(&[], today, StatsPeriod::All, &live).is_empty());
}

#[test]
fn the_day_chart_has_one_entry_per_day_of_the_period() {
    let today = day_of(2026, 1, 7);
    let live: std::collections::HashSet<&str> = ["a"].into_iter().collect();
    let rows: Vec<(u64, String, usize)> = vec![(today, "a".into(), 3), (today - 6, "a".into(), 1), (today - 7, "a".into(), 5)];
    let days = period_daily(&rows, today, StatsPeriod::Days(7), &live);
    assert_eq!(days.len(), 7);
    assert_eq!(days.iter().map(|d| d.count).collect::<Vec<_>>(), [1, 0, 0, 0, 0, 0, 3]);
}
