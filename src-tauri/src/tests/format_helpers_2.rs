// Pure helpers: Markdown sync files and lock text, the export filter graph, command-line
// argument checks, backup dates and retention settings.
use super::*;
use crate::cli::{reject_extra, split_tags, take_flag, take_flag_value, usage_err, valid_id, valid_name, valid_path_arg, valid_text, wants_help, Args, VALUE_FLAGS};
use std::time::Duration;

fn fmt2_scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{}-fmt2-{tag}", brand::SLUG));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn fields_of(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

// ---- Ids and names ----

#[test]
fn a_sync_id_may_hold_letters_digits_dash_and_underscore() {
    for id in ["a", "abc-DEF_09", "0", "_", "-", "p1"] {
        assert!(sync_safe_id(id), "{id:?} must be accepted");
    }
}

#[test]
fn a_sync_id_is_refused_when_empty_too_long_or_unsafe() {
    assert!(!sync_safe_id(""));
    assert!(sync_safe_id(&"a".repeat(64)));
    assert!(!sync_safe_id(&"a".repeat(65)));
    for id in ["a b", "a.b", "..", "a/b", "a\\b", "a:b", "caf\u{e9}", "a\n", "\u{1f600}", "a*"] {
        assert!(!sync_safe_id(id), "{id:?} must be refused");
    }
}

#[test]
fn a_sync_id_length_counts_bytes_so_multibyte_text_never_fits() {
    // 33 two-byte characters are 66 bytes, but they are refused for the character class anyway.
    assert!(!sync_safe_id(&"\u{e9}".repeat(33)));
}

#[test]
fn a_header_line_loses_its_line_breaks() {
    assert_eq!(md_escape_line("a\nb"), "a b");
    assert_eq!(md_escape_line("a\r\nb"), "a  b");
    assert_eq!(md_escape_line("\r"), " ");
    assert_eq!(md_escape_line(""), "");
    assert_eq!(md_escape_line("plain"), "plain");
}

#[test]
fn a_name_line_is_flattened_and_trimmed() {
    assert_eq!(md_name_line("  Work  "), "Work");
    assert_eq!(md_name_line("Work\n"), "Work");
    assert_eq!(md_name_line("\nWork"), "Work");
    assert_eq!(md_name_line("a\nb"), "a b");
    assert_eq!(md_name_line("   "), "");
    assert_eq!(md_name_line(""), "");
    assert_eq!(md_name_line("in  ner"), "in  ner");
}

#[test]
fn a_clean_name_matches_the_header_name_line() {
    assert_eq!(sync_clean_name(" Work \r\n"), "Work");
    assert_eq!(sync_clean_name("Work"), "Work");
}

// ---- Notes escaping ----

#[test]
fn a_single_line_note_is_written_unchanged() {
    assert_eq!(md_escape_notes(""), None);
    assert_eq!(md_escape_notes("plain"), None);
    assert_eq!(md_escape_notes("path C:\\temp\\new"), None);
}

#[test]
fn a_multi_line_note_is_escaped_with_backslashes_first() {
    assert_eq!(md_escape_notes("a\nb").as_deref(), Some("a\\nb"));
    assert_eq!(md_escape_notes("a\r\nb").as_deref(), Some("a\\r\\nb"));
    assert_eq!(md_escape_notes("\r").as_deref(), Some("\\r"));
    assert_eq!(md_escape_notes("a\\b\nc").as_deref(), Some("a\\\\b\\nc"));
    assert_eq!(md_escape_notes("\n\n").as_deref(), Some("\\n\\n"));
}

#[test]
fn a_note_escape_leaves_unicode_alone() {
    assert_eq!(md_escape_notes("caf\u{e9}\n\u{1f600}").as_deref(), Some("caf\u{e9}\\n\u{1f600}"));
}

#[test]
fn notes_unescape_reads_the_three_known_sequences() {
    assert_eq!(md_unescape_notes("a\\nb"), "a\nb");
    assert_eq!(md_unescape_notes("a\\r\\nb"), "a\r\nb");
    assert_eq!(md_unescape_notes("a\\\\b"), "a\\b");
    assert_eq!(md_unescape_notes(""), "");
}

#[test]
fn notes_unescape_keeps_an_escaped_backslash_before_n_as_two_characters() {
    // "\\n" in the file is a backslash followed by the letter n, not a line break.
    assert_eq!(md_unescape_notes("\\\\n"), "\\n");
}

#[test]
fn notes_unescape_keeps_unknown_sequences_and_a_trailing_backslash() {
    assert_eq!(md_unescape_notes("\\x"), "\\x");
    assert_eq!(md_unescape_notes("tail\\"), "tail\\");
    assert_eq!(md_unescape_notes("\\"), "\\");
    assert_eq!(md_unescape_notes("C:\\temp"), "C:\\temp");
}

#[test]
fn notes_survive_an_escape_and_unescape_round_trip() {
    for note in ["a\nb", "a\r\nb\r\n", "\\n\nreal", "back\\slash\nand more\\", "\n", "\u{e9}\n\u{1f600}\r", "\\\n\\\r\\"] {
        let escaped = md_escape_notes(note).expect("a line break is present");
        assert!(!escaped.contains(['\n', '\r']), "{note:?} still has a raw break");
        assert_eq!(md_unescape_notes(&escaped), note);
    }
}

// ---- Frontmatter ----

#[test]
fn frontmatter_needs_the_opening_line_to_be_exactly_three_dashes() {
    assert!(parse_frontmatter("").is_none());
    assert!(parse_frontmatter("---").is_none());
    assert!(parse_frontmatter("--- \nname: x\n---\n").is_none());
    assert!(parse_frontmatter("\n---\nname: x\n---\n").is_none());
    assert!(parse_frontmatter("----\nname: x\n---\n").is_none());
}

#[test]
fn frontmatter_splits_header_fields_from_the_body() {
    let (fields, body) = parse_frontmatter("---\nname: Hello\ncolor: red\n---\nline1\n\nline2\n").unwrap();
    assert_eq!(fields.get("name").map(String::as_str), Some("Hello"));
    assert_eq!(fields.get("color").map(String::as_str), Some("red"));
    assert_eq!(fields.len(), 2);
    assert_eq!(body, "line1\n\nline2\n");
}

#[test]
fn frontmatter_with_an_empty_body_gives_an_empty_body() {
    let (fields, body) = parse_frontmatter("---\nname: x\n---\n").unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(body, "");
    let (_, body) = parse_frontmatter("---\nname: x\n---").unwrap();
    assert_eq!(body, "");
}

#[test]
fn frontmatter_reads_crlf_files_like_lf_files() {
    let (fields, body) = parse_frontmatter("---\r\nname: x\r\ntags: a\r\n---\r\nbody\r\nmore").unwrap();
    assert_eq!(fields.get("name").map(String::as_str), Some("x"));
    assert_eq!(fields.get("tags").map(String::as_str), Some("a"));
    assert_eq!(body, "body\nmore");
}

#[test]
fn frontmatter_ignores_a_leading_byte_order_mark() {
    let (fields, body) = parse_frontmatter("\u{feff}---\nname: x\n---\nb").unwrap();
    assert_eq!(fields.get("name").map(String::as_str), Some("x"));
    assert_eq!(body, "b");
}

#[test]
fn frontmatter_value_keeps_colons_after_the_first() {
    let (fields, _) = parse_frontmatter("---\nname: a: b: c\n---\n").unwrap();
    assert_eq!(fields.get("name").map(String::as_str), Some("a: b: c"));
}

#[test]
fn frontmatter_trims_keys_and_ordinary_values() {
    let (fields, _) = parse_frontmatter("---\n  name  :   spaced   \ncolor:\n---\n").unwrap();
    assert_eq!(fields.get("name").map(String::as_str), Some("spaced"));
    assert_eq!(fields.get("color").map(String::as_str), Some(""));
}

#[test]
fn frontmatter_drops_only_the_one_space_after_the_notes_colon() {
    let (fields, _) = parse_frontmatter("---\nnotes:  two lead and trail  \n---\n").unwrap();
    assert_eq!(fields.get("notes").map(String::as_str), Some(" two lead and trail  "));
    let (fields, _) = parse_frontmatter("---\nnotes:\n---\n").unwrap();
    assert_eq!(fields.get("notes").map(String::as_str), Some(""));
    let (fields, _) = parse_frontmatter("---\nnotes:tight\n---\n").unwrap();
    assert_eq!(fields.get("notes").map(String::as_str), Some("tight"));
}

#[test]
fn frontmatter_skips_header_lines_without_a_colon() {
    let (fields, _) = parse_frontmatter("---\njust words\nname: x\n\n---\nb").unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields.get("name").map(String::as_str), Some("x"));
}

#[test]
fn frontmatter_later_duplicate_keys_win() {
    let (fields, _) = parse_frontmatter("---\nname: first\nname: second\n---\n").unwrap();
    assert_eq!(fields.get("name").map(String::as_str), Some("second"));
}

#[test]
fn frontmatter_body_may_contain_a_dashed_line_after_the_header_closed() {
    let (_, body) = parse_frontmatter("---\nname: x\n---\none\n---\ntwo").unwrap();
    assert_eq!(body, "one\n---\ntwo");
}

#[test]
fn frontmatter_with_a_closing_line_only_is_an_empty_header() {
    let (fields, body) = parse_frontmatter("---\n---\ntext").unwrap();
    assert!(fields.is_empty());
    assert_eq!(body, "text");
}

#[test]
fn a_file_is_ours_only_with_all_four_header_keys() {
    let all = fields_of(&[("name", "n"), ("color", "c"), ("tags", "t"), ("notes", "")]);
    assert!(md_is_ours(&all));
    for missing in ["name", "color", "tags", "notes"] {
        let mut f = all.clone();
        f.remove(missing);
        assert!(!md_is_ours(&f), "without {missing} it is somebody else's file");
    }
    assert!(!md_is_ours(&HashMap::new()));
}

// ---- Placement header ----

#[test]
fn view_grid_and_cell_are_read_from_the_header() {
    let f = fields_of(&[("view", "Work"), ("grid", "7x3"), ("cell", "2,1")]);
    assert_eq!(sync_parse_view_grid_cell(&f), ("Work", 7, 3, Some([2, 1])));
}

#[test]
fn a_missing_header_gives_empty_view_default_grid_and_no_cell() {
    let f = HashMap::new();
    assert_eq!(sync_parse_view_grid_cell(&f), ("", 5, 4, None));
}

#[test]
fn a_grid_with_spaces_around_the_numbers_still_parses() {
    let f = fields_of(&[("grid", " 3 x 2 ")]);
    let (_, cols, rows, _) = sync_parse_view_grid_cell(&f);
    assert_eq!((cols, rows), (3, 2));
}

#[test]
fn an_unparsable_grid_falls_back_to_the_default_size() {
    for bad in ["", "3", "3x", "x2", "axb", "3x2x1", "-3x2", "3,2", "3X2", "99999999999x2"] {
        let f = fields_of(&[("grid", bad)]);
        let (_, cols, rows, _) = sync_parse_view_grid_cell(&f);
        assert_eq!((cols, rows), (5, 4), "grid {bad:?} must fall back");
    }
}

#[test]
fn a_half_valid_grid_does_not_mix_with_the_default() {
    let f = fields_of(&[("grid", "3xbad")]);
    let (_, cols, rows, _) = sync_parse_view_grid_cell(&f);
    assert_eq!((cols, rows), (5, 4));
}

#[test]
fn a_cell_with_spaces_parses_and_a_bad_cell_is_none() {
    let f = fields_of(&[("cell", " 4 , 0 ")]);
    assert_eq!(sync_parse_view_grid_cell(&f).3, Some([4, 0]));
    for bad in ["", "1", "1,", ",1", "a,b", "-1,2", "1,2,3", "1;2", "1.5,2"] {
        let f = fields_of(&[("cell", bad)]);
        assert_eq!(sync_parse_view_grid_cell(&f).3, None, "cell {bad:?} must not be guessed");
    }
}

#[test]
fn a_bad_cell_does_not_disturb_a_good_grid() {
    let f = fields_of(&[("view", "V"), ("grid", "2x2"), ("cell", "oops")]);
    assert_eq!(sync_parse_view_grid_cell(&f), ("V", 2, 2, None));
}

// ---- Lock text ----

#[test]
fn a_lock_text_with_an_empty_computer_name_is_unreadable() {
    assert_eq!(sync_lock_holder("\n4242\n99\n"), SyncLockHolder::Unreadable);
    assert_eq!(sync_lock_holder("  \n4242\n99\n"), SyncLockHolder::Unreadable);
}

#[test]
fn a_lock_text_with_non_numeric_process_or_time_is_unreadable() {
    assert_eq!(sync_lock_holder("host\nabc\n99\n"), SyncLockHolder::Unreadable);
    assert_eq!(sync_lock_holder("host\n4242\nxyz\n"), SyncLockHolder::Unreadable);
    assert_eq!(sync_lock_holder("host\n-1\n99\n"), SyncLockHolder::Unreadable);
    assert_eq!(sync_lock_holder("host\n4242\n-5\n"), SyncLockHolder::Unreadable);
    assert_eq!(sync_lock_holder("only a name"), SyncLockHolder::Unreadable);
}

#[test]
fn a_lock_text_from_another_computer_is_elsewhere_even_with_crlf() {
    assert_eq!(sync_lock_holder("some-other-box-that-is-not-this\r\n4242\r\n99\r\n"), SyncLockHolder::Elsewhere);
}

#[test]
fn a_lock_text_matches_this_computer_without_regard_to_case() {
    let upper = local_host_name().to_uppercase();
    assert_eq!(sync_lock_holder(&format!("{upper}\n7\n8")), SyncLockHolder::Here { pid: 7, since_ns: 8 });
    let lower = local_host_name().to_lowercase();
    assert_eq!(sync_lock_holder(&format!(" {lower} \r\n 7 \r\n 8 \r\n")), SyncLockHolder::Here { pid: 7, since_ns: 8 });
}

#[test]
fn a_lock_text_takes_a_time_wider_than_thirty_two_bits() {
    let host = local_host_name();
    let big = u64::MAX as u128 + 5;
    assert_eq!(sync_lock_holder(&format!("{host}\n7\n{big}")), SyncLockHolder::Here { pid: 7, since_ns: big });
}

#[test]
fn a_lock_whose_file_is_gone_counts_as_stale() {
    let dir = fmt2_scratch("lock-gone");
    let lock = dir.join("missing.md.lock");
    assert!(sync_lock_text_is_stale(&lock, "other-box\n1\n2\n"));
    assert!(sync_lock_text_is_stale(&lock, ""));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_young_lock_with_unreadable_text_is_left_alone() {
    let dir = fmt2_scratch("lock-young-junk");
    let lock = dir.join("a.md.lock");
    fs::write(&lock, "junk").unwrap();
    assert!(!sync_lock_text_is_stale(&lock, "junk"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_old_lock_with_unreadable_text_is_stale() {
    let dir = fmt2_scratch("lock-old-junk");
    let lock = dir.join("a.md.lock");
    fs::write(&lock, "junk").unwrap();
    md_sync_2::backdate(&lock, Duration::from_secs(SYNC_LOCK_STALE_SECS + 30));
    assert!(sync_lock_text_is_stale(&lock, "junk"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_lock_of_another_computer_outlives_the_short_limit_but_not_the_long_one() {
    let dir = fmt2_scratch("lock-foreign");
    let lock = dir.join("a.md.lock");
    let text = "some-other-box-that-is-not-this\n1\n2\n";
    fs::write(&lock, text).unwrap();
    md_sync_2::backdate(&lock, Duration::from_secs(SYNC_LOCK_FOREIGN_STALE_SECS - 20));
    assert!(!sync_lock_text_is_stale(&lock, text));
    md_sync_2::backdate(&lock, Duration::from_secs(SYNC_LOCK_FOREIGN_STALE_SECS + 20));
    assert!(sync_lock_text_is_stale(&lock, text));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_lock_path_is_the_file_path_with_a_lock_suffix() {
    assert_eq!(sync_lock_path(Path::new("dir/a.md")), PathBuf::from("dir/a.md.lock"));
    assert_eq!(sync_lock_path(Path::new("a")), PathBuf::from("a.lock"));
}

// ---- Export filter graph ----

fn graph_chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

// Runs the token reader and reports the token with the index it stopped at.
fn token_of(s: &str, term: &[char]) -> (String, usize) {
    let c = graph_chars(s);
    let mut i = 0usize;
    let t = av_token(&c, &mut i, term);
    (t, i)
}

fn key_of(s: &str) -> (Option<String>, usize) {
    let c = graph_chars(s);
    let mut i = 0usize;
    let k = take_key(&c, &mut i);
    (k, i)
}

fn graph_names(graph: &str) -> Vec<String> {
    parse_graph(graph).unwrap().into_iter().map(|f| f.name).collect()
}

fn pos(v: &str) -> (Option<String>, String) {
    (None, v.to_string())
}

fn kv(k: &str, v: &str) -> (Option<String>, String) {
    (Some(k.to_string()), v.to_string())
}

#[test]
fn the_token_reader_skips_leading_blanks_and_trims_trailing_ones() {
    assert_eq!(token_of("  \t\n a b \r", &[':']).0, "a b");
    assert_eq!(token_of("", &[':']), (String::new(), 0));
    assert_eq!(token_of("   ", &[':']).0, "");
}

#[test]
fn the_token_reader_stops_at_an_unquoted_terminator() {
    assert_eq!(token_of("a:b", &[':']), ("a".to_string(), 1));
    assert_eq!(token_of("ab,cd", &[',', ';']), ("ab".to_string(), 2));
    assert_eq!(token_of(":x", &[':']), (String::new(), 0));
}

#[test]
fn the_token_reader_honours_backslash_escapes() {
    assert_eq!(token_of("a\\:b", &[':']), ("a:b".to_string(), 4));
    assert_eq!(token_of("a\\\\b", &[':']).0, "a\\b");
    assert_eq!(token_of("a\\ ", &[':']).0, "a ", "an escaped blank is not trimmed");
    assert_eq!(token_of("a\\'b", &[':']).0, "a'b");
}

#[test]
fn the_token_reader_keeps_a_lone_trailing_backslash() {
    assert_eq!(token_of("ab\\", &[':']).0, "ab\\");
    assert_eq!(token_of("\\", &[':']).0, "\\");
}

#[test]
fn the_token_reader_keeps_quoted_text_literally() {
    assert_eq!(token_of("'a:b'", &[':']), ("a:b".to_string(), 5));
    assert_eq!(token_of("'a b '", &[':']).0, "a b ", "quoted blanks are not trimmed");
    assert_eq!(token_of("a'b c'd", &[':']).0, "ab cd");
    assert_eq!(token_of("'a\\b'", &[':']).0, "a\\b", "a backslash inside quotes is literal");
    assert_eq!(token_of("''", &[':']).0, "");
}

#[test]
fn the_token_reader_takes_an_unterminated_quote_to_the_end() {
    assert_eq!(token_of("'abc", &[':']), ("abc".to_string(), 4));
    assert_eq!(token_of("'abc  ", &[':']).0, "abc", "without a closing quote the tail is trimmed");
}

#[test]
fn the_token_reader_handles_unicode_and_newlines_inside_the_token() {
    assert_eq!(token_of("caf\u{e9}\u{1f600}:x", &[':']).0, "caf\u{e9}\u{1f600}");
    assert_eq!(token_of("a\nb", &[':']).0, "a\nb");
}

#[test]
fn a_single_filter_graph_has_a_name_and_no_arguments() {
    let g = parse_graph("null").unwrap();
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].name, "null");
    assert_eq!(g[0].args, None);
}

#[test]
fn a_chain_is_split_at_commas_and_semicolons() {
    let g = parse_graph("scale=1280:720,fps=30;null").unwrap();
    assert_eq!(g.len(), 3);
    assert_eq!((g[0].name.as_str(), g[0].args.as_deref()), ("scale", Some("1280:720")));
    assert_eq!((g[1].name.as_str(), g[1].args.as_deref()), ("fps", Some("30")));
    assert_eq!((g[2].name.as_str(), g[2].args.as_deref()), ("null", None));
}

#[test]
fn link_labels_around_filters_are_skipped() {
    assert_eq!(graph_names("[0:v]trim=start=0:end=5,setpts=PTS-STARTPTS[v0];[v0]null[vout]"), ["trim", "setpts", "null"]);
    assert_eq!(graph_names("[a][b]concat=n=2[c]"), ["concat"]);
    assert_eq!(graph_names(" [a] \n null "), ["null"]);
}

#[test]
fn an_unterminated_link_label_is_an_error() {
    assert_eq!(parse_graph("[0:v").err().as_deref(), Some("unterminated link label"));
    assert_eq!(parse_graph("null[abc").err().as_deref(), Some("unterminated link label"));
    assert_eq!(parse_graph("[").err().as_deref(), Some("unterminated link label"));
}

#[test]
fn a_stray_closing_bracket_after_arguments_is_an_error() {
    assert_eq!(parse_graph("a=b]").err().as_deref(), Some("unexpected character in the graph"));
}

#[test]
fn an_empty_graph_is_one_nameless_filter() {
    let g = parse_graph("").unwrap();
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].name, "");
    assert_eq!(g[0].args, None);
    assert_eq!(graph_names("null,"), ["null", ""]);
}

#[test]
fn filter_arguments_are_unescaped_once() {
    let g = parse_graph("drawtext='a,b':x=1").unwrap();
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].args.as_deref(), Some("a,b:x=1"));
    let g = parse_graph("a=b\\,c").unwrap();
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].args.as_deref(), Some("b,c"));
}

#[test]
fn a_filter_name_keeps_its_instance_label_and_blanks_are_trimmed() {
    assert_eq!(graph_names("scale@s1=2:2"), ["scale@s1"]);
    assert_eq!(graph_names(" null , fps=1 "), ["null", "fps"]);
    let g = parse_graph(" fps = 1 ").unwrap();
    assert_eq!(g[0].name, "fps");
    assert_eq!(g[0].args.as_deref(), Some("1"));
}

#[test]
fn a_filter_with_an_empty_argument_string_has_empty_arguments() {
    let g = parse_graph("null=").unwrap();
    assert_eq!(g[0].args.as_deref(), Some(""));
}

#[test]
fn an_option_key_is_a_plain_run_ending_at_the_equals_sign() {
    assert_eq!(key_of("key=val"), (Some("key".to_string()), 4));
    assert_eq!(key_of(" key = val"), (Some("key".to_string()), 6));
    assert_eq!(key_of("a-b_c.d/e=1").0.as_deref(), Some("a-b_c.d/e"));
    assert_eq!(key_of("/file=x").0.as_deref(), Some("/file"));
    assert_eq!(key_of("a==").1, 2);
}

#[test]
fn a_token_that_is_not_a_key_leaves_the_position_alone() {
    for s in ["", "abc", "=1", " =1", "C\\:/x=1", "a b=1", "a:b=1", "a'b=1", "k\u{e9}y=1"] {
        assert_eq!(key_of(s), (None, 0), "{s:?} is a positional value");
    }
}

#[test]
fn filter_options_split_at_unescaped_colons() {
    assert!(parse_filter_args("").is_empty());
    assert_eq!(parse_filter_args("1280:720"), [pos("1280"), pos("720")]);
    assert_eq!(parse_filter_args("w=1:h=2"), [kv("w", "1"), kv("h", "2")]);
    assert_eq!(parse_filter_args("x:y=2"), [pos("x"), kv("y", "2")]);
    assert_eq!(parse_filter_args(" w = 5 "), [kv("w", "5")]);
}

#[test]
fn a_windows_path_with_an_escaped_drive_colon_is_one_positional_value() {
    assert_eq!(parse_filter_args("C\\:/clips/a=b.mp4"), [pos("C:/clips/a=b.mp4")]);
}

#[test]
fn quoted_and_escaped_option_values_may_hold_colons_and_equals_signs() {
    assert_eq!(parse_filter_args("text='hello: world':x=1"), [kv("text", "hello: world"), kv("x", "1")]);
    assert_eq!(parse_filter_args("t=a\\=b"), [kv("t", "a=b")]);
}

#[test]
fn empty_option_slots_are_kept_and_a_trailing_colon_adds_nothing() {
    assert_eq!(parse_filter_args("a="), [kv("a", "")]);
    assert_eq!(parse_filter_args("a:"), [pos("a")]);
    assert_eq!(parse_filter_args("a::b"), [pos("a"), pos(""), pos("b")]);
    assert_eq!(parse_filter_args(":"), [pos("")]);
}

#[test]
fn a_movie_source_is_one_path_with_an_optional_numeric_loop() {
    assert_eq!(movie_source_path(Some("a.mp4")).as_deref(), Some("a.mp4"));
    assert_eq!(movie_source_path(Some("filename=a.mp4")).as_deref(), Some("a.mp4"));
    assert_eq!(movie_source_path(Some("a.mp4:loop=2")).as_deref(), Some("a.mp4"));
    assert_eq!(movie_source_path(Some("loop=3:filename=a.mp4")).as_deref(), Some("a.mp4"));
}

#[test]
fn a_movie_source_with_any_other_shape_is_refused() {
    assert_eq!(movie_source_path(None), None);
    for bad in [
        "",
        "filename=",
        "loop=2",
        "a.mp4:loop=x",
        "a.mp4:loop=",
        "a.mp4:loop=-1",
        "a.mp4:format_name=lavfi",
        "a.mp4:b.mp4",
        "loop=0:a.mp4",
        "filename=a.mp4:filename=b.mp4",
        "a.mp4:filename=b.mp4",
        "streams=dv:a.mp4",
    ] {
        assert_eq!(movie_source_path(Some(bad)), None, "{bad:?}");
    }
}

#[test]
fn media_extensions_match_any_case_and_only_the_last_extension() {
    for p in ["a.PNG", "dir/b.Mp4", "c.jfif", "x.y.webm", "d.JXL"] {
        assert!(has_export_media_ext(Path::new(p)), "{p}");
    }
    for p in ["a.txt", "a", "a.tar.gz", "dir.png/file", "a.", ".png", "a.nfo", "a.asc", "a.png.txt", ""] {
        assert!(!has_export_media_ext(Path::new(p)), "{p:?}");
    }
}

#[test]
fn the_media_extension_list_is_lowercase_unique_and_has_no_text_formats() {
    let mut seen = std::collections::HashSet::new();
    for e in EXPORT_MEDIA_EXTS {
        assert_eq!(e, e.to_lowercase());
        assert!(!e.is_empty() && !e.contains('.'));
        assert!(seen.insert(e), "{e} is listed twice");
    }
    for text in ["txt", "nfo", "asc", "srt", "ass", "md", "json", "html"] {
        assert!(!EXPORT_MEDIA_EXTS.contains(&text), "{text} would let a text file become video");
    }
}

fn graph_refusal(graph: &str) -> String {
    validate_export_graph(graph, &[], &[]).expect_err(graph)
}

#[test]
fn the_filters_the_editor_builds_pass_without_any_file() {
    for g in [
        "null",
        "scale=1:1,fps=30",
        "scale@s1=2:2",
        "[0:v]trim=start=0:end=5,setpts=PTS-STARTPTS[v0]",
        "[0:a]atrim=0:5,asetpts=PTS-STARTPTS,volume=0.5[a0]",
        "drawtext=text=hi:x=1",
        "drawtext=a:b",
        "paletteuse=dither=bayer",
        "paletteuse=a:b:c:d:e",
    ] {
        assert_eq!(validate_export_graph(g, &[], &[]), Ok(()), "{g}");
    }
}

#[test]
fn an_unknown_or_oddly_cased_filter_is_refused_by_name() {
    assert_eq!(graph_refusal("nosuch"), "export graph not allowed: filter nosuch");
    assert_eq!(graph_refusal("Scale"), "export graph not allowed: filter Scale");
    assert_eq!(graph_refusal("scale=1:1,subtitles=a.srt"), "export graph not allowed: filter subtitles");
    assert_eq!(graph_refusal(""), "export graph not allowed: filter ");
    assert_eq!(graph_refusal("sendcmd=f=x"), "export graph not allowed: filter sendcmd");
}

#[test]
fn a_broken_graph_is_refused_with_the_parser_reason() {
    assert_eq!(graph_refusal("[0:v"), "export graph not allowed: unterminated link label");
    assert_eq!(graph_refusal("a=b]"), "export graph not allowed: unexpected character in the graph");
}

#[test]
fn file_loaded_options_are_refused_for_every_filter() {
    assert_eq!(graph_refusal("drawtext=/x=1"), "export graph not allowed: file-loaded option");
    assert_eq!(graph_refusal("scale=1:1:/w=2"), "export graph not allowed: file-loaded option");
    assert_eq!(graph_refusal("scale= /w=2"), "export graph not allowed: file-loaded option");
}

#[test]
fn a_text_file_option_is_refused_in_any_case_or_position() {
    assert_eq!(graph_refusal("drawtext=text=a:textfile=b"), "export graph not allowed: text file");
    assert_eq!(graph_refusal("drawtext=TextFile=b"), "export graph not allowed: text file");
    assert_eq!(graph_refusal("scale=textfile=b"), "export graph not allowed: text file");
    assert_eq!(graph_refusal("drawtext=a:b:c"), "export graph not allowed: text file");
}

#[test]
fn paletteuse_may_not_write_its_search_tree() {
    assert_eq!(graph_refusal("paletteuse=debug_kdtree=out"), "export graph not allowed: file-writing option");
    assert_eq!(graph_refusal("paletteuse= DEBUG_KDTREE =out"), "export graph not allowed: file-writing option");
    assert_eq!(graph_refusal("paletteuse=a:b:c:d:e:f"), "export graph not allowed: file-writing option");
}

#[test]
fn a_media_source_needs_a_file_the_request_names() {
    assert_eq!(graph_refusal("movie=x.png"), "export graph not allowed: media file not named by the request");
    assert_eq!(graph_refusal("amovie"), "export graph not allowed: media source options");
    assert_eq!(graph_refusal("movie=a.png:format_name=lavfi"), "export graph not allowed: media source options");
}

#[test]
fn a_network_looking_media_source_is_refused_unless_its_text_is_allowed() {
    let remote = "movie=//host/share/a.png";
    assert_eq!(graph_refusal(remote), "export graph not allowed: media file not named by the request");
    // Named by the request, but nothing canonical exists behind it, so it still does not pass.
    let named = ["//HOST/share/A.png".to_string()];
    assert!(validate_export_graph(remote, &[], &named).is_err());
}

fn graph_path(p: &Path) -> String {
    let c = fs::canonicalize(p).unwrap().to_string_lossy().to_string();
    c.trim_start_matches(r"\\?\").replace('\\', "/").replace(':', r"\:")
}

#[test]
fn a_named_media_file_passes_and_an_unnamed_one_does_not() {
    let dir = fmt2_scratch("graph-media");
    let named = dir.join("named.png");
    let other = dir.join("other.png");
    fs::write(&named, b"x").unwrap();
    fs::write(&other, b"x").unwrap();
    let allowed = vec![fs::canonicalize(&named).unwrap()];
    let ok = format!("movie='{}'[i0];[0:v][i0]overlay=1:2[vout]", graph_path(&named));
    assert_eq!(validate_export_graph(&ok, &allowed, &[]), Ok(()));
    let with_loop = format!("movie='{}':loop=2[i0]", graph_path(&named));
    assert_eq!(validate_export_graph(&with_loop, &allowed, &[]), Ok(()));
    let bad = format!("movie='{}'[i0]", graph_path(&other));
    assert_eq!(validate_export_graph(&bad, &allowed, &[]), Err("export graph not allowed: media file not named by the request".to_string()));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_request_may_carry_no_graph_at_all() {
    let dir = fmt2_scratch("graph-none");
    let src = dir.join("rec.mp4");
    fs::write(&src, b"x").unwrap();
    assert_eq!(validate_export_request_graphs(&src, &[], &[]), Ok(()));
    assert_eq!(validate_export_request_graphs(&src, &[], &["null", "scale=1:1"]), Ok(()));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_request_media_file_without_a_media_extension_is_refused() {
    let dir = fmt2_scratch("graph-ext");
    let src = dir.join("rec.mp4");
    let text = dir.join("notes.txt");
    fs::write(&src, b"x").unwrap();
    fs::write(&text, b"x").unwrap();
    let media = vec![text.to_string_lossy().to_string()];
    assert_eq!(
        validate_export_request_graphs(&src, &media, &["null"]),
        Err("export graph not allowed: media file type".to_string())
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_request_graph_may_read_the_source_itself() {
    let dir = fmt2_scratch("graph-src");
    let src = dir.join("rec.mp4");
    fs::write(&src, b"x").unwrap();
    let own = format!("movie='{}'[i0]", graph_path(&src));
    assert_eq!(validate_export_request_graphs(&src, &[], &[&own]), Ok(()));
    let elsewhere = dir.join("elsewhere.mp4");
    fs::write(&elsewhere, b"x").unwrap();
    let theirs = format!("movie='{}'[i0]", graph_path(&elsewhere));
    assert!(validate_export_request_graphs(&src, &[], &[&theirs]).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_named_media_file_that_is_a_folder_or_missing_is_not_allowed() {
    let dir = fmt2_scratch("graph-nofile");
    let src = dir.join("rec.mp4");
    fs::write(&src, b"x").unwrap();
    let folder = dir.join("folder.png");
    fs::create_dir_all(&folder).unwrap();
    let missing = dir.join("missing.png");
    for media in [&folder, &missing] {
        let graph = format!("movie='{}'[i0]", graph_path(&dir.join("rec.mp4")).replace("rec.mp4", &media.file_name().unwrap().to_string_lossy()));
        let r = validate_export_request_graphs(&src, &[media.to_string_lossy().to_string()], &[&graph]);
        assert!(r.is_err(), "{media:?} must not be readable by a graph");
    }
    let _ = fs::remove_dir_all(&dir);
}

// ---- Command-line argument checks ----

fn cli_args(items: &[&str]) -> Args {
    let owned: Vec<String> = items.iter().map(|s| s.to_string()).collect();
    Args::new(&owned)
}

const CLI_HINT: &str = "\nRun \"prompt-saver --help\" for the full command list.";

#[test]
fn a_usage_error_names_the_help_command() {
    assert_eq!(usage_err("bad thing"), format!("bad thing{CLI_HINT}"));
    assert_eq!(usage_err(String::from("x")), format!("x{CLI_HINT}"));
}

#[test]
fn an_id_is_ascii_letters_digits_dash_and_underscore() {
    for id in ["a", "A-b_9", "0", "_", "-"] {
        assert!(valid_id(id), "{id:?}");
    }
    assert!(valid_id(&"a".repeat(128)));
}

#[test]
fn an_id_that_is_empty_long_or_odd_is_refused() {
    assert!(!valid_id(""));
    assert!(!valid_id(&"a".repeat(129)));
    for id in ["a b", "a/b", "a\\b", "a.b", "caf\u{e9}", "a\0", "a\n", "id;rm", "--x=1", "\u{1f600}"] {
        assert!(!valid_id(id), "{id:?}");
    }
}

#[test]
fn a_name_may_not_be_empty_or_longer_than_five_hundred_characters() {
    assert_eq!(valid_name(""), Err("name is empty".to_string()));
    assert_eq!(valid_name("x"), Ok("x".to_string()));
    assert_eq!(valid_name("  "), Ok("  ".to_string()), "blanks are not trimmed here");
    assert_eq!(valid_name(&"a".repeat(500)), Ok("a".repeat(500)));
    assert_eq!(valid_name(&"a".repeat(501)), Err("name longer than 500 characters".to_string()));
}

#[test]
fn a_name_length_counts_characters_not_bytes() {
    let ok = "\u{e9}".repeat(500);
    assert_eq!(valid_name(&ok), Ok(ok.clone()));
    assert!(valid_name(&"\u{1f600}".repeat(501)).is_err());
}

#[test]
fn a_text_may_be_empty_but_not_longer_than_the_limit() {
    assert_eq!(valid_text(""), Ok(String::new()));
    assert_eq!(valid_text("line\r\nline"), Ok("line\r\nline".to_string()));
    assert_eq!(valid_text(&"a".repeat(500_000)).map(|s| s.len()), Ok(500_000));
    assert_eq!(valid_text(&"a".repeat(500_001)), Err("text longer than 500000 characters".to_string()));
}

#[test]
fn a_text_length_counts_characters_not_bytes() {
    assert!(valid_text(&"\u{e9}".repeat(500_000)).is_ok());
    assert!(valid_text(&"\u{e9}".repeat(500_001)).is_err());
}

#[test]
fn a_path_argument_is_refused_when_empty_nul_or_too_long() {
    let why = "path is empty, contains a NUL byte, or is too long".to_string();
    assert_eq!(valid_path_arg(""), Err(why.clone()));
    assert_eq!(valid_path_arg("a\0b"), Err(why.clone()));
    assert_eq!(valid_path_arg("\0"), Err(why.clone()));
    assert_eq!(valid_path_arg(&"a".repeat(32_001)), Err(why.clone()));
    assert_eq!(valid_path_arg(&"a".repeat(32_000)).map(|s| s.len()), Ok(32_000));
    assert_eq!(valid_path_arg("C:\\dir\\file name.txt"), Ok("C:\\dir\\file name.txt".to_string()));
}

#[test]
fn a_path_argument_length_counts_bytes() {
    // 16001 two-byte characters are 32002 bytes.
    assert!(valid_path_arg(&"\u{e9}".repeat(16_001)).is_err());
    assert!(valid_path_arg(&"\u{e9}".repeat(16_000)).is_ok());
}

#[test]
fn a_tag_list_is_split_at_commas_trimmed_and_stripped_of_empties() {
    assert_eq!(split_tags("a, b ,,c"), ["a", "b", "c"]);
    assert_eq!(split_tags(""), Vec::<String>::new());
    assert_eq!(split_tags(" , ,"), Vec::<String>::new());
    assert_eq!(split_tags("one"), ["one"]);
    assert_eq!(split_tags("a b, c"), ["a b", "c"]);
    assert_eq!(split_tags("a;b"), ["a;b"]);
    assert_eq!(split_tags("\u{e9}, \u{fc}\t,\n\u{4e2d}"), ["\u{e9}", "\u{fc}", "\u{4e2d}"]);
}

#[test]
fn a_tag_list_keeps_duplicates() {
    assert_eq!(split_tags("a,a, a"), ["a", "a", "a"]);
}

#[test]
fn the_cursor_hands_out_items_in_order() {
    let mut a = cli_args(&["x", "y"]);
    assert_eq!(a.peek(), Some("x"));
    assert_eq!(a.next().as_deref(), Some("x"));
    assert_eq!(a.next().as_deref(), Some("y"));
    assert_eq!(a.next(), None);
    assert_eq!(a.next(), None);
    assert_eq!(a.peek(), None);
    assert_eq!(a.pos, 2, "reading past the end does not move the cursor");
}

#[test]
fn an_empty_cursor_has_nothing() {
    let mut a = cli_args(&[]);
    assert_eq!(a.peek(), None);
    assert_eq!(a.next(), None);
    assert_eq!(a.next_positional(), None);
    assert_eq!(a.find_flag("--json"), None);
}

#[test]
fn a_positional_is_any_item_that_does_not_start_with_two_dashes() {
    let mut a = cli_args(&["x", "-y", "-", ""]);
    assert_eq!(a.next_positional().as_deref(), Some("x"));
    assert_eq!(a.next_positional().as_deref(), Some("-y"));
    assert_eq!(a.next_positional().as_deref(), Some("-"));
    assert_eq!(a.next_positional().as_deref(), Some(""));
    assert_eq!(a.next_positional(), None);
}

#[test]
fn a_flag_is_not_taken_as_a_positional_and_stays_in_place() {
    let mut a = cli_args(&["--json", "x"]);
    assert_eq!(a.next_positional(), None);
    assert_eq!(a.pos, 0);
    assert_eq!(a.next_positional(), None);
    assert_eq!(a.peek(), Some("--json"));
}

#[test]
fn a_bare_double_dash_makes_everything_after_it_a_value() {
    let mut a = cli_args(&["--", "--json", "--"]);
    assert_eq!(a.next_positional().as_deref(), Some("--json"));
    assert!(a.end_of_flags);
    assert_eq!(a.next_positional().as_deref(), Some("--"), "a second marker is just a value");
    assert_eq!(a.next_positional(), None);
}

#[test]
fn a_double_dash_with_nothing_after_it_gives_no_value() {
    let mut a = cli_args(&["--"]);
    assert_eq!(a.next_positional(), None);
    assert!(a.end_of_flags);
    assert_eq!(a.pos, 1);
}

#[test]
fn a_flag_is_found_at_its_own_index() {
    assert_eq!(cli_args(&["--json", "x"]).find_flag("--json"), Some(0));
    assert_eq!(cli_args(&["--favorite", "--json"]).find_flag("--json"), Some(1));
    assert_eq!(cli_args(&["x", "--json"]).find_flag("--json"), Some(1));
    assert_eq!(cli_args(&["x"]).find_flag("--json"), None);
}

#[test]
fn a_flag_search_starts_at_the_cursor() {
    let mut a = cli_args(&["--json", "get", "--json"]);
    a.next();
    assert_eq!(a.find_flag("--json"), Some(2));
}

#[test]
fn a_flag_search_steps_over_the_values_of_value_flags() {
    assert_eq!(cli_args(&["--name", "--json"]).find_flag("--json"), None);
    assert_eq!(cli_args(&["--text", "--name", "--name", "v"]).find_flag("--name"), Some(2));
    assert_eq!(cli_args(&["--tags", "a", "--json"]).find_flag("--json"), Some(2));
    assert_eq!(cli_args(&["--name"]).find_flag("--name"), Some(0));
    assert_eq!(cli_args(&["--name"]).find_flag("--json"), None);
}

#[test]
fn a_flag_search_stops_at_a_bare_double_dash() {
    assert_eq!(cli_args(&["a", "--", "--json"]).find_flag("--json"), None);
    let mut a = cli_args(&["--", "--json"]);
    a.next_positional();
    assert_eq!(a.find_flag("--json"), None, "after the marker nothing is a flag");
}

#[test]
fn every_value_flag_is_skipped_with_its_value() {
    for f in VALUE_FLAGS {
        assert_eq!(cli_args(&[*f, "--json", "--json"]).find_flag("--json"), Some(2), "{f}");
    }
}

#[test]
fn help_is_wanted_only_when_a_lone_help_flag_is_all_that_is_left() {
    assert!(wants_help(&cli_args(&["--help"])));
    assert!(wants_help(&cli_args(&["-h"])));
    assert!(!wants_help(&cli_args(&["--help", "x"])));
    assert!(!wants_help(&cli_args(&["x", "--help"])));
    assert!(!wants_help(&cli_args(&["--HELP"])));
    assert!(!wants_help(&cli_args(&[])));
    assert!(!wants_help(&cli_args(&["-h", "-h"])));
}

#[test]
fn help_is_judged_from_the_cursor_on() {
    let mut a = cli_args(&["get", "--help"]);
    assert!(!wants_help(&a));
    a.next();
    assert!(wants_help(&a));
    a.next();
    assert!(!wants_help(&a));
}

#[test]
fn taking_a_flag_removes_it_and_what_was_consumed() {
    let mut a = cli_args(&["head", "x", "--json", "y"]);
    a.next();
    assert!(take_flag(&mut a, "--json"));
    assert_eq!(a.items, ["x", "y"]);
    assert_eq!(a.pos, 0);
    assert!(!take_flag(&mut a, "--json"));
    assert_eq!(a.items, ["x", "y"]);
}

#[test]
fn taking_a_flag_that_is_not_there_changes_nothing() {
    let mut a = cli_args(&["a", "b"]);
    assert!(!take_flag(&mut a, "--json"));
    assert_eq!(a.items, ["a", "b"]);
}

#[test]
fn taking_a_flag_removes_only_the_first_occurrence() {
    let mut a = cli_args(&["--json", "--json"]);
    assert!(take_flag(&mut a, "--json"));
    assert_eq!(a.items, ["--json"]);
}

#[test]
fn taking_a_flag_value_removes_the_pair() {
    let mut a = cli_args(&["--tag", "t", "x"]);
    assert_eq!(take_flag_value(&mut a, "--tag"), Ok(Some("t".to_string())));
    assert_eq!(a.items, ["x"]);
    let mut a = cli_args(&["x", "--tag", "t"]);
    assert_eq!(take_flag_value(&mut a, "--tag"), Ok(Some("t".to_string())));
    assert_eq!(a.items, ["x"]);
}

#[test]
fn a_flag_value_may_itself_look_like_a_flag() {
    let mut a = cli_args(&["--tag", "--json", "x"]);
    assert_eq!(take_flag_value(&mut a, "--tag"), Ok(Some("--json".to_string())));
    assert_eq!(a.items, ["x"]);
    assert!(!take_flag(&mut cli_args(&["--text", "--json"]), "--json"));
}

#[test]
fn a_flag_without_its_value_is_a_usage_error() {
    let mut a = cli_args(&["x", "--tag"]);
    assert_eq!(take_flag_value(&mut a, "--tag"), Err(format!("--tag needs a value{CLI_HINT}")));
}

#[test]
fn a_missing_flag_value_is_none_and_a_flag_after_the_marker_is_ignored() {
    let mut a = cli_args(&["x"]);
    assert_eq!(take_flag_value(&mut a, "--tag"), Ok(None));
    let mut a = cli_args(&["--", "--tag", "t"]);
    assert_eq!(take_flag_value(&mut a, "--tag"), Ok(None));
    assert_eq!(a.items, ["--", "--tag", "t"]);
}

#[test]
fn leftover_arguments_are_rejected_by_name() {
    assert_eq!(reject_extra(&cli_args(&[])), Ok(()));
    assert_eq!(reject_extra(&cli_args(&["a"])), Err(format!("unexpected argument \"a\"{CLI_HINT}")));
    assert_eq!(reject_extra(&cli_args(&["--json"])), Err(format!("unexpected argument \"--json\"{CLI_HINT}")));
    assert_eq!(reject_extra(&cli_args(&["--", "x"])), Err(format!("unexpected argument \"--\"{CLI_HINT}")));
}

#[test]
fn a_lone_trailing_double_dash_is_not_an_extra_argument() {
    assert_eq!(reject_extra(&cli_args(&["--"])), Ok(()));
    let mut a = cli_args(&["x", "--"]);
    a.next();
    assert_eq!(reject_extra(&a), Ok(()));
}

#[test]
fn after_the_marker_was_read_any_leftover_is_extra() {
    let mut a = cli_args(&["--", "x", "y"]);
    assert_eq!(a.next_positional().as_deref(), Some("x"));
    assert_eq!(reject_extra(&a), Err(format!("unexpected argument \"y\"{CLI_HINT}")));
}
