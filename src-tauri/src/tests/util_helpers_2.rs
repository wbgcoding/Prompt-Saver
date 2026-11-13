// Pure helpers, second part: statistics, text recognition, the backup vault, MP4 box
// walking, recording file names and the ffmpeg download guards.

use super::util_helpers::util_dir;
use super::*;

// ---------- statistics ----------

#[test]
fn stats_date_formats_calendar_days_in_utc() {
    assert_eq!(stats_date(0), "1970-01-01");
    assert_eq!(stats_date(86_399), "1970-01-01");
    assert_eq!(stats_date(86_400), "1970-01-02");
    assert_eq!(stats_date(951_782_400), "2000-02-29");
    assert_eq!(stats_date(951_868_800), "2000-03-01");
    assert_eq!(stats_date(1_709_164_799), "2024-02-28");
    assert_eq!(stats_date(1_709_164_800), "2024-02-29");
    assert_eq!(stats_date(1_700_000_000), "2023-11-14");
    assert_eq!(stats_date(4_102_444_800), "2100-01-01");
}

#[test]
fn prompt_stat_type_sorts_by_file_extension_then_image_then_text() {
    let mut p = Prompt::default();
    assert_eq!(prompt_stat_type(&p), "text");
    p.image = "data:image/png;base64,QQ==".to_string();
    assert_eq!(prompt_stat_type(&p), "image");
    for (path, want) in [
        ("C:\\v\\clip.MP4", "video"),
        ("clip.webm", "video"),
        ("clip.mkv", "video"),
        ("clip.mov", "video"),
        ("clip.avi", "video"),
        ("clip.m4v", "video"),
        ("doc.PDF", "pdf"),
        ("a.png", "image"),
        ("a.JPG", "image"),
        ("a.jpeg", "image"),
        ("a.gif", "image"),
        ("a.webp", "image"),
        ("a.bmp", "image"),
        ("a.tiff", "image"),
        ("a.tga", "image"),
        ("a.ico", "image"),
        ("notes.txt", "file"),
        ("noextension", "file"),
        ("archive.mp4.zip", "file"),
        ("movie.mp4.pdf", "pdf"),
    ] {
        let q = Prompt { file_path: path.to_string(), ..Prompt::default() };
        assert_eq!(prompt_stat_type(&q), want, "{path}");
    }
}

#[test]
fn prompt_stat_type_prefers_the_file_over_a_stored_image() {
    let p = Prompt { file_path: "x.pdf".to_string(), image: "data:image/png;base64,QQ==".to_string(), ..Prompt::default() };
    assert_eq!(prompt_stat_type(&p), "pdf");
    let q = Prompt { file_path: "x.dat".to_string(), image: "data:image/png;base64,QQ==".to_string(), ..Prompt::default() };
    assert_eq!(prompt_stat_type(&q), "file");
}

#[test]
fn uf_find_returns_the_root_and_halves_the_path() {
    let mut parent = vec![0usize, 0, 1, 2, 3];
    assert_eq!(uf_find(&mut parent, 4), 0);
    assert_eq!(parent, vec![0, 0, 0, 2, 2]);
    assert_eq!(uf_find(&mut parent, 0), 0);
    let mut alone = vec![0usize];
    assert_eq!(uf_find(&mut alone, 0), 0);
    let mut two_roots = vec![0usize, 1, 1, 0];
    assert_eq!(uf_find(&mut two_roots, 2), 1);
    assert_eq!(uf_find(&mut two_roots, 3), 0);
}

#[test]
fn stats_labels_fall_back_to_english_for_blank_entries_only() {
    let l = StatsTxtLabels::default().or_english();
    assert_eq!(l.prompts, "Prompts");
    assert_eq!(l.daily, "Daily copies");
    assert_eq!(l.tools, "Tool usage");
    assert_eq!(l.copies, "{n} copies, last used {date}");
    assert_eq!(l.uses, "{n} uses, last used {date}");
    assert_eq!(l.never, "never");

    let l: StatsTxtLabels = serde_json::from_str(r#"{"prompts":"Prompts auf Deutsch","never":"   "}"#).unwrap();
    let l = l.or_english();
    assert_eq!(l.prompts, "Prompts auf Deutsch");
    assert_eq!(l.never, "never", "a whitespace-only label counts as missing");
    assert_eq!(l.daily, "Daily copies");
}

#[test]
fn stats_text_export_uses_translated_labels_and_never_for_unused_rows() {
    let entries = [
        StatsExportEntry { name: "Alpha".to_string(), copies: 3, last_used: 1_700_000_000 },
        StatsExportEntry { name: "Beta".to_string(), copies: 0, last_used: 0 },
    ];
    let daily = [DayCount { ts: 1_700_000_000, count: 5 }];
    let labels = StatsTxtLabels {
        prompts: "Eintraege".to_string(),
        copies: "{n}x, zuletzt {date}".to_string(),
        never: "nie".to_string(),
        ..StatsTxtLabels::default()
    };
    let txt = stats_to_txt(&entries, &daily, &[], labels, 0);
    assert_eq!(
        txt,
        "Eintraege\n=========\nAlpha: 3x, zuletzt 2023-11-14\nBeta: 0x, zuletzt nie\n\nDaily copies\n============\n2023-11-14: 5\n"
    );
}

#[test]
fn stats_text_export_shifts_last_used_by_the_zone_offset_and_clamps_below_zero() {
    let entries = [StatsExportEntry { name: "A".to_string(), copies: 1, last_used: 1_700_000_000 }];
    // 2023-11-14 22:13:20 UTC is already the 15th at +3 hours.
    let east = stats_to_txt(&entries, &[], &[], StatsTxtLabels::default(), 3 * 3600);
    assert!(east.contains("A: 1 copies, last used 2023-11-15\n"), "{east}");
    let west = stats_to_txt(&entries, &[], &[], StatsTxtLabels::default(), -22 * 3600);
    assert!(west.contains("last used 2023-11-14\n"), "{west}");
    let far_west = stats_to_txt(&entries, &[], &[], StatsTxtLabels::default(), -23 * 3600);
    assert!(far_west.contains("last used 2023-11-13\n"), "{far_west}");
    let early = [StatsExportEntry { name: "E".to_string(), copies: 1, last_used: 10 }];
    let clamped = stats_to_txt(&early, &[], &[], StatsTxtLabels::default(), -1_000_000);
    assert!(clamped.contains("last used 1970-01-01\n"), "{clamped}");
}

// ---------- text recognition ----------

#[test]
fn ps_single_quote_doubles_embedded_quotes() {
    use crate::ocr::ps_single_quote;
    assert_eq!(ps_single_quote("abc"), "'abc'");
    assert_eq!(ps_single_quote(""), "''");
    assert_eq!(ps_single_quote("it's"), "'it''s'");
    assert_eq!(ps_single_quote("''"), "''''''");
    assert_eq!(ps_single_quote("C:\\Users\\M\u{fc}ller\\a b"), "'C:\\Users\\M\u{fc}ller\\a b'");
}

#[test]
fn ps_single_quote_keeps_an_injection_attempt_inside_one_string() {
    use crate::ocr::ps_single_quote;
    assert_eq!(ps_single_quote("x'; Remove-Item *; '"), "'x''; Remove-Item *; '''");
}

#[test]
fn caps_from_json_gives_an_empty_list_for_no_output() {
    use crate::ocr::{caps_from_json, CapKind};
    assert!(caps_from_json(CapKind::Ocr, "").unwrap().is_empty());
    assert!(caps_from_json(CapKind::Ocr, "  \r\n ").unwrap().is_empty());
    assert!(caps_from_json(CapKind::Speech, "null").unwrap().is_empty());
}

#[test]
fn caps_from_json_refuses_malformed_or_oddly_shaped_output() {
    use crate::ocr::{caps_from_json, CapKind};
    assert!(caps_from_json(CapKind::Ocr, "{not json").is_err());
    assert_eq!(caps_from_json(CapKind::Ocr, "42").err().as_deref(), Some("unexpected PowerShell output shape"));
    assert_eq!(caps_from_json(CapKind::Ocr, "\"text\"").err().as_deref(), Some("unexpected PowerShell output shape"));
}

#[test]
fn caps_from_json_maps_one_row_and_localizes_its_label() {
    use crate::ocr::{caps_from_json, CapKind};
    let json = r#"{"Name":"Language.OCR~~~en-US~0.0.1.0","State":4,"Version":"1.2","DownloadSize":0,"InstallSize":2048}"#;
    let caps = caps_from_json(CapKind::Ocr, json).unwrap();
    assert_eq!(caps.len(), 1);
    assert_eq!(caps[0].name, "Language.OCR~~~en-US~0.0.1.0");
    assert_eq!(caps[0].tag, "en-US");
    assert_eq!(caps[0].state, "Installed");
    assert_eq!(caps[0].version, "1.2");
    assert_eq!(caps[0].download_size, None, "a reported 0 means not reported");
    assert_eq!(caps[0].install_size, Some(2048));
    assert!(!caps[0].label.is_empty());
}

#[test]
fn caps_from_json_keeps_the_row_order_and_uses_the_kind_prefix() {
    use crate::ocr::{caps_from_json, CapKind};
    let json = r#"[{"Name":"Language.Speech~~~de-DE~0.0.1.0","State":"NotPresent"},{"Name":"Language.Speech~~~fr-FR~0.0.1.0","State":0},{"Name":""}]"#;
    let caps = caps_from_json(CapKind::Speech, json).unwrap();
    let tags: Vec<&str> = caps.iter().map(|c| c.tag.as_str()).collect();
    assert_eq!(tags, ["de-DE", "fr-FR"], "a row without a name is dropped");
    assert_eq!(caps[0].state, "NotPresent");
    assert_eq!(caps[1].state, "NotPresent");
    // The same names read as OCR capabilities carry no tag: the prefix belongs to the kind.
    let as_ocr = caps_from_json(CapKind::Ocr, json).unwrap();
    assert!(as_ocr.iter().all(|c| c.tag.is_empty()));
}

#[test]
fn a_capability_that_was_never_listed_is_not_known() {
    use crate::ocr::{is_known_capability, CapKind};
    assert!(!is_known_capability(CapKind::Ocr, "Language.OCR~~~zz-ZZ~0.0.1.0"));
    assert!(!is_known_capability(CapKind::Speech, ""));
}

#[test]
fn ocr_join_lines_treats_a_closing_quote_after_a_period_as_a_sentence_end() {
    let lines: Vec<String> = ["He said \"stop.\"", "then left"].iter().map(|s| s.to_string()).collect();
    assert_eq!(ocr_join_lines(&lines), "He said \"stop.\"\nthen left");
    let lines: Vec<String> = ["(done.)", "and more"].iter().map(|s| s.to_string()).collect();
    assert_eq!(ocr_join_lines(&lines), "(done.)\nand more");
}

#[test]
fn ocr_join_lines_joins_after_a_closing_bracket_that_ends_no_sentence() {
    let lines: Vec<String> = ["(see above)", "and below"].iter().map(|s| s.to_string()).collect();
    assert_eq!(ocr_join_lines(&lines), "(see above) and below");
    let lines: Vec<String> = ["\"\"", "abc"].iter().map(|s| s.to_string()).collect();
    assert_eq!(ocr_join_lines(&lines), "\"\" abc");
}

#[test]
fn ocr_join_lines_stops_at_every_sentence_ending_mark() {
    for end in ["wait...", "wait\u{2026}", "really?", "wow!", "end."] {
        let lines = vec![end.to_string(), "next".to_string()];
        assert_eq!(ocr_join_lines(&lines), format!("{end}\nnext"), "{end}");
    }
    // A colon, a comma and a bare word do not end a sentence.
    for open in ["list:", "first,", "word"] {
        let lines = vec![open.to_string(), "next".to_string()];
        assert_eq!(ocr_join_lines(&lines), format!("{open} next"), "{open}");
    }
}

#[test]
fn ocr_join_lines_only_joins_before_a_lowercase_start() {
    let j = |a: &str, b: &str| ocr_join_lines(&[a.to_string(), b.to_string()]);
    assert_eq!(j("no end", "Capital"), "no end\nCapital");
    assert_eq!(j("no end", "  lower"), "no end lower");
    assert_eq!(j("total", "5 items"), "total\n5 items", "a digit is not a lowercase letter");
    assert_eq!(j("no end", "-dash"), "no end\n-dash");
    assert_eq!(j("\u{43f}\u{440}\u{438}\u{432}\u{435}\u{442}", "\u{43c}\u{438}\u{440}"), "\u{43f}\u{440}\u{438}\u{432}\u{435}\u{442} \u{43c}\u{438}\u{440}");
    assert_eq!(j("\u{43f}\u{440}\u{438}\u{432}\u{435}\u{442}", "\u{41c}\u{438}\u{440}"), "\u{43f}\u{440}\u{438}\u{432}\u{435}\u{442}\n\u{41c}\u{438}\u{440}");
    assert_eq!(j("caf\u{e9}", "\u{e9}t\u{e9}"), "caf\u{e9} \u{e9}t\u{e9}");
}

#[test]
fn ocr_join_lines_keeps_scripts_without_case_on_separate_lines() {
    let lines = vec!["\u{65e5}\u{672c}\u{8a9e}".to_string(), "\u{30c6}\u{30b9}\u{30c8}".to_string()];
    assert_eq!(ocr_join_lines(&lines), "\u{65e5}\u{672c}\u{8a9e}\n\u{30c6}\u{30b9}\u{30c8}");
}

#[test]
fn ocr_join_lines_strips_line_end_characters_and_keeps_paragraph_breaks() {
    let lines = vec!["first\r\n".to_string(), "line two".to_string()];
    assert_eq!(ocr_join_lines(&lines), "first line two");
    let lines: Vec<String> = ["a", "   ", "b"].iter().map(|s| s.to_string()).collect();
    assert_eq!(ocr_join_lines(&lines), "a\n\nb");
    let lines: Vec<String> = ["a", ""].iter().map(|s| s.to_string()).collect();
    assert_eq!(ocr_join_lines(&lines), "a\n\n");
    let lines: Vec<String> = ["", "b"].iter().map(|s| s.to_string()).collect();
    assert_eq!(ocr_join_lines(&lines), "\n\nb");
    // A paragraph break resets the wrap: the line after it never joins backwards.
    let lines: Vec<String> = ["open", "", "lower"].iter().map(|s| s.to_string()).collect();
    assert_eq!(ocr_join_lines(&lines), "open\n\nlower");
}

#[test]
fn ocr_capability_state_covers_every_json_shape() {
    use serde_json::json;
    assert_eq!(ocr_capability_state_from_value(&json!("Installed")), "Installed");
    assert_eq!(ocr_capability_state_from_value(&json!("anything")), "anything");
    assert_eq!(ocr_capability_state_from_value(&json!(4)), "Installed");
    assert_eq!(ocr_capability_state_from_value(&json!(0)), "NotPresent");
    assert_eq!(ocr_capability_state_from_value(&json!(5)), "NotPresent");
    assert_eq!(ocr_capability_state_from_value(&json!(-1)), "NotPresent");
    assert_eq!(ocr_capability_state_from_value(&json!(4.0)), "NotPresent", "a float is not the integer code");
    assert_eq!(ocr_capability_state_from_value(&json!(null)), "");
    assert_eq!(ocr_capability_state_from_value(&json!(true)), "");
    assert_eq!(ocr_capability_state_from_value(&json!([4])), "");
}

// ---------- vault ----------

#[test]
fn deflate_and_inflate_round_trip_every_kind_of_payload() {
    let all_bytes: Vec<u8> = (0..=255u8).collect();
    let big: Vec<u8> = b"prompt text ".iter().cycle().take(1_000_000).copied().collect();
    for raw in [Vec::new(), b"hello".to_vec(), all_bytes, "\u{e4}\u{f6}\u{fc} \u{65e5}\u{672c}".as_bytes().to_vec(), big.clone()] {
        let packed = deflate(&raw).unwrap();
        assert_eq!(inflate(&packed).unwrap(), raw, "{} bytes", raw.len());
    }
    assert!(deflate(&big).unwrap().len() < 20_000, "repetitive text must shrink to a sliver");
}

#[test]
fn inflate_refuses_bytes_that_are_not_a_deflate_stream() {
    assert_eq!(inflate(&[0xFF; 16]).unwrap_err(), "corrupt backup");
    let mut packed = deflate(b"a reasonably long line of text so the stream has some body").unwrap();
    packed.truncate(packed.len() / 2);
    assert!(inflate(&packed).is_err(), "a cut stream is not a backup");
}

#[test]
fn inflate_capped_reads_a_plain_reader_in_full() {
    assert_eq!(inflate_capped(&b"plain"[..]).unwrap(), b"plain");
    assert_eq!(inflate_capped(&b""[..]).unwrap(), Vec::<u8>::new());
}

#[test]
fn inflate_capped_refuses_a_stream_past_the_ceiling() {
    let endless = std::io::repeat(0u8);
    let err = inflate_capped(endless).unwrap_err();
    assert_eq!(err, "backup expands to more than 512 MB");
}

#[test]
fn inflate_capped_reports_a_failing_reader_as_corrupt() {
    struct Broken;
    impl std::io::Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk gone"))
        }
    }
    assert_eq!(inflate_capped(Broken).unwrap_err(), "corrupt backup");
}

#[test]
fn only_receiver_rows_are_denied_as_imported_values() {
    assert!(is_import_denied_value("remotePort"));
    assert!(is_import_denied_value("remote"));
    assert!(!is_import_denied_value("Remote"));
    assert!(!is_import_denied_value("myremote"));
    assert!(!is_import_denied_value("clipWatcher"), "flags have their own list");
    assert!(!is_import_denied_value(""));
    assert!(!is_import_denied_value("theme"));
}

#[test]
fn machine_text_keys_cover_secrets_folders_and_the_sealed_password() {
    for key in ["backupPw", "screenshotDir", "videoExportDir", "recordingDir", "syncDir", "remoteTlsKey", "remoteTlsCert", "remoteTlsCa", "remoteAnythingNew"] {
        assert!(is_machine_text_key(key), "{key}");
    }
    for key in ["", "theme", "BackupPw", "backupPw2", "screenshotDirs", "secretVaultSalt", "myremoteTlsKey", "customPrompt"] {
        assert!(!is_machine_text_key(key), "{key}");
    }
}

#[test]
fn only_the_two_vault_rows_are_vault_keys() {
    assert!(is_vault_key("secretVaultSalt"));
    assert!(is_vault_key("secretVaultVerify"));
    for key in ["", "secretVaultSalt ", "secretvaultsalt", "secretVault", "backupPw"] {
        assert!(!is_vault_key(key), "{key}");
    }
}

fn settings_with_texts(rows: &[(&str, &str)]) -> Settings {
    let mut s = Settings::default();
    for (k, v) in rows {
        s.ui_texts.insert(k.to_string(), v.to_string());
    }
    s
}

#[test]
fn vault_keys_differ_only_when_the_salt_or_verifier_differs() {
    let none = Settings::default();
    assert!(!vault_keys_differ(&none, &Settings::default()));
    let a = settings_with_texts(&[("secretVaultSalt", "s1"), ("secretVaultVerify", "v1")]);
    let same = settings_with_texts(&[("secretVaultSalt", "s1"), ("secretVaultVerify", "v1")]);
    assert!(!vault_keys_differ(&a, &same));
    assert!(vault_keys_differ(&a, &none), "a vault against no vault");
    assert!(vault_keys_differ(&none, &a), "and the other way round");
    let other_salt = settings_with_texts(&[("secretVaultSalt", "s2"), ("secretVaultVerify", "v1")]);
    assert!(vault_keys_differ(&a, &other_salt));
    let other_verify = settings_with_texts(&[("secretVaultSalt", "s1"), ("secretVaultVerify", "v2")]);
    assert!(vault_keys_differ(&a, &other_verify));
    let only_salt = settings_with_texts(&[("secretVaultSalt", "s1")]);
    assert!(vault_keys_differ(&a, &only_salt), "a missing verifier is a difference");
    let unrelated = settings_with_texts(&[("secretVaultSalt", "s1"), ("secretVaultVerify", "v1"), ("customText", "x")]);
    assert!(!vault_keys_differ(&a, &unrelated), "other text rows do not matter");
}

#[test]
fn settings_import_diff_counts_rows_that_would_change() {
    let mut current = Settings::default();
    current.ui_flags.insert("a".to_string(), true);
    current.ui_flags.insert("b".to_string(), false);
    current.ui_values.insert("v".to_string(), 1.5);
    current.ui_texts.insert("t".to_string(), "x".to_string());

    let mut same = vault::SettingsOnlyExport::default();
    same.ui_flags.insert("a".to_string(), true);
    same.ui_values.insert("v".to_string(), 1.5);
    same.ui_texts.insert("t".to_string(), "x".to_string());
    assert_eq!(settings_import_diff(&current, &same), (0, 3));

    let mut changed = vault::SettingsOnlyExport::default();
    changed.ui_flags.insert("a".to_string(), false);
    changed.ui_values.insert("v".to_string(), 2.0);
    changed.ui_texts.insert("t".to_string(), "y".to_string());
    assert_eq!(settings_import_diff(&current, &changed), (3, 3));

    let mut fresh = vault::SettingsOnlyExport::default();
    fresh.ui_flags.insert("new".to_string(), false);
    fresh.ui_values.insert("newv".to_string(), 0.0);
    fresh.ui_texts.insert("newt".to_string(), String::new());
    assert_eq!(settings_import_diff(&current, &fresh), (3, 3), "a row this machine lacks counts as a change");

    assert_eq!(settings_import_diff(&current, &vault::SettingsOnlyExport::default()), (0, 0));
    assert_eq!(settings_import_diff(&Settings::default(), &same), (3, 3));
}

// ---------- recording files ----------

// One MP4 box: 32-bit size, four-letter type, payload.
fn mp4_box(typ: &str, payload: &[u8]) -> Vec<u8> {
    let mut b = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    b.extend_from_slice(typ.as_bytes());
    b.extend_from_slice(payload);
    b
}

#[test]
fn mp4_child_boxes_lists_siblings_with_their_payload_ranges() {
    let mut data = mp4_box("ftyp", b"isom");
    data.extend(mp4_box("free", b""));
    data.extend(mp4_box("mdat", b"123456"));
    assert_eq!(
        mp4_child_boxes(&data),
        vec![("ftyp".to_string(), 8..12), ("free".to_string(), 20..20), ("mdat".to_string(), 28..34)]
    );
}

#[test]
fn mp4_child_boxes_handles_empty_and_short_input() {
    assert!(mp4_child_boxes(&[]).is_empty());
    assert!(mp4_child_boxes(&[0, 0, 0, 8, b'f', b'r', b'e']).is_empty(), "seven bytes cannot hold a header");
}

#[test]
fn mp4_child_boxes_stops_at_a_truncated_box() {
    let mut data = mp4_box("ftyp", b"isom");
    data.extend_from_slice(&100u32.to_be_bytes());
    data.extend_from_slice(b"mdat");
    data.extend_from_slice(b"short");
    assert_eq!(mp4_child_boxes(&data), vec![("ftyp".to_string(), 8..12)], "the box that runs past the end is dropped");
}

#[test]
fn mp4_child_boxes_stops_at_a_size_smaller_than_its_header() {
    let mut data = mp4_box("ftyp", b"isom");
    data.extend_from_slice(&4u32.to_be_bytes());
    data.extend_from_slice(b"bad!");
    data.extend(mp4_box("free", b""));
    assert_eq!(mp4_child_boxes(&data), vec![("ftyp".to_string(), 8..12)], "a walker must not loop on a size below 8");
}

#[test]
fn mp4_child_boxes_lets_a_zero_size_box_run_to_the_end() {
    let mut data = mp4_box("ftyp", b"isom");
    data.extend_from_slice(&0u32.to_be_bytes());
    data.extend_from_slice(b"mdat");
    data.extend_from_slice(b"rest of file");
    assert_eq!(mp4_child_boxes(&data), vec![("ftyp".to_string(), 8..12), ("mdat".to_string(), 20..32)]);
    // A zero-size box with nothing after its header is an empty payload.
    let bare = [0u8, 0, 0, 0, b'm', b'd', b'a', b't'];
    assert_eq!(mp4_child_boxes(&bare), vec![("mdat".to_string(), 8..8)]);
}

#[test]
fn mp4_child_boxes_reads_a_sixty_four_bit_size() {
    let mut data = vec![0u8, 0, 0, 1];
    data.extend_from_slice(b"wide");
    data.extend_from_slice(&24u64.to_be_bytes());
    data.extend_from_slice(b"12345678");
    assert_eq!(mp4_child_boxes(&data), vec![("wide".to_string(), 16..24)]);
}

#[test]
fn mp4_child_boxes_refuses_a_cut_or_undersized_wide_header() {
    // Only 12 bytes: the extended size field is incomplete.
    let mut cut = vec![0u8, 0, 0, 1];
    cut.extend_from_slice(b"wide");
    cut.extend_from_slice(&[0, 0, 0, 0]);
    assert!(mp4_child_boxes(&cut).is_empty());
    // A wide size below its own 16-byte header.
    let mut small = vec![0u8, 0, 0, 1];
    small.extend_from_slice(b"wide");
    small.extend_from_slice(&8u64.to_be_bytes());
    assert!(mp4_child_boxes(&small).is_empty());
    // A wide size far beyond the data.
    let mut huge = vec![0u8, 0, 0, 1];
    huge.extend_from_slice(b"wide");
    huge.extend_from_slice(&1_000_000u64.to_be_bytes());
    assert!(mp4_child_boxes(&huge).is_empty());
}

#[test]
#[ignore = "bug: mp4_child_boxes overflows when a later box declares a 64-bit size near u64::MAX"]
fn mp4_child_boxes_survives_a_wide_size_near_the_integer_limit() {
    let mut data = mp4_box("ftyp", b"isom");
    data.extend_from_slice(&[0, 0, 0, 1]);
    data.extend_from_slice(b"wide");
    data.extend_from_slice(&u64::MAX.to_be_bytes());
    data.extend_from_slice(b"payload!");
    assert_eq!(mp4_child_boxes(&data), vec![("ftyp".to_string(), 8..12)]);
}

#[test]
fn mp4_child_boxes_names_a_non_ascii_type_lossily() {
    let mut data = 12u32.to_be_bytes().to_vec();
    data.extend_from_slice(&[0xA9, b'n', b'a', b'm']);
    data.extend_from_slice(b"abcd");
    assert_eq!(mp4_child_boxes(&data), vec![("\u{fffd}nam".to_string(), 8..12)]);
}

// ftyp (0..12), then moov (12..50) holding two trak boxes: the first (20..38) wraps an
// mdia box, the second (38..50) a bare payload.
fn two_track_file() -> Vec<u8> {
    let trak1 = mp4_box("trak", &mp4_box("mdia", b"AA"));
    let trak2 = mp4_box("trak", b"BBBB");
    let mut moov_payload = trak1;
    moov_payload.extend(trak2);
    let mut data = mp4_box("ftyp", b"isom");
    data.extend(mp4_box("moov", &moov_payload));
    data
}

#[test]
fn mp4_find_path_fans_out_over_siblings_with_absolute_offsets() {
    let data = two_track_file();
    assert_eq!(data.len(), 50);
    assert_eq!(mp4_find_path(&data, &["moov", "trak"]), vec![28..38, 46..50]);
    assert_eq!(mp4_find_path(&data, &["moov", "trak", "mdia"]), vec![36..38]);
    assert_eq!(mp4_find_path(&data, &["moov"]), vec![20..50]);
    assert_eq!(mp4_find_path(&data, &["ftyp"]), vec![8..12]);
}

#[test]
fn mp4_find_path_returns_nothing_for_a_path_that_is_not_there() {
    let data = two_track_file();
    assert!(mp4_find_path(&data, &["moov", "udta"]).is_empty());
    assert!(mp4_find_path(&data, &["trak"]).is_empty(), "trak is not a top level box");
    assert!(mp4_find_path(&data, &["moov", "trak", "mdia", "hdlr"]).is_empty());
    assert!(mp4_find_path(&[], &["moov"]).is_empty());
    assert_eq!(mp4_find_path(&data, &[]), vec![0..50], "an empty path is the whole file");
}

#[test]
fn mp4_find_path_does_not_descend_into_a_truncated_box() {
    let mut data = two_track_file();
    data.truncate(40);
    assert!(mp4_find_path(&data, &["moov", "trak"]).is_empty());
    assert_eq!(mp4_find_path(&data, &["ftyp"]), vec![8..12], "the intact box before the cut is still found");
}

#[test]
fn a_continuation_part_needs_the_stem_and_a_digit() {
    let d = util_dir("continuation");
    assert!(!has_continuation_part(&d, "take"));
    for name in ["take_partial.mp4", "other_part1.mp4", "take_part", "TAKE_part1.mp4", "take.mp4"] {
        fs::write(d.join(name), b"x").unwrap();
    }
    assert!(!has_continuation_part(&d, "take"), "none of these is a part of take");
    fs::write(d.join("take_part3.mp4"), b"x").unwrap();
    assert!(has_continuation_part(&d, "take"), "part 3 counts even with part 2 gone");
    assert!(!has_continuation_part(&d, "tak"));
    assert!(!has_continuation_part(&d.join("missing"), "take"), "an unreadable folder holds no part");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_recording_stem_is_taken_by_any_file_of_the_take() {
    let d = util_dir("stemtaken");
    assert!(!rec_stem_taken(&d, "rec-1"));
    for name in ["rec-1.mp4", "rec-1.webm", "rec-1.gif", "rec-1.avi", "rec-1.mic.wav", "rec-1.keys.json", "rec-1.cam.json", "rec-1_part2.mp4"] {
        let case = util_dir("stemtaken-case");
        fs::write(case.join(name), b"x").unwrap();
        assert!(rec_stem_taken(&case, "rec-1"), "{name}");
        let _ = fs::remove_dir_all(&case);
    }
    fs::write(d.join("rec-10.mp4"), b"x").unwrap();
    fs::write(d.join("rec-1.txt"), b"x").unwrap();
    fs::write(d.join("other.keys.json"), b"x").unwrap();
    assert!(!rec_stem_taken(&d, "rec-1"), "a longer stem, a text file and another take's sidecar do not count");
    assert!(!rec_stem_taken(&d.join("missing"), "rec-1"));
    let _ = fs::remove_dir_all(&d);
}

// ---------- ffmpeg download and export journal ----------

#[test]
fn parse_journal_line_splits_host_pid_and_path() {
    assert_eq!(parse_journal_line("PC\t4242\tC:\\x\\y.mp4"), Some(("PC", 4242, "C:\\x\\y.mp4")));
    assert_eq!(parse_journal_line("h\t1\ta\tb"), Some(("h", 1, "a\tb")), "tabs inside the path stay in the path");
    assert_eq!(parse_journal_line("\t1\tp"), Some(("", 1, "p")));
    assert_eq!(parse_journal_line("h\t1\t"), Some(("h", 1, "")));
    assert_eq!(parse_journal_line("h\t4294967295\tp"), Some(("h", u32::MAX, "p")));
}

#[test]
fn parse_journal_line_refuses_malformed_lines() {
    for line in ["", "host", "host\t12", "host\tabc\tp", "host\t-1\tp", "host\t4294967296\tp", "host\t 1\tp", "host\t\tp"] {
        assert_eq!(parse_journal_line(line), None, "{line:?}");
    }
}

#[test]
fn journal_line_writes_the_tab_format_and_cleans_the_host() {
    assert_eq!(journal_line("PC", 42, Path::new("C:\\x\\y")), "PC\t42\tC:\\x\\y");
    assert_eq!(journal_line("a\tb\r\nc", 7, Path::new("p")), "a b  c\t7\tp");
    let line = journal_line("host", 9, Path::new("D:\\out\\clip.mp4"));
    assert_eq!(parse_journal_line(&line), Some(("host", 9, "D:\\out\\clip.mp4")));
}

#[test]
fn the_journal_reads_back_what_was_written_and_skips_blank_lines() {
    let d = util_dir("journal");
    let j = d.join("export.journal");
    assert!(journal_lines(&j).is_empty(), "a missing journal is empty");
    fs::write(&j, "a\t1\tx\n\nb\t2\ty\n\n").unwrap();
    assert_eq!(journal_lines(&j), vec!["a\t1\tx".to_string(), "b\t2\ty".to_string()]);
    journal_write(&j, &["c\t3\tz".to_string()]);
    assert_eq!(fs::read_to_string(&j).unwrap(), "c\t3\tz\n");
    journal_write(&j, &[]);
    assert!(!j.exists(), "an empty list removes the journal");
    journal_write(&j, &[]);
    let left: Vec<_> = fs::read_dir(&d).unwrap().flatten().collect();
    assert!(left.is_empty(), "no temp file stays behind: {left:?}");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn hex_decode_accepts_both_cases_and_refuses_everything_else() {
    assert_eq!(hex_decode(""), Some(Vec::new()));
    assert_eq!(hex_decode("00ff"), Some(vec![0, 255]));
    assert_eq!(hex_decode("00FF"), Some(vec![0, 255]));
    assert_eq!(hex_decode("aBcD"), Some(vec![0xAB, 0xCD]));
    for bad in ["abc", "zz", "+1", "0x", " 00", "00 ", "\u{e9}1", "\u{e9}\u{e9}"] {
        assert_eq!(hex_decode(bad), None, "{bad:?}");
    }
}

#[test]
fn ct_eq_compares_whole_slices() {
    assert!(ct_eq(b"", b""));
    assert!(!ct_eq(b"", b"a"));
    assert!(!ct_eq(b"ab", b"a"));
    assert!(!ct_eq(&[0, 0, 0], &[0, 0, 1]));
    assert!(!ct_eq(&[1, 0, 0], &[0, 0, 0]));
    assert!(ct_eq(&[0xFF; 32], &[0xFF; 32]));
}

// SHA-256 of the four bytes "PK\x03\x04", the start of a zip local file header.
const PK_HASH: &str = "8dcc7e601606217f3b754766511182a916b17e9a26a94c9d887104eba92e9bb2";

#[test]
fn the_ffmpeg_zip_check_accepts_the_exact_hash_in_either_case() {
    let bytes = b"PK\x03\x04";
    assert!(verify_ffmpeg_zip_against(bytes, PK_HASH).is_ok());
    assert!(verify_ffmpeg_zip_against(bytes, &PK_HASH.to_uppercase()).is_ok());
}

#[test]
fn the_ffmpeg_zip_check_refuses_a_wrong_hash_of_any_shape() {
    let bytes = b"PK\x03\x04";
    let mismatch = "ffmpeg download hash mismatch, rejected";
    let mut off_by_one = PK_HASH.to_string();
    off_by_one.replace_range(63..64, "3");
    assert_eq!(verify_ffmpeg_zip_against(bytes, &off_by_one).unwrap_err(), mismatch);
    assert_eq!(verify_ffmpeg_zip_against(bytes, &PK_HASH[..62]).unwrap_err(), mismatch, "a shortened hash never matches");
    assert_eq!(verify_ffmpeg_zip_against(bytes, "").unwrap_err(), mismatch);
    assert_eq!(verify_ffmpeg_zip_against(bytes, "zz").unwrap_err(), "bad pinned hash");
    assert_eq!(verify_ffmpeg_zip_against(bytes, "abc").unwrap_err(), "bad pinned hash");
    assert_eq!(verify_ffmpeg_zip_against(b"PK\x03\x04more", PK_HASH).unwrap_err(), mismatch, "one extra byte changes the hash");
    assert_eq!(verify_ffmpeg_zip_against(bytes, &"0".repeat(64)).unwrap_err(), mismatch);
}

#[test]
fn the_ffmpeg_zip_check_rejects_a_non_archive_before_looking_at_the_hash() {
    let not_zip = "downloaded file is not a valid archive";
    assert_eq!(verify_ffmpeg_zip_against(b"", PK_HASH).unwrap_err(), not_zip);
    assert_eq!(verify_ffmpeg_zip_against(b"PK", PK_HASH).unwrap_err(), not_zip, "under four bytes");
    assert_eq!(verify_ffmpeg_zip_against(b"MZ\x90\x00", PK_HASH).unwrap_err(), not_zip);
    assert_eq!(verify_ffmpeg_zip_against(b"<html>404</html>", "zz").unwrap_err(), not_zip, "the format guard comes first");
}

#[test]
fn the_pinned_ffmpeg_hash_never_matches_a_tiny_archive() {
    assert!(verify_ffmpeg_zip(b"PK\x03\x04").is_err());
    assert!(verify_ffmpeg_zip(b"").is_err());
}
