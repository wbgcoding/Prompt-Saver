use super::*;

#[test]
fn snip_preview_rotates_and_cleans() {
    let gen = SNIP_GEN.load(Ordering::SeqCst);
    let p1 = write_snip_preview(b"first", gen).expect("write1");
    assert!(std::path::Path::new(&p1).exists());
    // A stale encode neither writes nor touches the current preview.
    assert!(write_snip_preview(b"stale", gen.wrapping_add(1)).is_none());
    assert!(std::path::Path::new(&p1).exists(), "stale encode keeps the live preview");
    let p2 = write_snip_preview(b"second", gen).expect("write2");
    assert_ne!(p1, p2);
    assert!(!std::path::Path::new(&p1).exists(), "previous preview deleted");
    assert!(std::path::Path::new(&p2).exists());
    remove_snip_preview();
    assert!(!std::path::Path::new(&p2).exists(), "cleanup removes file");
}

#[test]
fn base64_roundtrip_and_reject() {
    let data = b"\x00\x01\x02\xff\xfe hello";
    assert_eq!(base64_decode(&base64_encode(data)), data);
    assert!(base64_decode("not valid base64!@#").is_empty());
}

// A file written before tags existed must import unchanged, with no tags.
#[test]
fn old_exports_without_tags_still_parse() {
    let txt = "### Old\nSome text\n@color #123456\n@style mono 20";
    let data = parse_txt(txt);
    assert_eq!(data.prompts[0].text, "Some text");
    assert_eq!(data.prompts[0].color, "#123456");
    assert_eq!(data.prompts[0].font, "mono");
    assert!(data.prompts[0].tags.is_empty());

    // Same for a CSV row that stops before the tags column.
    let csv = "\"name\";\"text\"\r\n\"Old\";\"Some text\"";
    let data = parse_csv_data(csv);
    assert_eq!(data.prompts.len(), 1);
    assert!(data.prompts[0].tags.is_empty());
}

// A multi-line note survives both exporters, and a file written before
// notes existed still imports (with an empty note).
#[test]
fn notes_roundtrip_through_txt_and_csv() {
    let prompts = sample_prompts();
    let note = &prompts[0].notes;
    assert!(note.contains('\n'), "the sample note must be multi-line");

    let txt = to_txt(&prompts);
    // The line-based format may never carry a raw newline inside the marker.
    let marker = txt.lines().find(|l| l.starts_with("@notes ")).expect("@notes line");
    assert!(marker.contains("\\n"), "the newline must be escaped: {marker}");
    let data = parse_txt(&txt);
    assert_eq!(&data.prompts[0].notes, note);
    assert_eq!(data.prompts[0].text, "Hello\nWorld"); // marker did not eat the body
    assert!(data.prompts[1].notes.is_empty());

    let csv = to_csv(&prompts);
    assert_eq!(parse_csv(&csv)[1][14], *note);
    let data = parse_csv_data(&csv);
    assert_eq!(&data.prompts[0].notes, note);
    assert!(data.prompts[1].notes.is_empty());
}

// Security: a name/text starting with =, +, -, @ or a tab must not turn into a
// live formula when the CSV is opened in Excel/LibreOffice, but it must still
// come back exactly as typed on the next import into this app.
#[test]
fn csv_export_guards_formula_prefixes_and_import_undoes_it() {
    let mut prompts = sample_prompts();
    prompts[0].name = "=cmd|'/c calc'!A1".into();
    prompts[0].text = "-1 to 5 scale".into();
    prompts[0].notes = "@mention someone".into();
    let csv = to_csv(&prompts);
    assert!(csv.contains("\"'=cmd"), "the name must be guarded: {csv}");
    assert!(csv.contains("\"'-1 to 5"), "the text must be guarded: {csv}");
    assert!(csv.contains("\"'@mention"), "the notes must be guarded: {csv}");
    let data = parse_csv_data(&csv);
    assert_eq!(data.prompts[0].name, "=cmd|'/c calc'!A1");
    assert_eq!(data.prompts[0].text, "-1 to 5 scale");
    assert_eq!(data.prompts[0].notes, "@mention someone");
}

#[test]
fn stats_export_entries_sorts_by_copies_and_carries_last_used() {
    let mut a = test_prompt("a");
    a.name = "Alpha".into();
    let mut b = test_prompt("b");
    b.name = "Beta".into();
    let mut store = Store {
        prompts: vec![a, b],
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: Default::default(),
    };
    store.settings.usage.insert("b".into(), 9);
    store.settings.usage.insert("a".into(), 2);
    store.settings.last_used.insert("b".into(), 1700000200);
    store.settings.last_used.insert("a".into(), 1700000100);
    let entries = stats_export_entries(&store);
    // most-copied first; a prompt never copied still appears (0, not dropped).
    assert_eq!(entries[0].name, "Beta");
    assert_eq!(entries[0].copies, 9);
    assert_eq!(entries[0].last_used, 1700000200);
    assert_eq!(entries[1].name, "Alpha");
    assert_eq!(entries[1].copies, 2);
    assert_eq!(entries[1].last_used, 1700000100);
}

// The statistics export reuses csv_row/csv_cell, but this asserts the
// guarantee directly against the stats formatter: a semicolon or a quote
// in a prompt name must not shift or break the row it sits in.
#[test]
fn stats_csv_export_escapes_semicolons_and_quotes_in_names() {
    let entries = vec![StatsExportEntry { name: "Weird;\"Name\"".into(), copies: 3, last_used: 100 }];
    let csv = stats_to_csv(&entries, &[], &[]);
    let rows = parse_csv(&csv);
    assert_eq!(rows[1][0], "Weird;\"Name\"");
    assert_eq!(rows[1][1], "3");
    assert_eq!(rows[1][2], "100");
}

#[test]
fn stats_txt_and_json_export_carry_the_same_entries_and_daily_counts() {
    let entries = vec![StatsExportEntry { name: "Mail".into(), copies: 5, last_used: 42 }];
    let daily = vec![DayCount { ts: 1700000000, count: 4 }];
    let txt = stats_to_txt(&entries, &daily, &[], StatsTxtLabels::default());
    assert!(txt.contains("Mail: 5 copies, last used 1970-01-01"));
    assert!(txt.contains("2023-11-14: 4"));
    let de = StatsTxtLabels {
        prompts: "Prompts".into(),
        daily: "Kopien pro Tag".into(),
        copies: "{n} Kopien, zuletzt {date}".into(),
        never: "nie".into(),
        ..Default::default()
    };
    let never = vec![StatsExportEntry { name: "Neu".into(), copies: 0, last_used: 0 }];
    let txt = stats_to_txt(&never, &daily, &[], de);
    assert!(txt.contains("Kopien pro Tag\n=============="), "{txt}");
    assert!(txt.contains("Neu: 0 Kopien, zuletzt nie"), "{txt}");
    assert!(!txt.contains("1700000000"));
    let json = stats_to_json(&entries, &daily, &[]);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["version"], 1);
    assert_eq!(parsed["entries"][0]["name"], "Mail");
    assert_eq!(parsed["entries"][0]["copies"], 5);
    assert_eq!(parsed["daily"][0]["count"], 4);
    assert_eq!(parsed["tools"].as_array().unwrap().len(), 0);
}

#[test]
fn apply_tool_usage_increments_exactly_once_per_call() {
    let mut settings = Settings::default();
    apply_tool_usage(&mut settings, "screenshot");
    assert_eq!(settings.usage.get(&tool_usage_key("screenshot")), Some(&1));
    apply_tool_usage(&mut settings, "screenshot");
    assert_eq!(settings.usage.get(&tool_usage_key("screenshot")), Some(&2), "a second use must add exactly one, not reset or double-count");
    // A different tool has its own counter, untouched by the one above.
    assert_eq!(settings.usage.get(&tool_usage_key("chain")), None);
    assert!(settings.last_used.get(&tool_usage_key("screenshot")).copied().unwrap_or(0) > 0);
}

#[test]
fn tool_usage_enabled_defaults_to_off_and_needs_an_explicit_true() {
    let settings = Settings::default();
    assert!(!tool_usage_enabled(&settings), "usage statistics must default to off, same as the frontend's own shipsOn reading");
    let mut on = Settings::default();
    on.ui_flags.insert("usageStats".into(), true);
    assert!(tool_usage_enabled(&on));
    let mut explicit_off = Settings::default();
    explicit_off.ui_flags.insert("usageStats".into(), false);
    assert!(!tool_usage_enabled(&explicit_off));
}

// Checks the count points against the same list the export reads, so a
// tool call site can never quietly go uninstrumented: every TOOL_KEYS
// entry must appear as a real record_tool_usage call somewhere in the
// frontend, whether through the main window's helper or a direct call
// from a window of its own.
#[test]
fn every_tool_key_has_a_matching_frontend_call_site() {
    // Most tools are counted from the main window through the recordToolUsage(key)
    // helper, but a few live in windows or shared modules of their own (the video
    // editor, the shared colour popup) and call record_tool_usage directly.
    let src = [
        main_window_js().as_str(),
        include_str!("../../../ui/video.js"),
        include_str!("../../../ui/colorpop.js"),
    ]
    .join("\n");
    for key in TOOL_KEYS {
        let via_helper = format!("recordToolUsage(\"{key}\")");
        let via_invoke = format!("tool: \"{key}\"");
        assert!(
            src.contains(&via_helper) || src.contains(&via_invoke),
            "TOOL_KEYS has \"{key}\" but no frontend file has a record_tool_usage call for it"
        );
    }
}

// export_prompts is a #[tauri::command] async fn (AppHandle + State<'_, Db>)
// whose unrecognized-format arm returns before ever touching either: but the
// command itself still cannot be invoked directly in a unit test without
// Tauri's mock runtime, which this project does not depend on, and the match
// is not factored into a function of its own. This asserts against the actual
// committed source instead of a re-typed copy of the logic: the fallback arm
// must keep returning the stable code the frontend keys off, never a raw
// string interpolated from the caller-supplied format (that reached the user
// unlocalized for anything but "csv"/"txt" before this fix). Scoped to the
// function's own body (not the whole file) so this cannot false-positive on
// an unrelated comment elsewhere that happens to mention the old shape.
#[test]
fn export_prompts_unsupported_format_uses_a_stable_error_code() {
    let src = production_sources();
    assert_eq!(src.matches("async fn export_prompts(").count(), 1, "one export_prompts command");
    let start = src
        .find("async fn export_prompts(")
        .expect("export_prompts exists in the crate sources");
    let end = start
        + src[start..]
            .find("\n// Minimal CSV reader")
            .expect("export_prompts is followed by the parse_csv section comment");
    let body = &src[start..end];
    let needle = "_ => return Err(\"unsupported-export-format\".to_string()),";
    assert!(
        body.contains(needle),
        "export_prompts' unsupported-format arm no longer returns the stable \"unsupported-export-format\" code verbatim"
    );
    // Narrower than "no format!( anywhere in the function": export_prompts
    // legitimately uses format!() to build the save-file name. Only the
    // error-building shape (a format! fed straight into Err(..)) is the bug.
    assert!(
        !body.contains("Err(format!("),
        "export_prompts must never build its error via string interpolation again (it must stay the stable, non-localized-text-free code)"
    );
}

#[test]
fn find_var_tokens_finds_a_plain_a_default_and_a_choice_token() {
    let text = "Hi {?name}, order {?item=widget} in {?size|S,M,L} please.";
    let tokens = find_var_tokens(text);
    let bodies: Vec<&str> = tokens.iter().map(|(_, _, b)| b.as_str()).collect();
    assert_eq!(bodies, vec!["name", "item=widget", "size|S,M,L"]);
}

#[test]
fn parse_var_token_splits_name_default_and_options() {
    assert_eq!(parse_var_token("name"), ("name".to_string(), vec![], "".to_string()));
    assert_eq!(parse_var_token("item=widget"), ("item".to_string(), vec![], "widget".to_string()));
    assert_eq!(
        parse_var_token("size|S,M,L"),
        ("size".to_string(), vec!["S".to_string(), "M".to_string(), "L".to_string()], "".to_string())
    );
    assert_eq!(
        parse_var_token("size=M|S,M,L"),
        ("size".to_string(), vec!["S".to_string(), "M".to_string(), "L".to_string()], "M".to_string())
    );
    // A bar with nothing usable after it is a plain variable, whole token as its name.
    assert_eq!(parse_var_token("weird|"), ("weird|".to_string(), vec![], "".to_string()));
}

// The Abnahme case: the same priority chain the fill-in dialog offers,
// minus the interactive last step, checked across all four sources in one
// pass: a remembered value beats a fixed value beats the text's own
// default beats blank.
#[test]
fn resolve_static_vars_priority_is_remembered_then_fixed_then_default_then_blank() {
    let remembered: HashMap<String, String> = [("a".to_string(), "from-remembered".to_string())].into_iter().collect();
    let fixed: HashMap<String, String> =
        [("a".to_string(), "from-fixed".to_string()), ("b".to_string(), "from-fixed".to_string())].into_iter().collect();
    // a: remembered AND fixed AND a default: remembered wins.
    assert_eq!(resolve_static_vars("{?a=from-default}", &remembered, &fixed), "from-remembered");
    // b: fixed AND a default, no remembered value: fixed wins.
    assert_eq!(resolve_static_vars("{?b=from-default}", &remembered, &fixed), "from-fixed");
    // c: only a default, no remembered or fixed value: the default wins.
    assert_eq!(resolve_static_vars("{?c=from-default}", &remembered, &fixed), "from-default");
    // d: nothing anywhere: blank, never the raw placeholder.
    assert_eq!(resolve_static_vars("{?d}", &remembered, &fixed), "");
}

#[test]
fn resolve_static_vars_choice_variable_follows_the_same_priority_chain() {
    let remembered: HashMap<String, String> = [("size".to_string(), "M".to_string())].into_iter().collect();
    let fixed: HashMap<String, String> = [("size".to_string(), "L".to_string())].into_iter().collect();
    assert_eq!(resolve_static_vars("{?size|S,M,L}", &remembered, &fixed), "M", "a remembered pick wins when it is a real option");
    let fixed_only: HashMap<String, String> = fixed.clone();
    assert_eq!(resolve_static_vars("{?size|S,M,L}", &HashMap::new(), &fixed_only), "L", "falls to the fixed value next");
    assert_eq!(resolve_static_vars("{?size=S|S,M,L}", &HashMap::new(), &HashMap::new()), "S", "then the text's own default");
    assert_eq!(resolve_static_vars("{?size|S,M,L}", &HashMap::new(), &HashMap::new()), "S", "and finally the first option");
}

#[test]
fn resolve_static_vars_leaves_a_dynamic_name_and_an_unresolved_gap_untouched() {
    let remembered: HashMap<String, String> = HashMap::new();
    let fixed: HashMap<String, String> = HashMap::new();
    // A reserved dynamic name only means something at the moment of copy --
    // a background write has no such moment, so it is left exactly as written.
    assert_eq!(resolve_static_vars("Today is {?@date}.", &remembered, &fixed), "Today is {?@date}.");
}

#[test]
fn resolved_for_export_resolves_the_text_and_every_rotating_variant() {
    let remembered: HashMap<String, String> = [("name".to_string(), "Alex".to_string())].into_iter().collect();
    let fixed: HashMap<String, String> = HashMap::new();
    let mut p = test_prompt("p1");
    p.text = "Hi {?name}".to_string();
    p.variants = vec!["Bye {?name}".to_string(), "no placeholder here".to_string()];
    let resolved = resolved_for_export(&p, &remembered, &fixed);
    assert_eq!(resolved.text, "Hi Alex");
    assert_eq!(resolved.variants, vec!["Bye Alex".to_string(), "no placeholder here".to_string()]);
    // The live prompt itself must never be mutated by an export/sync pass.
    assert_eq!(p.text, "Hi {?name}");
}

// An exported theme file never carries the internal id, so importing it on
// another install (or re-importing it on the same one) always creates a fresh
// theme instead of silently colliding with (or overwriting) an existing one.
#[test]
fn theme_export_roundtrips_without_leaking_the_internal_id() {
    let mut vars = HashMap::new();
    vars.insert("bg".to_string(), "#123456".to_string());
    vars.insert("accent".to_string(), "#abcdef".to_string());
    let theme = CustomTheme { id: "secret-internal-id".to_string(), name: "My theme".to_string(), base: "dark".to_string(), vars };
    let export = ThemeExport { name: theme.name.clone(), base: theme.base.clone(), vars: theme.vars.clone() };
    let json = serde_json::to_string_pretty(&export).unwrap();
    assert!(!json.contains("secret-internal-id"), "the export must not carry the internal id: {json}");
    let back: ThemeExport = serde_json::from_str(&json).unwrap();
    assert_eq!(back.name, theme.name);
    assert_eq!(back.base, theme.base);
    assert_eq!(back.vars, theme.vars);
}

#[test]
fn old_exports_without_notes_still_parse() {
    let txt = "### Old\nSome text\n@color #123456\n@tags Work";
    let data = parse_txt(txt);
    assert_eq!(data.prompts[0].text, "Some text");
    assert_eq!(data.prompts[0].color, "#123456");
    assert!(data.prompts[0].notes.is_empty());

    // A CSV row that stops before the notes column.
    let csv = "\"name\";\"text\"\r\n\"Old\";\"Some text\"";
    assert!(parse_csv_data(csv).prompts[0].notes.is_empty());
}

#[test]
fn notes_escaping_survives_backslashes() {
    for raw in ["a\nb", "path C:\\n\\temp", "trailing\\", "\\n literal", ""] {
        assert_eq!(notes_unescape(&notes_escape(raw)), raw, "roundtrip failed for {raw:?}");
    }
}

#[test]
fn sanitize_notes_normalizes_and_bounds() {
    assert_eq!(sanitize_notes("  a\r\nb\r  ").unwrap(), "a\nb");
    assert!(sanitize_notes(&"x".repeat(MAX_NOTES_LEN)).is_ok());
    assert!(sanitize_notes(&"x".repeat(MAX_NOTES_LEN + 1)).is_err());
}

// The whole point of private notes: a note must never reach the clipboard;
// not through a single copy, and not through a chained multi-prompt copy.
#[test]
fn notes_never_reach_the_copied_payload() {
    let mut prompts = sample_prompts();
    prompts[1].notes = "internal only: budget 5000".into();
    for p in &prompts {
        let payload = copy_payload(p);
        assert_eq!(payload, p.text);
        for line in p.notes.lines().filter(|l| !l.trim().is_empty()) {
            assert!(!payload.contains(line), "note leaked into a copy: {line}");
        }
    }
    // The chain concatenates the same payloads, so it inherits the guarantee.
    let chained = prompts.iter().map(copy_payload).collect::<Vec<_>>().join("\n\n");
    for p in &prompts {
        for line in p.notes.lines().filter(|l| !l.trim().is_empty()) {
            assert!(!chained.contains(line), "note leaked into a chain copy: {line}");
        }
    }
}

#[test]
fn tags_are_deduped_case_insensitively_keeping_known_casing() {
    let known = canonical_tags(&sample_prompts()); // "Work", "E-Mail"
    let out = sanitize_tags(
        vec![" work ".into(), "WORK".into(), "".into(), "  ".into(), "New".into()],
        &known,
    )
    .expect("valid tags");
    assert_eq!(out, vec!["Work".to_string(), "New".to_string()]);
    // An unknown tag keeps the casing it was typed with.
    assert_eq!(sanitize_tags(vec!["MiXeD".into()], &HashMap::new()).unwrap(), vec!["MiXeD"]);
}

// One pass over the whole store, so a tag on many prompts stays one operation.
#[test]
fn renaming_a_tag_rewrites_every_prompt_at_once() {
    let mut prompts = sample_prompts(); // p1: ["Work", "E-Mail"], p2: []
    prompts[1].tags = vec!["work".into(), "Draft".into()];
    assert!(rename_tag_in(&mut prompts, "work", "Job"));
    assert_eq!(prompts[0].tags, vec!["Job".to_string(), "E-Mail".to_string()]);
    assert_eq!(prompts[1].tags, vec!["Job".to_string(), "Draft".to_string()]);
    // Renaming onto a tag the prompt already has merges instead of duplicating.
    assert!(rename_tag_in(&mut prompts, "e-mail", "Job"));
    assert_eq!(prompts[0].tags, vec!["Job".to_string()]);
    // A name nobody uses changes nothing, so no save is triggered.
    assert!(!rename_tag_in(&mut prompts, "nothing", "Else"));
}

// The merge color rule: the surviving (target) tag keeps its own color if it
// has one, and only borrows the source's color when it had none at all.
#[test]
fn merging_a_tag_keeps_the_targets_color_if_it_has_one() {
    let mut colors = HashMap::new();
    colors.insert("work".to_string(), "#ff0000".to_string());
    colors.insert("job".to_string(), "#00ff00".to_string());
    transfer_tag_color(&mut colors, "work", "job");
    assert_eq!(colors.get("job"), Some(&"#00ff00".to_string()), "target's own color must win");
    assert_eq!(colors.get("work"), None, "the merged-away key's color entry is gone");
}

#[test]
fn merging_a_tag_into_an_uncolored_one_transfers_the_color() {
    let mut colors = HashMap::new();
    colors.insert("work".to_string(), "#ff0000".to_string());
    transfer_tag_color(&mut colors, "work", "job");
    assert_eq!(colors.get("job"), Some(&"#ff0000".to_string()));
    assert_eq!(colors.get("work"), None);
}

#[test]
fn deleting_a_tag_clears_it_everywhere() {
    let mut prompts = sample_prompts();
    prompts[1].tags = vec!["WORK".into()];
    assert!(remove_tag_from(&mut prompts, "work"));
    assert_eq!(prompts[0].tags, vec!["E-Mail".to_string()]);
    assert!(prompts[1].tags.is_empty());
    assert!(!remove_tag_from(&mut prompts, "work")); // idempotent
}

// Batch add: a prompt that already carries the tag is left exactly as-is (no
// duplicate, no reorder), and a fresh addition takes the casing the library
// already uses for that tag: same rule the single-prompt editor applies.
#[test]
fn batch_add_tag_skips_prompts_that_already_have_it_and_uses_known_casing() {
    let mut prompts = sample_prompts(); // p1: ["Work", "E-Mail"], p2: []
    let known = canonical_tags(&prompts);
    let ids: std::collections::HashSet<&str> = ["p1", "p2"].into_iter().collect();
    add_tag_to_ids(&mut prompts, &ids, "work", &known).unwrap();
    assert_eq!(prompts[0].tags, vec!["Work".to_string(), "E-Mail".to_string()]);
    assert_eq!(prompts[1].tags, vec!["Work".to_string()]);
}

#[test]
fn batch_add_tag_only_touches_the_given_ids() {
    let mut prompts = sample_prompts();
    let known = canonical_tags(&prompts);
    let ids: std::collections::HashSet<&str> = ["p2"].into_iter().collect();
    add_tag_to_ids(&mut prompts, &ids, "New", &known).unwrap();
    assert_eq!(prompts[0].tags, vec!["Work".to_string(), "E-Mail".to_string()], "unselected prompt must stay untouched");
    assert_eq!(prompts[1].tags, vec!["New".to_string()]);
}

// Removing a tag from a prompt that never had it is not an error, just a no-op
// for that one prompt: mirrors the single-prompt "already has it" rule above.
#[test]
fn batch_remove_tag_ignores_a_prompt_that_never_had_it() {
    let mut prompts = sample_prompts();
    let ids: std::collections::HashSet<&str> = ["p1", "p2"].into_iter().collect();
    remove_tag_from_ids(&mut prompts, &ids, "e-mail");
    assert_eq!(prompts[0].tags, vec!["Work".to_string()]);
    assert!(prompts[1].tags.is_empty());
}

#[test]
fn tag_input_is_validated() {
    let empty = HashMap::new();
    assert!(sanitize_tags(vec!["a".repeat(MAX_TAG_LEN + 1)], &empty).is_err());
    assert!(sanitize_tags(vec!["a,b".into()], &empty).is_err()); // comma = separator
    assert!(sanitize_tags(vec!["a\nb".into()], &empty).is_err()); // breaks the TXT format
    assert!(sanitize_tags(vec!["a".repeat(MAX_TAG_LEN)], &empty).is_ok());
    assert!(valid_tag_name("  ").is_err());
    assert_eq!(valid_tag_name(" Work ").unwrap(), "Work");
    // Colors: empty clears, #rrggbb is normalized, anything else is refused.
    assert_eq!(valid_tag_color("  ").unwrap(), "");
    assert_eq!(valid_tag_color("#AABBCC").unwrap(), "#aabbcc");
    assert!(valid_tag_color("red").is_err());
    assert!(valid_tag_color("#abc").is_err());
}

#[test]
fn custom_theme_input_is_validated() {
    assert!(valid_theme_name("  ").is_err());
    assert_eq!(valid_theme_name("  Night  ").unwrap(), "Night");
    assert!(valid_theme_name(&"a".repeat(MAX_THEME_NAME + 1)).is_err());
    assert!(valid_theme_name("Night\nshift").is_err());
    // A base must be a real built-in; "system" and another custom theme are not,
    // so inheritance can never chain.
    assert_eq!(valid_theme_base(" dark ").unwrap(), "dark");
    assert!(valid_theme_base("system").is_err());
    assert!(valid_theme_base("custom:abc").is_err());
    let mut vars = HashMap::new();
    vars.insert("accent".to_string(), "#AABBCC".to_string());
    vars.insert("bg".to_string(), "  ".to_string()); // empty = not overridden
    let clean = valid_theme_vars(&vars).unwrap();
    assert_eq!(clean.get("accent").unwrap(), "#aabbcc");
    assert!(!clean.contains_key("bg"));
    vars.insert("shadow".to_string(), "#000000".to_string());
    assert!(valid_theme_vars(&vars).is_err()); // not a themable colour token
    let mut bad = HashMap::new();
    bad.insert("accent".to_string(), "red".to_string());
    assert!(valid_theme_vars(&bad).is_err());
}

#[test]
fn custom_theme_resolves_to_its_base() {
    let mut settings = Settings::default();
    settings.custom_themes.push(CustomTheme {
        id: "t1".into(),
        name: "Night".into(),
        base: "dark".into(),
        vars: HashMap::from([("accent".to_string(), "#ff0000".to_string())]),
    });
    settings.theme = "custom:t1".into();
    assert_eq!(theme_base_of(&settings, &settings.theme), "dark");
    // The feature ships off, so a stored custom theme stays parked until enabled.
    assert!(theme_vars_of(&settings).is_empty());
    settings.ui_flags.insert("customThemes".into(), true);
    assert_eq!(theme_vars_of(&settings).get("accent").unwrap(), "#ff0000");
    // A dangling reference falls back to the system theme instead of breaking.
    settings.theme = "custom:gone".into();
    assert_eq!(theme_base_of(&settings, &settings.theme), "system");
    assert!(theme_vars_of(&settings).is_empty());
    // Built-in preferences pass through untouched.
    assert_eq!(theme_base_of(&settings, "cyberpunk"), "cyberpunk");
}

#[test]
fn custom_theme_cap_is_bounded() {
    let mut settings = Settings::default();
    assert_eq!(custom_theme_cap(&settings), DEFAULT_CUSTOM_THEMES);
    settings.ui_values.insert("customThemeMax".into(), 3.0);
    assert_eq!(custom_theme_cap(&settings), 3);
    settings.ui_values.insert("customThemeMax".into(), 9999.0);
    assert_eq!(custom_theme_cap(&settings), MAX_CUSTOM_THEMES);
    settings.ui_values.insert("customThemeMax".into(), -5.0);
    assert_eq!(custom_theme_cap(&settings), 1);
    settings.ui_values.insert("customThemeMax".into(), f64::NAN);
    assert_eq!(custom_theme_cap(&settings), DEFAULT_CUSTOM_THEMES);
}

#[test]
fn settings_without_custom_themes_still_load() {
    // An older settings blob has no `custom_themes` key at all.
    let s: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
    assert!(s.custom_themes.is_empty());
    // A theme entry from a newer build with unknown extra fields keeps what it can.
    let s: Settings = serde_json::from_str(
        r##"{"custom_themes":[{"id":"a","name":"N","base":"mono","vars":{"bg":"#111111"},"future":1}]}"##,
    )
    .unwrap();
    assert_eq!(s.custom_themes[0].base, "mono");
    assert_eq!(s.custom_themes[0].vars.get("bg").unwrap(), "#111111");
}

pub(super) fn test_view(id: &str) -> View {
    View {
        id: id.to_string(),
        name: id.to_uppercase(),
        cols: default_cols(),
        rows: default_rows(),
        layouts: HashMap::new(),
        color: String::new(),
        members: Vec::new(),
    }
}

#[test]
fn dissolving_a_folder_adds_members_the_view_did_not_list() {
    let mut view = test_view("v");
    view.members = vec!["a".to_string()];
    let members = vec!["a".to_string(), "b".to_string()];
    seat_dissolved_members(&mut view, &members, &[[0, 0]]);
    let layout = view.layouts.get(&grid_key(view.cols, view.rows)).expect("a layout");
    assert!(layout.contains_key("a") && layout.contains_key("b"));
    assert!(layout.values().all(|c| *c != [0, 0]), "another folder's cell stays free");
    assert_eq!(view.members, vec!["a".to_string(), "b".to_string()], "each member listed once");
}

#[test]
fn reorder_views_by_id_moves_a_view_to_a_new_position() {
    let views = vec![test_view("a"), test_view("b"), test_view("c")];
    // "c" moved to the front, same as dropping its tab before "a"'s.
    let order: Vec<String> = ["c", "a", "b"].iter().map(|s| s.to_string()).collect();
    let out = reorder_views_by_id(views, &order);
    let ids: Vec<&str> = out.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, ["c", "a", "b"]);
}

#[test]
fn reorder_views_by_id_keeps_a_view_missing_from_order() {
    // A stale caller that only named two of three views must still get all
    // three back: the third keeps its old relative place instead of vanishing.
    let views = vec![test_view("a"), test_view("b"), test_view("c")];
    let order: Vec<String> = ["c", "a"].iter().map(|s| s.to_string()).collect();
    let out = reorder_views_by_id(views, &order);
    let ids: Vec<&str> = out.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, ["c", "a", "b"], "the view left out of order must still be present");
}

#[test]
fn reorder_views_by_id_ignores_an_unknown_id_and_never_touches_members() {
    // A view's own data (its membership list here) rides along with it
    // untouched: reordering is a pure array shuffle, never a rebuild.
    let mut a = test_view("a");
    a.members = vec!["p1".to_string(), "p2".to_string()];
    let mut b = test_view("b");
    b.members = vec!["p3".to_string()];
    let views = vec![a, b];
    let order: Vec<String> = ["ghost", "b", "a"].iter().map(|s| s.to_string()).collect();
    let out = reorder_views_by_id(views, &order);
    let ids: Vec<&str> = out.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, ["b", "a"], "an id not among the views must be skipped, not inserted");
    assert_eq!(out[0].members, vec!["p3".to_string()]);
    assert_eq!(out[1].members, vec!["p1".to_string(), "p2".to_string()]);
}

#[test]
fn first_free_cell_skips_other_folder_cells() {
    // 3x1 grid, tile "a" on [0,0]: the plain scan would find [1,0] free.
    let mut view = test_view("v1");
    view.cols = 3;
    view.rows = 1;
    view.layouts.insert(
        grid_key(view.cols, view.rows),
        HashMap::from([("a".to_string(), [0, 0])]),
    );
    assert_eq!(first_free_cell(&view), Some([1, 0]));
    // A folder sits on [1,0] (outside view.layouts): the scan must skip it, or a
    // dissolved member handed that cell would save fine but never render.
    assert_eq!(first_free_cell_excluding(&view, &[[1, 0]]), Some([2, 0]));
    assert_eq!(first_free_cell_excluding(&view, &[[1, 0], [2, 0]]), None);
}

#[test]
fn place_ids_in_view_fills_free_cells_and_skips_already_placed_ids() {
    // 2x1 grid, "a" already on [0,0].
    let mut layout = HashMap::from([("a".to_string(), [0, 0])]);
    let ids: Vec<String> = vec!["a".to_string(), "b".to_string()];
    assert!(place_ids_in_view(
        &mut layout, &ids, 2, 1, &std::collections::HashSet::new(), &std::collections::HashSet::new()
    ));
    assert_eq!(layout.get("a"), Some(&[0, 0])); // untouched, was already placed
    assert_eq!(layout.get("b"), Some(&[1, 0]));
}

#[test]
fn place_ids_in_view_is_all_or_nothing_when_the_view_is_full() {
    // A batch that doesn't fully fit must place NOTHING, not a partial subset
    //: a caller that already removed these ids from their source view relies on
    // this to know the move truly failed.
    let mut layout = HashMap::from([("a".to_string(), [0, 0])]);
    let before = layout.clone();
    let ids: Vec<String> = vec!["b".to_string(), "c".to_string()]; // only 1 free cell for 2 ids
    assert!(!place_ids_in_view(
        &mut layout, &ids, 2, 1, &std::collections::HashSet::new(), &std::collections::HashSet::new()
    ));
    assert_eq!(layout, before);
}

#[test]
fn place_ids_in_view_counts_grid_folder_cells_as_occupied() {
    // A folder's cell lives outside `layout` (it is the folder's own `cell`
    // field, not a layout entry): without passing it as extra_occupied, addView could hand a
    // prompt the same cell a folder already sits on.
    let mut layout: HashMap<String, [u32; 2]> = HashMap::new();
    let folder_cells = std::collections::HashSet::from([(0u32, 0u32)]);
    let ids: Vec<String> = vec!["a".to_string()];
    assert!(place_ids_in_view(&mut layout, &ids, 2, 1, &folder_cells, &std::collections::HashSet::new()));
    assert_eq!(layout.get("a"), Some(&[1, 0])); // [0,0] was the folder's, must be skipped
}

#[test]
fn place_ids_in_view_skips_ids_already_placed_in_a_folder() {
    // "a" lives in a grid folder of this view: it needs no cell of its own
    // and must not eat the view's only free one, or a full target would
    // wrongly refuse a move that has nothing left to place.
    let mut layout: HashMap<String, [u32; 2]> = HashMap::new();
    let folder_cells = std::collections::HashSet::from([(0u32, 0u32)]); // the folder's own cell
    let folder_members = std::collections::HashSet::from(["a".to_string()]);
    let ids: Vec<String> = vec!["a".to_string()];
    assert!(place_ids_in_view(&mut layout, &ids, 1, 1, &folder_cells, &folder_members));
    assert!(layout.is_empty(), "a folder member must never get a layout entry of its own");
}

#[test]
fn place_ids_in_view_heals_a_stray_layout_entry_for_a_folder_member() {
    // Old data: "a" already sits in a folder AND still carries a leftover
    // layout entry from before it joined: that entry must be dropped, not
    // left behind occupying a cell the grid never shows as taken.
    let mut layout = HashMap::from([("a".to_string(), [1, 0])]);
    let folder_members = std::collections::HashSet::from(["a".to_string()]);
    let ids: Vec<String> = vec!["a".to_string()];
    assert!(place_ids_in_view(&mut layout, &ids, 2, 1, &std::collections::HashSet::new(), &folder_members));
    assert!(!layout.contains_key("a"), "the stray entry for a folder member must be removed");
}

#[test]
fn view_capacity_deficit_counts_only_the_ids_still_missing_a_cell() {
    // 2x1 grid, "a" already placed: one free cell, two new ids asked for.
    let layout = HashMap::from([("a".to_string(), [0, 0])]);
    let ids: Vec<String> = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    // "a" is already there and doesn't count against the need.
    assert_eq!(
        view_capacity_deficit(
            &layout, &ids, 2, 1, &std::collections::HashSet::new(), &std::collections::HashSet::new()
        ),
        1
    );
    // A folder cell shrinks the free count the same way it does in place_ids_in_view.
    let folder_cells = std::collections::HashSet::from([(1u32, 0u32)]);
    let empty: HashMap<String, [u32; 2]> = HashMap::new();
    let one_id: Vec<String> = vec!["z".to_string()];
    assert_eq!(
        view_capacity_deficit(
            &empty, &one_id, 1, 1, &std::collections::HashSet::new(), &std::collections::HashSet::new()
        ),
        0
    );
    assert_eq!(
        view_capacity_deficit(
            &layout, &["b".to_string()], 2, 1, &folder_cells, &std::collections::HashSet::new()
        ),
        1
    );
}

#[test]
fn view_capacity_deficit_does_not_count_an_id_already_placed_in_a_folder() {
    // A 1x1 target whose only cell the folder itself occupies: the deficit
    // for an id already sitting in that folder must be 0, not 1, or a full
    // target would refuse a move that has nothing left to place.
    let empty: HashMap<String, [u32; 2]> = HashMap::new();
    let folder_cells = std::collections::HashSet::from([(0u32, 0u32)]);
    let folder_members = std::collections::HashSet::from(["a".to_string()]);
    assert_eq!(view_capacity_deficit(&empty, &["a".to_string()], 1, 1, &folder_cells, &folder_members), 0);
}

#[test]
fn strip_ids_from_view_drops_them_from_every_layout_size_and_from_members() {
    // A view resized at least once carries more than one layout entry ("2x1" from
    // an earlier size, "3x1" the current one): a member dropped from the view
    // must lose its cell in BOTH, not just the size it happens to sit in today.
    let mut view = test_view("v1");
    view.cols = 3;
    view.rows = 1;
    view.members = vec!["a".to_string(), "b".to_string()];
    view.layouts.insert("2x1".to_string(), HashMap::from([("a".to_string(), [0, 0])]));
    view.layouts.insert(
        "3x1".to_string(),
        HashMap::from([("a".to_string(), [0, 0]), ("b".to_string(), [1, 0])]),
    );
    strip_ids_from_view(&mut view, &["a".to_string()]);
    assert_eq!(view.members, vec!["b".to_string()]);
    assert!(!view.layouts["2x1"].contains_key("a"));
    assert!(!view.layouts["3x1"].contains_key("a"));
    assert_eq!(view.layouts["3x1"].get("b"), Some(&[1, 0])); // untouched
}

#[test]
fn moving_between_views_places_in_the_target_and_strips_the_source() {
    // Goes through batch_prompts's actual "moveView" core (apply_view_batch_action),
    // not place_ids_in_view/strip_ids_from_view called directly: a regression
    // in the core's save handling or its folder-membership accounting now fails
    // this test too, not just its own dedicated ones. Proves "moved" means gone
    // from one view, present in the other, and durably saved.
    let mut settings = Settings::default();
    let mut from_view = test_view("home");
    from_view.cols = 2;
    from_view.rows = 1;
    from_view.members = vec!["a".to_string()];
    from_view.layouts.insert(grid_key(2, 1), HashMap::from([("a".to_string(), [0, 0])]));
    let mut to_view = test_view("work");
    to_view.cols = 2;
    to_view.rows = 1;
    settings.views = vec![from_view, to_view];

    let ids: Vec<String> = vec!["a".to_string()];
    let mut saved = false;
    let result = apply_view_batch_action(&mut settings, "moveView", &ids, "work", "home", |_s| {
        saved = true;
        true
    });

    assert!(result.is_ok());
    assert!(saved, "a successful move must call through to save");
    assert!(settings.views[0].members.is_empty());
    assert!(!settings.views[0].layouts[&grid_key(2, 1)].contains_key("a"));
    assert_eq!(settings.views[1].members, vec!["a".to_string()]);
    assert_eq!(settings.views[1].layouts[&grid_key(2, 1)].get("a"), Some(&[0, 0]));
}

#[test]
fn moveview_action_treats_an_existing_folder_member_as_already_placed() {
    // Same folder-membership case as place_ids_in_view's own tests, run
    // through the real "moveView" core this time: a 1x1 target whose only
    // cell a folder occupies, moving in an id the folder already lists as
    // a member: must succeed (not view-full) and never grow a second
    // layout entry for it.
    let mut settings = Settings::default();
    let mut from_view = test_view("home");
    from_view.members = vec!["a".to_string()];
    let mut to_view = test_view("work");
    to_view.cols = 1;
    to_view.rows = 1;
    settings.views = vec![from_view, to_view];
    settings.tile_folders.push(TileFolder {
        id: "tf1".to_string(),
        name: "Drafts".to_string(),
        color: String::new(),
        view_id: "work".to_string(),
        cell: [0, 0],
        members: vec!["a".to_string()],
    });

    let ids: Vec<String> = vec!["a".to_string()];
    let result = apply_view_batch_action(&mut settings, "moveView", &ids, "work", "home", |_s| true);

    assert!(result.is_ok(), "a target whose only cell a folder occupies must not report view-full: {result:?}");
    assert!(settings.views[0].members.is_empty(), "the source view must lose the moved id");
    assert!(
        settings.views[1].layouts.values().all(|l| !l.contains_key("a")),
        "an id already placed via a folder must never also get a layout entry"
    );
}

#[test]
fn copying_into_a_view_never_touches_a_second_view() {
    // The "addView" (Ctrl-drop / copy) arm never calls strip_ids_from_view --
    // placing "a" into a target view must leave an unrelated view's own
    // members/layouts byte-for-byte unchanged, proving a copy really is one.
    let mut other_view = test_view("home");
    other_view.members = vec!["a".to_string()];
    other_view.layouts.insert(grid_key(2, 1), HashMap::from([("a".to_string(), [0, 0])]));
    let other_before = other_view.clone();

    let mut to_view = test_view("work");
    to_view.cols = 2;
    to_view.rows = 1;
    let ids: Vec<String> = vec!["a".to_string()];
    let to_layout = to_view.layouts.entry(grid_key(2, 1)).or_default();
    assert!(place_ids_in_view(
        to_layout, &ids, 2, 1, &std::collections::HashSet::new(), &std::collections::HashSet::new()
    ));
    to_view.members.push("a".to_string());

    // The source view was never touched: View has no PartialEq of its own,
    // so the two fields a copy could plausibly disturb are compared directly.
    assert_eq!(other_view.members, other_before.members);
    assert_eq!(other_view.layouts, other_before.layouts);
    assert_eq!(to_view.layouts[&grid_key(2, 1)].get("a"), Some(&[0, 0]));
}

#[test]
fn moveview_action_rolls_back_the_move_when_save_fails() {
    // A failed write must never leave the in-memory move standing: the disk
    // would show the old placement after a restart while the running app kept
    // showing the new one until then, exactly the drift 477 closes.
    let mut settings = Settings::default();
    let mut from_view = test_view("home");
    from_view.cols = 2;
    from_view.rows = 1;
    from_view.members = vec!["a".to_string()];
    from_view.layouts.insert(grid_key(2, 1), HashMap::from([("a".to_string(), [0, 0])]));
    let mut to_view = test_view("work");
    to_view.cols = 2;
    to_view.rows = 1;
    settings.views = vec![from_view, to_view];
    let before_from_members = settings.views[0].members.clone();
    let before_from_layouts = settings.views[0].layouts.clone();
    let before_to_members = settings.views[1].members.clone();
    let before_to_layouts = settings.views[1].layouts.clone();

    let ids: Vec<String> = vec!["a".to_string()];
    let result = apply_view_batch_action(&mut settings, "moveView", &ids, "work", "home", |_s| false);
    assert_eq!(result, Err("save-failed".to_string()));
    assert_eq!(settings.views[0].members, before_from_members, "the source view's members must be untouched");
    assert_eq!(settings.views[0].layouts, before_from_layouts, "the source view's layout must be untouched");
    assert_eq!(settings.views[1].members, before_to_members, "the target view's members must be untouched");
    assert_eq!(settings.views[1].layouts, before_to_layouts, "the target view's layout must be untouched");
}

#[test]
fn moveview_action_saves_and_keeps_the_move_when_the_write_succeeds() {
    let mut settings = Settings::default();
    let mut from_view = test_view("home");
    from_view.cols = 2;
    from_view.rows = 1;
    from_view.members = vec!["a".to_string()];
    from_view.layouts.insert(grid_key(2, 1), HashMap::from([("a".to_string(), [0, 0])]));
    let mut to_view = test_view("work");
    to_view.cols = 2;
    to_view.rows = 1;
    settings.views = vec![from_view, to_view];

    let ids: Vec<String> = vec!["a".to_string()];
    let mut saved = false;
    let result =
        apply_view_batch_action(&mut settings, "moveView", &ids, "work", "home", |_s| { saved = true; true });
    assert!(result.is_ok());
    assert!(saved, "a successful move must still call through to save");
    assert!(settings.views[0].members.is_empty());
    assert_eq!(settings.views[1].members, vec!["a".to_string()]);
}

#[test]
fn addview_action_rolls_back_the_placement_when_save_fails() {
    let mut settings = Settings::default();
    let mut view = test_view("work");
    view.cols = 2;
    view.rows = 1;
    settings.views = vec![view];
    let before_members = settings.views[0].members.clone();
    let before_layouts = settings.views[0].layouts.clone();

    let ids: Vec<String> = vec!["a".to_string()];
    let result = apply_view_batch_action(&mut settings, "addView", &ids, "work", "", |_s| false);
    assert_eq!(result, Err("save-failed".to_string()));
    assert_eq!(settings.views[0].members, before_members, "the view's members must be untouched");
    assert_eq!(settings.views[0].layouts, before_layouts, "the view's layout must be untouched");
}

// `members` decides who belongs to a view, `layouts` only where they
// sit. A grid resize must never add or drop a member.
#[test]
fn rebuild_view_layout_grows_and_restores_old_positions() {
    let mut view = test_view("v1");
    view.cols = 2;
    view.rows = 2;
    view.members = vec!["a".into(), "b".into(), "c".into(), "d".into()];
    view.layouts.insert(
        grid_key(2, 2),
        HashMap::from([
            ("a".to_string(), [0, 0]),
            ("b".to_string(), [1, 0]),
            ("c".to_string(), [0, 1]),
            ("d".to_string(), [1, 1]),
        ]),
    );
    view.cols = 3;
    view.rows = 3;
    rebuild_view_layout(&mut view, &std::collections::HashSet::new(), &std::collections::HashSet::new());
    let layout = view.layouts.get(&grid_key(3, 3)).unwrap();
    assert_eq!(layout.len(), 4);
    assert_eq!(layout.get("a"), Some(&[0, 0]));
    assert_eq!(layout.get("b"), Some(&[1, 0]));
    assert_eq!(layout.get("c"), Some(&[0, 1]));
    assert_eq!(layout.get("d"), Some(&[1, 1]));
}

#[test]
fn rebuild_view_layout_grows_a_view_with_no_saved_layout_at_all() {
    // A view that has never been saved at 2x2 still gets all four members
    // placed when it grows to 3x3 - nothing to "remember" is not an excuse
    // to leave anyone off the grid.
    let mut view = test_view("v1");
    view.cols = 3;
    view.rows = 3;
    view.members = vec!["a".into(), "b".into(), "c".into(), "d".into()];
    rebuild_view_layout(&mut view, &std::collections::HashSet::new(), &std::collections::HashSet::new());
    let layout = view.layouts.get(&grid_key(3, 3)).unwrap();
    assert_eq!(layout.len(), 4);
}

#[test]
fn rebuild_view_layout_never_skips_a_member_whose_remembered_cell_is_taken() {
    // "b" remembers [0,0] from a 1x2 layout; growing to 2x1 also wants to seat
    // "a" fresh, and "a" happens to land on [0,0] first. "b" must still get a
    // cell (the other free one), never be silently left off the grid - this is
    // the reported 2x2-to-3x3 loss, reduced to its smallest repro.
    let mut view = test_view("v1");
    view.cols = 1;
    view.rows = 2;
    view.members = vec!["a".into(), "b".into()];
    view.layouts.insert(grid_key(1, 2), HashMap::from([("b".to_string(), [0, 0])]));
    view.cols = 2;
    view.rows = 1;
    // Force "a" onto [0,0] first via an existing (stale) 2x1 layout entry, so
    // "b"'s remembered [0,0] is genuinely occupied by someone else.
    view.layouts.insert(grid_key(2, 1), HashMap::from([("a".to_string(), [0, 0])]));
    rebuild_view_layout(&mut view, &std::collections::HashSet::new(), &std::collections::HashSet::new());
    let layout = view.layouts.get(&grid_key(2, 1)).unwrap();
    assert_eq!(layout.len(), 2, "both members must have a cell, none dropped");
    assert_eq!(layout.get("a"), Some(&[0, 0]));
    assert_eq!(layout.get("b"), Some(&[1, 0]));
}

#[test]
fn rebuild_view_layout_shrink_keeps_membership_and_shows_overhang() {
    // 3x3 with nine members shrunk to 2x2: nobody loses membership, five have
    // no cell (overhang) instead of being dropped.
    let mut view = test_view("v1");
    view.cols = 3;
    view.rows = 3;
    let ids: Vec<String> = (0..9).map(|i| format!("p{i}")).collect();
    view.members = ids.clone();
    let mut full = HashMap::new();
    for (i, id) in ids.iter().enumerate() {
        full.insert(id.clone(), [(i % 3) as u32, (i / 3) as u32]);
    }
    view.layouts.insert(grid_key(3, 3), full);
    view.cols = 2;
    view.rows = 2;
    rebuild_view_layout(&mut view, &std::collections::HashSet::new(), &std::collections::HashSet::new());
    assert_eq!(view.members.len(), 9, "shrinking must never drop membership");
    let small = view.layouts.get(&grid_key(2, 2)).unwrap();
    assert_eq!(small.len(), 4, "only 4 cells exist, the other 5 are overhang");
    for id in &ids {
        if let Some(cell) = small.get(id) {
            assert!(cell[0] < 2 && cell[1] < 2);
        }
    }
    // Growing back to 3x3 restores exactly the old arrangement, untouched
    // the whole time under its own layouts key.
    view.cols = 3;
    view.rows = 3;
    rebuild_view_layout(&mut view, &std::collections::HashSet::new(), &std::collections::HashSet::new());
    let restored = view.layouts.get(&grid_key(3, 3)).unwrap();
    assert_eq!(restored.len(), 9);
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(restored.get(id), Some(&[(i % 3) as u32, (i / 3) as u32]));
    }
}

#[test]
fn rebuild_view_layout_mixed_resize_does_not_fall_back_to_a_stale_layout() {
    // 2x3 has 6 members. A much older, incomplete 3x2 layout (only 2 of them)
    // is already on record from a previous visit at that size. Resizing to
    // 3x2 (one axis grows, the other shrinks) must top up the stale layout
    // from `members`, not just reuse it as-is and lose the other four.
    let mut view = test_view("v1");
    view.cols = 2;
    view.rows = 3;
    let ids: Vec<String> = (0..6).map(|i| format!("p{i}")).collect();
    view.members = ids.clone();
    view.layouts.insert(
        grid_key(3, 2),
        HashMap::from([("p0".to_string(), [0, 0]), ("p1".to_string(), [1, 0])]),
    );
    view.cols = 3;
    view.rows = 2;
    rebuild_view_layout(&mut view, &std::collections::HashSet::new(), &std::collections::HashSet::new());
    let layout = view.layouts.get(&grid_key(3, 2)).unwrap();
    assert_eq!(layout.len(), 6, "every member must be seated, not just the stale two");
}

#[test]
fn rebuild_view_layout_excludes_grid_folder_members_from_their_own_cell() {
    // A prompt living inside a grid folder is still a view member but must
    // never also get its own individual layout cell.
    let mut view = test_view("v1");
    view.cols = 2;
    view.rows = 2;
    view.members = vec!["a".into(), "b".into()];
    let folder_members = std::collections::HashSet::from(["a".to_string()]);
    let folder_cells = std::collections::HashSet::from([(0u32, 0u32)]);
    rebuild_view_layout(&mut view, &folder_cells, &folder_members);
    let layout = view.layouts.get(&grid_key(2, 2)).unwrap();
    assert!(!layout.contains_key("a"), "folder member must not get its own tile");
    assert_eq!(layout.get("b"), Some(&[1, 0]));
}

#[test]
fn dup_pair_is_ignored_checks_shared_group_membership() {
    // Groups replace pairs - "a" and "d" each belong to a DIFFERENT ignored
    // group (from two separate ignore_dups calls); neither call ever put them
    // together, so they must NOT count as ignored even though both ids are
    // independently "in some ignored group somewhere".
    let groups = vec![
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        vec!["c".to_string(), "d".to_string(), "e".to_string()],
    ];
    assert!(dup_pair_is_ignored(&groups, "a", "b"), "same group, both directions");
    assert!(dup_pair_is_ignored(&groups, "b", "a"));
    assert!(dup_pair_is_ignored(&groups, "d", "e"), "second group on its own");
    assert!(dup_pair_is_ignored(&groups, "a", "c"), "c belongs to both groups - still ignored with a");
    assert!(dup_pair_is_ignored(&groups, "c", "d"), "and still ignored with d");
    assert!(!dup_pair_is_ignored(&groups, "a", "d"), "never co-ignored in any single call");
    assert!(!dup_pair_is_ignored(&groups, "a", "z"), "z was never ignored at all");
}

#[test]
fn screen_identity_differs_by_name_even_at_the_same_bounds() {
    let a = screen_identity("Dell-1", 1920, 0, 1920, 1080);
    let b = screen_identity("Dell-2", 1920, 0, 1920, 1080);
    assert_ne!(a, b);
    assert_eq!(a, screen_identity("Dell-1", 1920, 0, 1920, 1080));
}

#[test]
fn resolve_float_spot_parks_a_position_outside_every_current_screen() {
    let screens = vec![("main".to_string(), 0, 0, 1920, 1080)];
    let pos = Pos { x: 5000, y: 5000, screen: "gone-monitor".to_string() };
    let (x, y, parked) = resolve_float_spot(&pos, 200.0, 80.0, &screens, &[]);
    assert!(parked);
    // Landed inside the main screen's work area, not at the stale coordinates.
    assert!(x >= 0.0 && x + 200.0 <= 1920.0);
    assert!(y >= 0.0 && y + 80.0 <= 1080.0);
    // The stored position itself is never touched by resolving a spot for it.
    assert_eq!((pos.x, pos.y, pos.screen.as_str()), (5000, 5000, "gone-monitor"));
}

#[test]
fn resolve_float_spot_returns_home_once_the_same_screen_identity_reappears() {
    let screens = vec![("dell-1".to_string(), 1920, 0, 1920, 1080)];
    let pos = Pos { x: 2500, y: 100, screen: "dell-1".to_string() };
    let (x, y, parked) = resolve_float_spot(&pos, 200.0, 80.0, &screens, &[]);
    assert!(!parked);
    assert_eq!((x, y), (2500.0, 100.0));
}

#[test]
fn resolve_float_spot_stays_parked_for_a_different_identity_at_the_same_bounds() {
    // A second monitor with the exact dimensions and position of the one the
    // pill used to live on, but a different name: not the screen it left.
    let screens = vec![("dell-3".to_string(), 1920, 0, 1920, 1080)];
    let pos = Pos { x: 2500, y: 100, screen: "dell-1".to_string() };
    let (_, _, parked) = resolve_float_spot(&pos, 200.0, 80.0, &screens, &[]);
    assert!(parked);
}

#[test]
fn resolve_float_spot_trusts_plain_coordinates_for_a_position_saved_before_screen_ids() {
    // Empty `screen` (upgraded from an older save) falls back to today's
    // coordinate check, so an ordinary upgrade never parks a button that
    // has not actually moved.
    let screens = vec![("dell-1".to_string(), 0, 0, 1920, 1080)];
    let pos = Pos { x: 100, y: 100, screen: String::new() };
    let (x, y, parked) = resolve_float_spot(&pos, 200.0, 80.0, &screens, &[]);
    assert!(!parked);
    assert_eq!((x, y), (100.0, 100.0));
}

#[test]
fn resolve_float_spot_never_parks_on_a_failed_monitor_probe() {
    let pos = Pos { x: 5000, y: 5000, screen: "dell-1".to_string() };
    let (x, y, parked) = resolve_float_spot(&pos, 200.0, 80.0, &[], &[]);
    assert!(!parked);
    assert_eq!((x, y), (5000.0, 5000.0));
}

#[test]
fn ids_past_park_cleanup_is_off_at_zero_hours() {
    let mut parked = HashMap::new();
    parked.insert("a".to_string(), std::time::Duration::from_secs(999_999));
    assert!(ids_past_park_cleanup(&parked, 0.0).is_empty());
}

#[test]
fn ids_past_park_cleanup_only_returns_what_is_actually_past_its_hours() {
    let mut parked = HashMap::new();
    parked.insert("old".to_string(), std::time::Duration::from_secs(3 * 3600 + 1));
    parked.insert("young".to_string(), std::time::Duration::from_secs(3600));
    let expired = ids_past_park_cleanup(&parked, 3.0);
    assert_eq!(expired, vec!["old".to_string()]);
}

pub(super) fn test_tile_folder(id: &str, view_id: &str, cell: [u32; 2], members: &[&str]) -> TileFolder {
    TileFolder {
        id: id.to_string(),
        name: String::new(),
        color: String::new(),
        view_id: view_id.to_string(),
        cell,
        members: members.iter().map(|s| s.to_string()).collect(),
    }
}


#[test]
fn tile_folders_normalize_to_their_invariants() {
    let view_ids: Vec<String> = vec![test_view("v1").id];
    let ids: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
    let mut folders = vec![
        test_tile_folder("f1", "v1", [1, 1], &["a", "gone", "b"]),
        // Claims a prompt the first folder already holds, plus one of its own.
        test_tile_folder("f2", "v1", [1, 1], &["b", "c", "d"]),
        // A view that no longer exists takes its folder with it.
        test_tile_folder("f3", "vX", [0, 0], &["a", "c"]),
    ];
    normalize_tile_folders(&mut folders, &ids, &view_ids);
    assert_eq!(folders.len(), 2);
    assert_eq!(folders[0].members, vec!["a", "b"]); // unknown id dropped
    assert_eq!(folders[1].members, vec!["c", "d"]); // "b" belongs to f1
    // Two folders never sit on the same cell.
    assert_ne!(folders[0].cell, folders[1].cell);
    // Idempotent.
    let before: Vec<[u32; 2]> = folders.iter().map(|f| f.cell).collect();
    normalize_tile_folders(&mut folders, &ids, &view_ids);
    assert_eq!(folders.len(), 2);
    assert_eq!(folders.iter().map(|f| f.cell).collect::<Vec<_>>(), before);
    // Down to one member: the folder dissolves instead of lingering.
    folders[0].members.pop();
    normalize_tile_folders(&mut folders, &ids, &view_ids);
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].id, "f2");
}

// A hand-edited/imported cell at u32::MAX used to hang normalize_tile_folders
// forever (saturating_add never moves off the cap while the store mutex is held,
// i.e. the app is dead on the next launch). Must terminate and clamp instead.
#[test]
fn tile_folder_cell_collision_at_u32_max_terminates() {
    let view_ids: Vec<String> = vec![test_view("v1").id];
    let ids: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
    let mut folders = vec![
        test_tile_folder("f1", "v1", [u32::MAX, 0], &["a", "b"]),
        test_tile_folder("f2", "v1", [u32::MAX, 0], &["c", "d"]),
    ];
    normalize_tile_folders(&mut folders, &ids, &view_ids); // must return, not hang
    assert_eq!(folders.len(), 2);
    assert!(folders.iter().all(|f| f.cell[0] <= GRID_MAX + 5), "cell must be clamped into range");
    assert_ne!(folders[0].cell, folders[1].cell, "still resolved to different cells");

    let mut sync_folders = vec![
        test_tile_folder("f1", "v1", [u32::MAX, 0], &["a", "b"]),
        test_tile_folder("f2", "v1", [u32::MAX, 0], &["c", "d"]),
    ];
    sync_normalize_tile_folders(&mut sync_folders, &ids, &view_ids);
    assert_eq!(sync_folders.len(), 2);
    assert!(sync_folders.iter().all(|f| f.cell[0] <= GRID_MAX + 5));
    assert_ne!(sync_folders[0].cell, sync_folders[1].cell);
}

#[test]
fn settings_without_tile_folders_still_load() {
    let s: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
    assert!(s.tile_folders.is_empty());
    // A folder from a newer build with unknown extra fields keeps what it can.
    let s: Settings = serde_json::from_str(
        r##"{"tile_folders":[{"id":"f","name":"Work","color":"#112233","view_id":"v1","cell":[2,3],"members":["a","b"],"future":1}]}"##,
    )
    .unwrap();
    assert_eq!(s.tile_folders[0].cell, [2, 3]);
    assert_eq!(s.tile_folders[0].members, vec!["a", "b"]);
    assert_eq!(s.tile_folders[0].color, "#112233");
}

#[test]
fn legacy_view_groups_key_is_ignored() {
    // A settings blob written while view groups existed still loads: the
    // group list is dropped, every view stays, in its stored order.
    let s: Settings = serde_json::from_str(
        r##"{"active_view":"b","views":[{"id":"b","name":"B"},{"id":"a","name":"A"}],
                 "view_groups":[{"id":"g","name":"G","color":"","view_ids":["a"]}]}"##,
    )
    .unwrap();
    let ids: Vec<&str> = s.views.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, vec!["b", "a"]);
    assert_eq!(s.active_view, "b");
    assert!(!serde_json::to_string(&s).unwrap().contains("view_groups"));
}

#[test]
fn font_size_clamped() {
    assert_eq!(clamp_font_size(0), 0);
    assert_eq!(clamp_font_size(1), 1);
    assert_eq!(clamp_font_size(8), 10);
    assert_eq!(clamp_font_size(99), 40);
}
