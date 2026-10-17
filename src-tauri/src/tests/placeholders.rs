use super::*;

// ---- Placeholder spelling {?Name}: scanner, upgrade of the retired {#{Name}#} ----

#[test]
fn find_var_tokens_reads_the_short_spelling_and_ignores_look_alikes() {
    assert_eq!(find_var_tokens("a {?Größe} b"), vec![(2, 10, "Größe".to_string())]);
    assert_eq!(find_var_tokens("{?who  } and {?tone|a, b}"), vec![(0, 8, "who".to_string()), (13, 25, "tone|a, b".to_string())]);
    assert_eq!(find_var_tokens("{?@date}")[0].2, "@date");
    // Nothing that is not a name right after the brace and the question mark.
    for text in ["{?}", "{? x}", "{?>", "{?> x}", "{\"a\": 1}", "{x}", "{{x}}", "{?-x}", "{?x\ny}"] {
        assert!(find_var_tokens(text).is_empty(), "{text:?} must not be a placeholder");
    }
    // The old spelling is no placeholder any more.
    assert!(find_var_tokens("{#{name}#}").is_empty());
    // Body limit: 200 characters fit, 201 do not.
    assert_eq!(find_var_tokens(&format!("{{?{}}}", "a".repeat(200))).len(), 1);
    assert!(find_var_tokens(&format!("{{?{}}}", "a".repeat(201))).is_empty());
}

#[test]
fn a_self_filling_name_is_written_with_an_at_sign() {
    assert!(is_dyn_var_name("@date"));
    assert!(!is_dyn_var_name("@"));
    assert!(!is_dyn_var_name("date"));
    let none = HashMap::new();
    assert_eq!(resolve_static_vars("On {?@date}: {?who=you}", &none, &none), "On {?@date}: you");
}

#[test]
fn upgrade_placeholders_rewrites_every_old_kind() {
    assert_eq!(upgrade_placeholders("Hi {#{name}#}!"), "Hi {?name}!");
    assert_eq!(upgrade_placeholders("{#{city=Berlin}#}"), "{?city=Berlin}");
    assert_eq!(upgrade_placeholders("{#{tone|formal,casual}#}"), "{?tone|formal,casual}");
    assert_eq!(upgrade_placeholders("{#{tone=casual|formal,casual}#}"), "{?tone=casual|formal,casual}");
    assert_eq!(upgrade_placeholders("{#{  spaced  }#}"), "{?spaced}");
    // Self-filling: the slash fence and the bare pre-fence spelling both become @name.
    assert_eq!(upgrade_placeholders("On {#{/date\\}#} at {#{/time\\}#}"), "On {?@date} at {?@time}");
    assert_eq!(upgrade_placeholders("{#{date}#} {#{uuid}#}"), "{?@date} {?@uuid}");
    // A fenced name that is not a reserved one stays an ordinary variable, spelled safely.
    assert_eq!(upgrade_placeholders("{#{/foo\\}#}"), "{?_/foo\\}");
    // Unnamed token kept as typed; a body that cannot start a name gets an underscore.
    assert_eq!(upgrade_placeholders("a {#{ }#} b"), "a {#{ }#} b");
    assert_eq!(upgrade_placeholders("{#{-x}#}"), "{?_-x}");
    assert_eq!(upgrade_placeholders("{#{@foo}#}"), "{?@foo}");
    // Text without an old token is returned untouched, new tokens included.
    assert_eq!(upgrade_placeholders("plain {?name} text {x}"), "plain {?name} text {x}");
    assert_eq!(upgrade_placeholders("unclosed {#{name"), "unclosed {#{name");
}

#[test]
fn upgrade_placeholders_is_idempotent_and_leaves_no_old_token() {
    let src = "A {#{a}#} B {#{/date\\}#} C {#{ }#} D {#{b=1|x,y}#}";
    let once = upgrade_placeholders(src);
    assert_eq!(once, "A {?a} B {?@date} C {#{ }#} D {?b=1|x,y}");
    assert_eq!(upgrade_placeholders(&once), once);
    // Whatever the old scanner still sees afterwards is only the kept empty one.
    assert_eq!(find_old_var_tokens(&once).len(), 1);
    // Rewritten tokens are found by the new scanner.
    let names: Vec<String> = find_var_tokens(&once).into_iter().map(|t| t.2).collect();
    assert_eq!(names, vec!["a", "@date", "b=1|x,y"]);
}

#[test]
fn migrate_prompts_upgrades_the_text_and_every_rotation_entry() {
    let mut p = test_prompt("a");
    p.text = "Hi {#{name}#}".into();
    p.variants = vec!["Bye {#{name}#}".into(), "plain".into()];
    let mut list = vec![p];
    migrate_prompts(&mut list);
    assert_eq!(list[0].text, "Hi {?name}");
    assert_eq!(list[0].variants, vec!["Bye {?name}".to_string(), "plain".to_string()]);
    let snapshot = (list[0].text.clone(), list[0].variants.clone());
    migrate_prompts(&mut list);
    assert_eq!((list[0].text.clone(), list[0].variants.clone()), snapshot, "a second pass changes nothing");
}

#[test]
fn every_inbound_path_upgrades_the_old_spelling() {
    // Markdown sync: a file an older version wrote.
    let base = sync_test_prompt();
    let mut old = base.clone();
    old.text = "Dear {#{name}#}".into();
    let content = prompt_to_md(&old, &SyncPlacement::default());
    let updated = apply_md_to_prompt(&base, &content).expect("valid frontmatter");
    assert_eq!(updated.text, "Dear {?name}");
    // Import file: preview and import both read through the same parse.
    let data = parse_import_bytes("### Greeting\nHello {#{tone|a,b}#}".as_bytes(), std::path::Path::new("x.txt")).expect("parse");
    assert_eq!(data.prompts[0].text, "Hello {?tone|a,b}");
    // Trash: the entry comes back in the new spelling.
    let mut settings = Settings::default();
    settings.migrate();
    let mut store = Store { prompts: Vec::new(), trash: Vec::new(), settings, prompts_locked: false, written: Default::default() };
    let mut t = test_prompt("t");
    t.id = "t1".into();
    t.text = "Old {#{x}#}".into();
    t.variants = vec!["Old two {#{/date\\}#}".into()];
    t.trashed_at = 5;
    store.trash.push(t);
    restore_trashed_core(&mut store, "t1", |_| true).expect("restore");
    assert_eq!(store.prompts[0].text, "Old {?x}");
    assert_eq!(store.prompts[0].variants, vec!["Old two {?@date}".to_string()]);
    // A restored version snapshot, shown or restored as a new entry.
    let source = test_prompt("s");
    let restored = build_restored_prompt(&source, "Name", "Snap {#{y}#}".into(), vec!["V {#{y}#}".into()], "");
    assert_eq!(restored.text, "Snap {?y}");
    assert_eq!(restored.variants, vec!["V {?y}".to_string()]);
}

#[test]
fn a_confidential_entry_is_upgraded_where_it_is_read_back() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut p = test_prompt("a");
    p.secret = true;
    p.text = secret_encrypt("Hi {#{name}#}", &key);
    p.variants = vec![secret_encrypt("Two {#{/date\\}#}", &key)];
    let (text, variants) = reveal_secret_text(&p, Some(key)).expect("decrypt");
    assert_eq!(text, "Hi {?name}");
    assert_eq!(variants, vec!["Two {?@date}".to_string()]);
}

#[test]
fn the_stored_history_upgrade_runs_once_and_then_leaves_everything_alone() {
    assert_eq!(DATA_VERSION, 1);
    // A settings blob from before the field existed is at step 0.
    let mut v = serde_json::to_value(Settings::default()).unwrap();
    v.as_object_mut().unwrap().remove("data_version");
    let old: Settings = serde_json::from_value(v).unwrap();
    assert_eq!(old.data_version, 0);

    let mut conn = Connection::open_in_memory().unwrap();
    assert!(db_conn_init_schema(&conn));
    db_add_version(&conn, "p1", 10, "Name", "Old {#{a}#}", &["Entry {#{/date\\}#}".to_string()], 20).unwrap();
    db_add_version(&conn, "p1", 11, "Name", "Already {?b}", &[], 20).unwrap();
    let mut trash = vec![test_prompt("t1")];
    trash[0].text = "Trashed {#{c=1}#}".into();
    trash[0].variants = vec!["Two {#{c}#}".into()];

    // Two version rows are not both old: one rewritten row plus one trash entry.
    let changed = upgrade_history_placeholders(&mut conn, &mut trash).unwrap();
    assert_eq!(changed, 2);
    assert_eq!(trash[0].text, "Trashed {?c=1}");
    assert_eq!(trash[0].variants, vec!["Two {?c}".to_string()]);
    // The stored rows themselves hold the new spelling now, not only their display.
    let raw: Vec<(String, String)> = conn
        .prepare("SELECT text, variants FROM prompt_versions ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((dec_str(&r.get::<_, String>(0)?), dec_str(&r.get::<_, String>(1)?))))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(raw[0].0, "Old {?a}");
    assert_eq!(decode_variants(&raw[0].1), vec!["Entry {?@date}".to_string()]);
    assert_eq!(raw[1].0, "Already {?b}");
    // Listing a version shows the new spelling.
    assert_eq!(db_list_versions(&conn, "p1")[1].text, "Old {?a}");
    // A second start finds nothing to rewrite.
    assert_eq!(upgrade_history_placeholders(&mut conn, &mut trash).unwrap(), 0);
}

#[test]
fn an_old_snapshot_reads_in_the_new_spelling_before_the_upgrade_has_run() {
    let conn = Connection::open_in_memory().unwrap();
    assert!(db_conn_init_schema(&conn));
    db_add_version(&conn, "p1", 10, "Name", "Old {#{a}#}", &["E {#{b}#}".to_string()], 20).unwrap();
    let listed = db_list_versions(&conn, "p1");
    assert_eq!(listed[0].text, "Old {?a}");
    assert_eq!(listed[0].variants, vec!["E {?b}".to_string()]);
    let (_, _, text, variants) = db_get_version(&conn, listed[0].id).expect("version");
    assert_eq!(text, "Old {?a}");
    assert_eq!(variants, vec!["E {?b}".to_string()]);
}
