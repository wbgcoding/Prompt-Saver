// Pure parsing and text-format helpers: import and export, the Markdown sync files,
// the export filter graph, command-line argument checks and backup dates.
use super::*;

fn fmt_store() -> Store {
    Store { prompts: Vec::new(), trash: Vec::new(), settings: Settings::default(), prompts_locked: false, written: Default::default() }
}

fn fmt_scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{}-fmt-{tag}", brand::SLUG));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn fmt_touch(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "x").unwrap();
}

// ---- Import and export: spreadsheet guard ----

#[test]
fn the_formula_guard_covers_every_leading_formula_character() {
    for s in ["=1+1", "+1", "-1", "@x", "\tx", "\rx"] {
        assert!(starts_with_guarded_char(s), "{s:?} must be guarded");
    }
    for s in ["", "abc", " =1", "1=1", "'", "'abc", "\"=x"] {
        assert!(!starts_with_guarded_char(s), "{s:?} must not be guarded");
    }
}

#[test]
fn the_formula_guard_looks_past_leading_apostrophes() {
    assert!(starts_with_guarded_char("'=x"));
    assert!(starts_with_guarded_char("'''-x"));
    assert!(!starts_with_guarded_char("'''x"));
}

#[test]
fn csv_cell_prefixes_one_apostrophe_and_doubles_quotes() {
    assert_eq!(csv_cell("=SUM(A1)"), "\"'=SUM(A1)\"");
    assert_eq!(csv_cell("plain"), "\"plain\"");
    assert_eq!(csv_cell("say \"hi\""), "\"say \"\"hi\"\"\"");
    assert_eq!(csv_cell(""), "\"\"");
    assert_eq!(csv_cell("'-x"), "\"''-x\"");
}

#[test]
fn csv_row_joins_guarded_cells_with_semicolons() {
    assert_eq!(csv_row(&["a", "=b", "c;d"]), "\"a\";\"'=b\";\"c;d\"");
    assert_eq!(csv_row(&[]), "");
    assert_eq!(csv_row(&["only"]), "\"only\"");
}

#[test]
fn strip_formula_guard_removes_exactly_one_apostrophe_when_it_guards() {
    assert_eq!(strip_formula_guard("'=x".to_string()), "=x");
    assert_eq!(strip_formula_guard("''=x".to_string()), "'=x");
    assert_eq!(strip_formula_guard("'@mail".to_string()), "@mail");
    assert_eq!(strip_formula_guard("'\tx".to_string()), "\tx");
}

#[test]
fn strip_formula_guard_leaves_text_that_was_never_guarded() {
    assert_eq!(strip_formula_guard("'abc".to_string()), "'abc");
    assert_eq!(strip_formula_guard("'".to_string()), "'");
    assert_eq!(strip_formula_guard(String::new()), "");
    assert_eq!(strip_formula_guard("=x".to_string()), "=x");
    assert_eq!(strip_formula_guard("it's".to_string()), "it's");
}

#[test]
fn csv_cell_and_strip_formula_guard_round_trip_for_tricky_cells() {
    for s in ["=1", "'=1", "''-2", "+x", "@x", "'", "''", "a'=b", "\u{e9}=", "-", "'-", "\tindent", "plain"] {
        let cell = csv_cell(s);
        let rows = parse_csv(&cell);
        assert_eq!(rows, vec![vec![s.to_string()]], "{s:?} via {cell}");
    }
}

#[test]
fn parse_csv_reads_quotes_newlines_and_crlf() {
    let rows = parse_csv("\"a\";\"b\"\r\n\"c\"\"d\";\"e\nf\"\n");
    assert_eq!(rows, vec![vec!["a".to_string(), "b".to_string()], vec!["c\"d".to_string(), "e\nf".to_string()]]);
}

#[test]
fn parse_csv_keeps_a_last_row_without_a_trailing_newline() {
    assert_eq!(parse_csv("\"x\";\"y\""), vec![vec!["x".to_string(), "y".to_string()]]);
    assert_eq!(parse_csv("\"x\";"), vec![vec!["x".to_string(), String::new()]]);
}

#[test]
fn parse_csv_of_empty_input_has_no_rows() {
    assert!(parse_csv("").is_empty());
}

#[test]
fn parse_csv_keeps_a_semicolon_inside_quotes() {
    assert_eq!(parse_csv("\"a;b\";\"c\""), vec![vec!["a;b".to_string(), "c".to_string()]]);
}

// ---- Import and export: rotating-prompt marker ----

#[test]
fn variant_marker_lines_are_recognised_with_their_backslash_count() {
    assert_eq!(variant_marker_backslashes("<!-- next -->"), Some(0));
    assert_eq!(variant_marker_backslashes("<!-- \\next -->"), Some(1));
    assert_eq!(variant_marker_backslashes("<!-- \\\\\\next -->"), Some(3));
}

#[test]
fn variant_marker_lookalikes_are_not_markers() {
    for s in ["", "<!-- next -->x", " <!-- next -->", "<!--  next -->", "<!-- \\ next -->", "<!-- NEXT -->", "<!--next-->", "<!-- next -->\r"] {
        assert_eq!(variant_marker_backslashes(s), None, "{s:?}");
    }
}

#[test]
fn escaping_adds_one_backslash_to_each_marker_line_only() {
    assert_eq!(escape_variant_markers("<!-- next -->"), "<!-- \\next -->");
    assert_eq!(escape_variant_markers("a\n<!-- \\next -->\nb"), "a\n<!-- \\\\next -->\nb");
    assert_eq!(escape_variant_markers("keep <!-- next --> inline"), "keep <!-- next --> inline");
    assert_eq!(escape_variant_markers(""), "");
}

#[test]
fn adjacent_marker_lines_are_each_escaped() {
    assert_eq!(escape_variant_markers("<!-- next -->\n<!-- next -->"), "<!-- \\next -->\n<!-- \\next -->");
    assert_eq!(unescape_variant_markers("<!-- \\next -->\n<!-- \\next -->"), "<!-- next -->\n<!-- next -->");
}

#[test]
fn unescaping_never_touches_a_bare_marker_or_other_lines() {
    assert_eq!(unescape_variant_markers("<!-- next -->"), "<!-- next -->");
    assert_eq!(unescape_variant_markers("plain\n\u{fc}ber"), "plain\n\u{fc}ber");
    assert_eq!(unescape_variant_markers("<!-- \\\\next -->"), "<!-- \\next -->");
}

#[test]
fn variant_marker_escape_round_trips_for_any_backslash_count() {
    for n in 0..6 {
        let line = format!("<!-- {}next -->", "\\".repeat(n));
        assert_eq!(unescape_variant_markers(&escape_variant_markers(&line)), line, "{n} backslashes");
    }
    let mixed = "a\r\n<!-- next -->\r\n<!-- \\next -->\n\n<!-- next -->";
    assert_eq!(unescape_variant_markers(&escape_variant_markers(mixed)), mixed);
}

#[test]
fn variants_join_and_split_back_to_the_same_entries() {
    let cases: Vec<(&str, Vec<String>)> = vec![
        ("one", vec![]),
        ("one", vec!["two".to_string()]),
        ("a\nb", vec!["c".to_string(), "d\ne".to_string(), String::new()]),
        ("<!-- next -->", vec![]),
        ("<!-- next -->", vec!["<!-- next -->".to_string()]),
        ("x\n<!-- next -->\n<!-- next -->\ny", vec!["<!-- \\next -->".to_string()]),
        ("", vec![String::new()]),
        ("caf\u{e9} \u{1f600}", vec!["\u{4e2d}\u{6587}".to_string()]),
    ];
    for (text, variants) in cases {
        let joined = join_variants(text, &variants);
        assert_eq!(split_variants(&joined), (text.to_string(), variants.clone()), "{joined:?}");
    }
}

#[test]
fn join_variants_uses_the_html_comment_marker() {
    assert_eq!(join_variants("a", &["b".to_string(), "c".to_string()]), "a\n<!-- next -->\nb\n<!-- next -->\nc");
    assert_eq!(join_variants("a", &[]), "a");
}

#[test]
fn a_marker_at_the_very_start_or_end_is_plain_text_when_splitting() {
    assert_eq!(split_variants("<!-- next -->\nabc"), ("<!-- next -->\nabc".to_string(), vec![]));
    assert_eq!(split_variants("abc\n<!-- next -->"), ("abc\n<!-- next -->".to_string(), vec![]));
    assert_eq!(split_variants(""), (String::new(), vec![]));
}

#[test]
fn splitting_three_entries_gives_the_first_as_text() {
    let (text, rest) = split_variants("1\n<!-- next -->\n2\n<!-- next -->\n3");
    assert_eq!(text, "1");
    assert_eq!(rest, vec!["2".to_string(), "3".to_string()]);
}

// ---- Import and export: settings text, tags, sizes ----

#[test]
fn a_ui_text_map_reads_a_json_object_of_strings() {
    let mut s = Settings::default();
    s.ui_texts.insert("varFixed".to_string(), "{\"name\":\"Ann\",\"city\":\"Wien\"}".to_string());
    let map = parse_ui_text_map(&s, "varFixed");
    assert_eq!(map.len(), 2);
    assert_eq!(map.get("name").map(String::as_str), Some("Ann"));
    assert_eq!(map.get("city").map(String::as_str), Some("Wien"));
}

#[test]
fn a_ui_text_map_falls_back_to_empty_for_anything_unreadable() {
    let mut s = Settings::default();
    assert!(parse_ui_text_map(&s, "missing").is_empty());
    for bad in ["", "not json", "[1,2]", "{\"a\":1}", "{\"a\":", "null", "\"str\""] {
        s.ui_texts.insert("k".to_string(), bad.to_string());
        assert!(parse_ui_text_map(&s, "k").is_empty(), "{bad:?}");
    }
}

#[test]
fn a_ui_text_map_keeps_unicode_values() {
    let mut s = Settings::default();
    s.ui_texts.insert("k".to_string(), "{\"gru\u{df}\":\"\u{e4}\u{f6}\u{fc}\"}".to_string());
    assert_eq!(parse_ui_text_map(&s, "k").get("gru\u{df}").map(String::as_str), Some("\u{e4}\u{f6}\u{fc}"));
}

#[test]
fn split_tag_list_trims_and_drops_empty_parts() {
    assert_eq!(split_tag_list("a,b , c"), vec!["a", "b", "c"]);
    assert_eq!(split_tag_list(",, ,"), Vec::<String>::new());
    assert_eq!(split_tag_list(""), Vec::<String>::new());
    assert_eq!(split_tag_list("  solo  "), vec!["solo"]);
    assert_eq!(split_tag_list("one two, \u{fc}ber"), vec!["one two", "\u{fc}ber"]);
}

#[test]
fn split_tag_list_does_not_split_on_other_separators_or_dedupe() {
    assert_eq!(split_tag_list("a;b"), vec!["a;b"]);
    assert_eq!(split_tag_list("a,a"), vec!["a", "a"]);
}

#[test]
fn caption_and_font_sizes_clamp_to_the_allowed_range() {
    let cases = [(0, 0), (1, 1), (2, 10), (9, 10), (10, 10), (11, 11), (39, 39), (40, 40), (41, 40), (u32::MAX, 40)];
    for (input, expected) in cases {
        assert_eq!(clamp_caption_size(input), expected, "caption {input}");
        assert_eq!(clamp_font_size(input), expected, "font {input}");
    }
}

#[test]
fn settings_lines_fill_every_known_key() {
    let mut d = ImportData::default();
    parse_settings_lines(
        "language=de\ntheme=dark\ntile_font=Segoe UI\ntile_size=18\nminimize_to_tray=1\nauto_update=0\nshow_header=1\nshow_composer=0\n",
        &mut d,
    );
    assert_eq!(d.language.as_deref(), Some("de"));
    assert_eq!(d.theme.as_deref(), Some("dark"));
    assert_eq!(d.tile_font.as_deref(), Some("Segoe UI"));
    assert_eq!(d.tile_size, Some(18));
    assert_eq!(d.minimize_to_tray, Some(true));
    assert_eq!(d.auto_update, Some(false));
    assert_eq!(d.show_header, Some(true));
    assert_eq!(d.show_composer, Some(false));
}

#[test]
fn settings_lines_survive_crlf_padding_and_unknown_keys() {
    let mut d = ImportData::default();
    parse_settings_lines("  language= fr \r\nunknown=1\r\n\r\njust text\r\ntheme=light\r\n", &mut d);
    assert_eq!(d.language.as_deref(), Some("fr"));
    assert_eq!(d.theme.as_deref(), Some("light"));
    assert_eq!(d.tile_size, None);
    assert_eq!(d.minimize_to_tray, None);
}

#[test]
fn settings_lines_treat_only_one_as_true_and_reject_bad_numbers() {
    let mut d = ImportData::default();
    parse_settings_lines("auto_update=yes\nshow_header= 1 \nshow_composer=true\ntile_size=-5\n", &mut d);
    assert_eq!(d.auto_update, Some(false));
    assert_eq!(d.show_header, Some(true));
    assert_eq!(d.show_composer, Some(false));
    assert_eq!(d.tile_size, None);
    let mut e = ImportData::default();
    parse_settings_lines("tile_size=abc\ntile_size=\nlanguage=\n", &mut e);
    assert_eq!(e.tile_size, None);
    assert_eq!(e.language.as_deref(), Some(""));
}

#[test]
fn settings_lines_of_empty_input_change_nothing() {
    let mut d = ImportData::default();
    parse_settings_lines("", &mut d);
    assert!(d.language.is_none() && d.theme.is_none() && d.tile_size.is_none() && d.show_header.is_none());
}

// ---- Import and export: TXT blocks ----

#[test]
fn txt_lines_that_pass_for_metadata_or_the_rule_need_an_escape() {
    for s in ["@tags x", "@", "---", "\\@tags x", "\\\\---", "\\---"] {
        assert!(txt_line_needs_escape(s), "{s:?}");
    }
    for s in ["", "text", "----", "--- ", " ---", " @x", "\\", "\\text", "a@b", "--"] {
        assert!(!txt_line_needs_escape(s), "{s:?}");
    }
}

#[test]
fn txt_body_escape_adds_one_backslash_only_where_needed() {
    assert_eq!(txt_escape_body("a\n@tags x\n---\nb"), "a\n\\@tags x\n\\---\nb");
    assert_eq!(txt_escape_body("\\@x\n\\text"), "\\\\@x\n\\text");
    assert_eq!(txt_escape_body(""), "");
    assert_eq!(txt_escape_body("plain\n\nlines"), "plain\n\nlines");
}

#[test]
fn txt_body_unescape_removes_one_backslash_only_where_it_guards() {
    assert_eq!(txt_unescape_body("a\n\\@tags x\n\\---\nb"), "a\n@tags x\n---\nb");
    assert_eq!(txt_unescape_body("\\text\n\\"), "\\text\n\\");
    assert_eq!(txt_unescape_body("@raw"), "@raw");
    assert_eq!(txt_unescape_body(""), "");
}

#[test]
fn txt_body_escape_round_trips_for_nasty_lines() {
    let bodies = [
        "@tags a",
        "---",
        "\\---\n\\\\@x\n@y\n---",
        "line\n\n---\n\nline",
        "\\\n\\\\\n\\@",
        "a\r\n@b\r\n---\r\n",
        "\u{e9}\n@\u{fc}",
    ];
    for body in bodies {
        assert_eq!(txt_unescape_body(&txt_escape_body(body)), body, "{body:?}");
    }
}

#[test]
fn a_txt_meta_run_is_split_off_the_end_of_a_body() {
    let (text, meta) = split_txt_meta("hello\nworld\n@tags a,b\n@notes some note");
    assert_eq!(text, "hello\nworld");
    assert_eq!(meta.get("tags").map(String::as_str), Some("a,b"));
    assert_eq!(meta.get("notes").map(String::as_str), Some("some note"));
    assert_eq!(meta.len(), 2);
}

#[test]
fn a_txt_meta_run_stops_at_an_unknown_key_or_a_plain_line() {
    let (text, meta) = split_txt_meta("t\n@bogus x\n@tags a");
    assert_eq!(text, "t\n@bogus x");
    assert_eq!(meta.len(), 1);
    let (text, meta) = split_txt_meta("t\n@tags a\nplain\n@color #fff");
    assert_eq!(text, "t\n@tags a\nplain");
    assert_eq!(meta.get("color").map(String::as_str), Some("#fff"));
}

#[test]
fn a_txt_meta_key_counts_once_and_the_last_line_wins() {
    let (text, meta) = split_txt_meta("t\n@tags a\n@tags b");
    assert_eq!(text, "t\n@tags a");
    assert_eq!(meta.get("tags").map(String::as_str), Some("b"));
}

#[test]
fn a_txt_meta_line_needs_a_space_after_its_key() {
    let (text, meta) = split_txt_meta("t\n@tags");
    assert_eq!(text, "t\n@tags");
    assert!(meta.is_empty());
    let (text, meta) = split_txt_meta("t\n@tags ");
    assert_eq!(text, "t");
    assert_eq!(meta.get("tags").map(String::as_str), Some(""));
}

#[test]
fn a_txt_body_of_only_metadata_or_nothing_splits_cleanly() {
    let (text, meta) = split_txt_meta("@color #123456");
    assert_eq!(text, "");
    assert_eq!(meta.get("color").map(String::as_str), Some("#123456"));
    let (text, meta) = split_txt_meta("");
    assert_eq!(text, "");
    assert!(meta.is_empty());
}

#[test]
fn a_txt_meta_value_keeps_inner_spaces_and_unicode() {
    let (_, meta) = split_txt_meta("t\n@caption a  b \u{fc}ber");
    assert_eq!(meta.get("caption").map(String::as_str), Some("a  b \u{fc}ber"));
}

#[test]
fn every_documented_txt_meta_key_is_recognised() {
    for key in TXT_META_KEYS {
        let (text, meta) = split_txt_meta(&format!("t\n@{key} v"));
        assert_eq!(text, "t", "{key}");
        assert_eq!(meta.get(key).map(String::as_str), Some("v"), "{key}");
    }
    assert_eq!(TXT_META_KEYS.len(), 12);
}

#[test]
fn parse_txt_reads_crlf_blocks_and_unescapes_the_body() {
    let txt = "### One\r\n\\@tags not meta\r\n@tags a, b\r\n\r\n---\r\n\r\n### Two\r\nsecond";
    let data = parse_txt(txt);
    assert_eq!(data.prompts.len(), 2);
    assert_eq!(data.prompts[0].name, "One");
    assert_eq!(data.prompts[0].text, "@tags not meta");
    assert_eq!(data.prompts[0].tags, vec!["a", "b"]);
    assert_eq!(data.prompts[1].name, "Two");
    assert_eq!(data.prompts[1].text, "second");
}

#[test]
fn parse_txt_ignores_blocks_without_a_heading() {
    let data = parse_txt("stray text\n\n---\n\n### Real\nbody");
    assert_eq!(data.prompts.len(), 1);
    assert_eq!(data.prompts[0].name, "Real");
    assert!(parse_txt("").prompts.is_empty());
}

#[test]
fn parse_txt_reads_settings_and_view_blocks() {
    let data = parse_txt("@settings\nlanguage=de\ntile_size=20\n\n---\n\n@views\nWork\n\nPrivate\n\n---\n\n### P\nx");
    assert_eq!(data.language.as_deref(), Some("de"));
    assert_eq!(data.tile_size, Some(20));
    assert_eq!(data.view_defs, vec!["Work", "Private"]);
    assert_eq!(data.prompts.len(), 1);
}

#[test]
fn parse_txt_reads_style_flags_and_sizes_with_defaults_for_bad_values() {
    let data = parse_txt("### S\nbody\n@style Consolas 18\n@flags 1 0\n@capsize x\n@pdfpage 3");
    let p = &data.prompts[0];
    assert_eq!(p.font, "Consolas");
    assert_eq!(p.font_size, 18);
    assert!(p.show_image);
    assert!(!p.copy_image);
    assert_eq!(p.caption_size, 0);
    assert_eq!(p.pdf_page, 3);
    let data = parse_txt("### S\nbody\n@style - 12");
    assert_eq!(data.prompts[0].font, "");
    assert_eq!(data.prompts[0].font_size, 12);
}

// ---- Import and export: signature, byte-order mark, files ----

#[test]
fn the_import_signature_joins_fields_with_a_control_separator() {
    assert_eq!(import_sig("n", "t", "f", "i", true, &[]), "n\u{1}t\u{1}f\u{1}i\u{1}true\u{1}");
    assert_eq!(import_sig("n", "t", "", "", false, &["a".to_string(), "b".to_string()]), "n\u{1}t\u{1}\u{1}\u{1}false\u{1}a\n<!-- next -->\nb");
}

#[test]
fn the_import_signature_tells_prompts_apart_by_every_field() {
    let base = import_sig("n", "t", "f", "i", false, &["v".to_string()]);
    assert_ne!(base, import_sig("N", "t", "f", "i", false, &["v".to_string()]));
    assert_ne!(base, import_sig("n", "T", "f", "i", false, &["v".to_string()]));
    assert_ne!(base, import_sig("n", "t", "F", "i", false, &["v".to_string()]));
    assert_ne!(base, import_sig("n", "t", "f", "I", false, &["v".to_string()]));
    assert_ne!(base, import_sig("n", "t", "f", "i", true, &["v".to_string()]));
    assert_ne!(base, import_sig("n", "t", "f", "i", false, &[]));
    assert_eq!(base, import_sig("n", "t", "f", "i", false, &["v".to_string()]));
}

#[test]
fn a_name_and_text_that_only_move_a_boundary_get_different_signatures() {
    assert_ne!(import_sig("ab", "c", "", "", false, &[]), import_sig("a", "bc", "", "", false, &[]));
}

#[test]
fn strip_bom_removes_one_leading_mark_only() {
    assert_eq!(strip_bom("\u{feff}abc"), "abc");
    assert_eq!(strip_bom("\u{feff}\u{feff}a"), "\u{feff}a");
    assert_eq!(strip_bom("abc"), "abc");
    assert_eq!(strip_bom(""), "");
    assert_eq!(strip_bom("a\u{feff}b"), "a\u{feff}b");
    assert_eq!(strip_bom("\u{feff}"), "");
}

#[test]
fn an_import_file_with_a_byte_order_mark_parses_like_one_without() {
    let plain = "### A\nbody";
    let with_bom = format!("\u{feff}{plain}");
    let a = parse_import_bytes(plain.as_bytes(), Path::new("x.txt")).unwrap();
    let b = parse_import_bytes(with_bom.as_bytes(), Path::new("x.txt")).unwrap();
    assert_eq!(a.prompts[0].name, "A");
    assert_eq!(b.prompts[0].name, "A");
}

#[test]
fn an_import_file_that_is_not_utf8_is_refused_with_a_message() {
    let err = parse_import_bytes(&[0xff, 0xfe, 0x41, 0x00], Path::new("x.txt")).err().unwrap();
    assert_eq!(err, "the file is not valid UTF-8 text");
}

#[test]
fn an_import_file_without_prompts_says_so() {
    let err = parse_import_bytes(b"", Path::new("x.txt")).err().unwrap();
    assert_eq!(err, "no prompts found");
    let err = parse_import_bytes(b"just some text", Path::new("x.csv")).err().unwrap();
    assert_eq!(err, "no prompts found");
}

#[test]
fn the_import_file_type_comes_from_the_extension_in_any_case() {
    let csv = "\"name\";\"text\"\n\"A\";\"body\"\n";
    let data = parse_import_bytes(csv.as_bytes(), Path::new("LIST.CSV")).unwrap();
    assert_eq!(data.prompts[0].name, "A");
    assert_eq!(data.prompts[0].text, "body");
    let data = parse_import_bytes("### T\nx".as_bytes(), Path::new("noext")).unwrap();
    assert_eq!(data.prompts[0].name, "T");
}

#[test]
fn the_import_file_hash_is_the_lowercase_sha256_hex() {
    assert_eq!(import_file_hash(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(import_file_hash(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}

#[test]
fn import_folder_extensions_are_md_and_txt_in_any_case() {
    for p in ["a.md", "a.MD", "a.txt", "dir/a.Txt", "a.b.md"] {
        assert!(has_import_folder_ext(Path::new(p)), "{p}");
    }
    for p in ["a.json", "a", "a.", ".md", "a.md.bak", "a.markdown", ""] {
        assert!(!has_import_folder_ext(Path::new(p)), "{p}");
    }
}

#[test]
fn an_import_folder_file_must_exist_and_carry_a_note_extension() {
    let dir = fmt_scratch("folder-file");
    fmt_touch(&dir.join("a.md"));
    fmt_touch(&dir.join("b.json"));
    fs::create_dir_all(dir.join("c.md")).unwrap();
    assert!(is_import_folder_file(&dir.join("a.md")));
    assert!(!is_import_folder_file(&dir.join("b.json")));
    assert!(!is_import_folder_file(&dir.join("c.md")), "a folder named like a note is not a file");
    assert!(!is_import_folder_file(&dir.join("missing.md")));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn import_folder_rel_paths_use_forward_slashes_below_the_base() {
    let base = Path::new("C:\\notes");
    assert_eq!(import_folder_rel_path(base, Path::new("C:\\notes\\sub\\a.md")), "sub/a.md");
    assert_eq!(import_folder_rel_path(base, Path::new("C:\\notes\\a.md")), "a.md");
    assert_eq!(import_folder_rel_path(base, Path::new("D:\\other\\b.md")), "D:/other/b.md");
}

#[test]
fn import_folder_setting_defaults_and_bounds() {
    let mut store = fmt_store();
    assert_eq!(import_folder_depth_setting(&store), 3);
    assert_eq!(import_folder_max_setting(&store), 500);
    store.settings.ui_values.insert("importFolderDepth".to_string(), 0.0);
    store.settings.ui_values.insert("importFolderMax".to_string(), 0.0);
    assert_eq!(import_folder_depth_setting(&store), 1);
    assert_eq!(import_folder_max_setting(&store), 1);
    store.settings.ui_values.insert("importFolderDepth".to_string(), 99.0);
    store.settings.ui_values.insert("importFolderMax".to_string(), 12.0);
    assert_eq!(import_folder_depth_setting(&store), 5);
    assert_eq!(import_folder_max_setting(&store), 12);
    store.settings.ui_values.insert("importFolderDepth".to_string(), 2.0);
    assert_eq!(import_folder_depth_setting(&store), 2);
}

fn fmt_walk(dir: &Path, depth: u32, files: usize) -> Vec<String> {
    let mut out = Vec::new();
    walk_import_folder(dir, 0, depth, files, &mut out);
    out.iter().map(|p| import_folder_rel_path(dir, p)).collect()
}

fn fmt_walk_tree(tag: &str) -> PathBuf {
    let dir = fmt_scratch(tag);
    for f in ["a.md", "b.txt", "c.json", "sub/d.md", "sub/deep/e.md", "sub/deep/deeper/f.md"] {
        fmt_touch(&dir.join(f));
    }
    dir
}

#[test]
fn walking_an_import_folder_is_depth_first_and_alphabetical() {
    let dir = fmt_walk_tree("walk-order");
    assert_eq!(fmt_walk(&dir, 5, 100), vec!["a.md", "b.txt", "sub/d.md", "sub/deep/deeper/f.md", "sub/deep/e.md"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn walking_an_import_folder_leaves_out_folders_past_the_depth_limit() {
    let dir = fmt_walk_tree("walk-depth");
    assert_eq!(fmt_walk(&dir, 1, 100), vec!["a.md", "b.txt"]);
    assert_eq!(fmt_walk(&dir, 2, 100), vec!["a.md", "b.txt", "sub/d.md"]);
    assert_eq!(fmt_walk(&dir, 3, 100), vec!["a.md", "b.txt", "sub/d.md", "sub/deep/e.md"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn walking_an_import_folder_stops_at_the_file_limit() {
    let dir = fmt_walk_tree("walk-max");
    assert_eq!(fmt_walk(&dir, 5, 2), vec!["a.md", "b.txt"]);
    assert_eq!(fmt_walk(&dir, 5, 3), vec!["a.md", "b.txt", "sub/d.md"]);
    assert!(fmt_walk(&dir, 5, 0).is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn walking_a_missing_folder_finds_nothing() {
    let dir = std::env::temp_dir().join(format!("{}-fmt-no-such-folder", brand::SLUG));
    assert!(fmt_walk(&dir, 3, 10).is_empty());
}
