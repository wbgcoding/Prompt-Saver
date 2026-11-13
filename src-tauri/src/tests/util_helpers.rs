// Pure helpers of the utility modules: logging, paths, settings model, update,
// colours, statistics, text recognition, vault, recording files and clipboard text.

use super::*;

// A fresh scratch folder below the sandboxed temp folder, named per test.
pub(super) fn util_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ps-util-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn set_age(path: &Path, age: Duration) {
    let f = fs::OpenOptions::new().write(true).open(path).unwrap();
    f.set_modified(SystemTime::now() - age).unwrap();
}

// ---------- logging ----------

#[test]
fn replace_case_insensitive_swaps_every_match_whatever_its_case() {
    assert_eq!(replace_case_insensitive("C:\\Temp\\a and c:\\TEMP\\b", "C:\\temp", "<t>"), "<t>\\a and <t>\\b");
    assert_eq!(replace_case_insensitive("none here", "zzz", "X"), "none here");
    assert_eq!(replace_case_insensitive("", "abc", "X"), "");
    assert_eq!(replace_case_insensitive("abc", "abc", ""), "");
}

#[test]
fn replace_case_insensitive_leaves_the_text_alone_for_an_empty_needle() {
    assert_eq!(replace_case_insensitive("some text", "", "X"), "some text");
    assert_eq!(replace_case_insensitive("", "", "X"), "");
}

#[test]
fn replace_case_insensitive_does_not_rescan_its_own_replacement() {
    // Adjacent matches are both taken, overlapping ones only from the left.
    assert_eq!(replace_case_insensitive("aaaa", "aa", "<>"), "<><>");
    assert_eq!(replace_case_insensitive("aaa", "aa", "<>"), "<>a");
    // A replacement that contains the needle is not replaced again.
    assert_eq!(replace_case_insensitive("ab", "ab", "abab"), "abab");
}

#[test]
fn replace_case_insensitive_keeps_multibyte_text_around_an_ascii_needle_intact() {
    assert_eq!(replace_case_insensitive("Ünï ABC ünï abc", "abc", "x"), "Ünï x ünï x");
    assert_eq!(replace_case_insensitive("日本語ABC日本語", "abc", "-"), "日本語-日本語");
    // Folding is ASCII only: a non-ASCII needle matches its exact spelling and nothing else.
    assert_eq!(replace_case_insensitive("Ü ü", "Ü", "X"), "X ü");
}

#[test]
fn looks_like_a_path_accepts_drive_unc_and_unix_roots_with_something_below() {
    for s in [r"D:\x", "D:/x", r"c:\Users\a", r"\\srv\share", r"\\srv", "/mnt/data", "/a/b/c", "/a/"] {
        assert!(looks_like_a_path(s), "{s:?}");
    }
}

#[test]
fn looks_like_a_path_rejects_bare_roots_and_plain_words() {
    for s in ["", "C:", r"C:\", "C:/", "/", "/etc", r"\\", "relative\\path", "1:\\x", "http://x/y", "name.txt", " /a/b"] {
        assert!(!looks_like_a_path(s), "{s:?}");
    }
}

#[test]
fn path_starts_at_needs_a_word_boundary_before_the_path() {
    let line = r"see D:\x\y";
    assert!(path_starts_at(line, 4));
    assert!(!path_starts_at(line, 5), "the colon alone is no path");
    assert!(path_starts_at(r"D:\x\y", 0));
    assert!(!path_starts_at(r"xD:\x\y", 1), "a letter before it means mid-word");
    assert!(!path_starts_at(r"9D:\x\y", 1), "a digit before it means mid-word");
    assert!(path_starts_at(r"(D:\x\y", 1), "punctuation is a boundary");
    assert!(path_starts_at(r"éD:\x\y", 2), "a non-ASCII letter does not count as a word character");
}

#[test]
fn shorten_stray_paths_cuts_a_quoted_path_to_its_file_name() {
    assert_eq!(shorten_stray_paths(r#"open "D:\Private Folder\x.mp4" failed"#), r#"open "x.mp4" failed"#);
    assert_eq!(shorten_stray_paths(r"open 'D:\a\b.txt' now"), r"open 'b.txt' now");
    assert_eq!(shorten_stray_paths(r#"open "D:\a\b\" now"#), r#"open "b" now"#);
    assert_eq!(shorten_stray_paths(r#"say "hello" now"#), r#"say "hello" now"#);
}

#[test]
fn shorten_stray_paths_cuts_an_unquoted_path_and_keeps_closing_punctuation() {
    assert_eq!(shorten_stray_paths(r"error at C:\a\b.txt."), "error at b.txt.");
    assert_eq!(shorten_stray_paths(r"(see C:\a\b.txt)"), "(see b.txt)");
    assert_eq!(shorten_stray_paths(r"Fehler: D:\Müll\datei ü.txt"), "Fehler: datei ü.txt");
    assert_eq!(shorten_stray_paths(r"share \\srv\share\dir\f.txt"), "share f.txt");
    assert_eq!(shorten_stray_paths("read /mnt/data/x.bin"), "read x.bin");
    assert_eq!(shorten_stray_paths(r"it's D:\a\b.txt"), "it's b.txt", "an unbalanced quote is no reason to skip the path");
}

#[test]
fn shorten_stray_paths_leaves_lines_without_a_path_alone() {
    for s in ["", "plain text", "50/50 done", "a:b", "version 1.2.3", "mixed 日本語 text"] {
        assert_eq!(shorten_stray_paths(s), s);
    }
}

#[test]
fn scrub_log_line_replaces_the_temp_folder_and_cuts_unknown_paths() {
    let temp_file = std::env::temp_dir().join("util-scrub-x.log");
    let line = format!("open {}", temp_file.display());
    assert_eq!(scrub_log_line(&line), "open <temp>\\util-scrub-x.log");
    assert_eq!(scrub_log_line(r"failed D:\Elsewhere\x.txt"), "failed x.txt");
    assert_eq!(scrub_log_line("nothing to hide"), "nothing to hide");
    assert_eq!(scrub_log_line(""), "");
}

#[test]
fn scrub_log_line_names_the_user_profile_with_a_placeholder() {
    let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) else { return };
    let line = format!("{}\\Documents\\a.txt broke", home.display());
    let out = scrub_log_line(&line);
    assert!(!out.contains(&*home.to_string_lossy()), "{out}");
    assert!(out.contains("a.txt"), "{out}");
}

#[test]
fn scrub_log_line_is_idempotent() {
    let temp_file = std::env::temp_dir().join("util-scrub-y.log");
    for line in [
        format!("open {}", temp_file.display()),
        r#"could not open "D:\Private Folder\x.mp4" for writing"#.to_string(),
        r"path=\\srv\share\a\b.txt: denied".to_string(),
        "no path at all".to_string(),
    ] {
        let once = scrub_log_line(&line);
        assert_eq!(scrub_log_line(&once), once, "{line}");
    }
}

#[test]
fn error_log_is_on_unless_the_flag_is_switched_off() {
    let mut s = Settings::default();
    assert!(error_log_enabled(&s));
    s.ui_flags.insert("errorLog".to_string(), false);
    assert!(!error_log_enabled(&s));
    s.ui_flags.insert("errorLog".to_string(), true);
    assert!(error_log_enabled(&s));
}

#[test]
fn a_missing_crash_lock_is_not_stale() {
    let dir = util_dir("lock-missing");
    assert!(!crash_lock_is_stale(&dir.join("none.lock")));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_fresh_crash_lock_owned_by_this_process_is_not_stale() {
    let dir = util_dir("lock-own");
    let lock = dir.join("a.log.lock");
    fs::write(&lock, std::process::id().to_string()).unwrap();
    assert!(!crash_lock_is_stale(&lock));
    // Whitespace around the id is ignored.
    fs::write(&lock, format!(" {}\r\n", std::process::id())).unwrap();
    assert!(!crash_lock_is_stale(&lock));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_crash_lock_whose_owner_is_gone_is_stale_at_once() {
    let dir = util_dir("lock-gone");
    let lock = dir.join("a.log.lock");
    // A process id no machine hands out.
    fs::write(&lock, "4294967000").unwrap();
    assert!(crash_lock_is_stale(&lock));
    // A child that already exited.
    let mut child = std::process::Command::new("cmd").args(["/c", "exit", "0"]).spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    fs::write(&lock, pid.to_string()).unwrap();
    assert!(crash_lock_is_stale(&lock));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_crash_lock_with_no_readable_id_waits_a_moment_for_its_creator() {
    let dir = util_dir("lock-garbage");
    let lock = dir.join("a.log.lock");
    for body in ["", "not a number", "-5", "12x"] {
        fs::write(&lock, body).unwrap();
        assert!(!crash_lock_is_stale(&lock), "fresh {body:?} may still be filled in");
        set_age(&lock, Duration::from_secs(5));
        assert!(crash_lock_is_stale(&lock), "old {body:?} is a leftover");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_crash_lock_older_than_ten_seconds_is_stale_even_with_a_live_owner() {
    let dir = util_dir("lock-old");
    let lock = dir.join("a.log.lock");
    fs::write(&lock, std::process::id().to_string()).unwrap();
    set_age(&lock, Duration::from_secs(9));
    assert!(!crash_lock_is_stale(&lock), "nine seconds is still within a write's time");
    set_age(&lock, Duration::from_secs(11));
    assert!(crash_lock_is_stale(&lock));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn process_is_running_tells_this_process_from_a_missing_one() {
    assert!(process_is_running(std::process::id()));
    assert!(!process_is_running(4294967000));
}

// ---------- paths ----------

#[test]
fn title_case_raises_the_first_letter_only() {
    assert_eq!(title_case("prompts"), "Prompts");
    assert_eq!(title_case("Prompts"), "Prompts");
    assert_eq!(title_case("ABC"), "ABC");
    assert_eq!(title_case("a"), "A");
    assert_eq!(title_case(""), "");
    assert_eq!(title_case("1ab"), "1ab");
    assert_eq!(title_case("élan vital"), "Élan vital");
    assert_eq!(title_case("ßx"), "SSx", "a letter that upper-cases to two keeps both");
    assert_eq!(title_case("日本"), "日本");
}

#[test]
fn volume_guid_paths_are_recognised_by_shape() {
    let guid = "01234567-89ab-cdef-0123-456789abcdef";
    assert!(is_volume_guid_path(&format!("Volume{{{guid}}}")));
    assert!(is_volume_guid_path(&format!("Volume{{{guid}}}\\")));
    assert!(is_volume_guid_path(&format!("Volume{{{guid}}}\\dir\\f.txt")));
    assert!(is_volume_guid_path(&format!("Volume{{{guid}}}/dir")));
    assert!(is_volume_guid_path(&format!("VOLUME{{{}}}", guid.to_uppercase())));
}

#[test]
fn volume_guid_paths_refuse_every_wrong_shape() {
    let guid = "01234567-89ab-cdef-0123-456789abcdef";
    for s in [
        String::new(),
        "Volume".to_string(),
        "Volume{}".to_string(),
        format!("Volume{{{guid}"),
        format!("Volume{{{guid}}}x"),
        format!("Volume{{{guid}}}name\\x"),
        format!("Volum{{{guid}}}"),
        format!("Volume({guid})"),
        "Volume{0123456789abcdef0123456789abcdef0123}".to_string(),
        "Volume{0123456g-89ab-cdef-0123-456789abcdef}".to_string(),
        "Volume{01234567-89ab-cdef-0123-456789abcdeg}".to_string(),
        "Volume{012345678-9ab-cdef-0123-456789abcdef}".to_string(),
        r"GLOBALROOT\Device\x".to_string(),
    ] {
        assert!(!is_volume_guid_path(&s), "{s:?}");
    }
}

#[test]
fn own_network_dir_matches_by_spelling_only() {
    let own = [PathBuf::from(r"\\nas\share\data")];
    for p in [
        r"\\nas\share\data\x.db",
        "//nas/share/data/x.db",
        r"\\NAS\SHARE\DATA\sub\x.db",
        r"  \\nas\share\data\x.db  ",
    ] {
        assert!(is_inside_own_network_dir(p, &own), "{p:?}");
    }
    for p in [
        r"\\nas\share\other\x.db",
        r"\\nas\share\data2\x.db",
        r"\\other\share\data\x.db",
        r"\\nas\share\data\..\other\x.db",
        "//nas/share/data/../x",
        r"\\?\GLOBALROOT\Device\Mup\nas\share\data\x.db",
        "",
    ] {
        assert!(!is_inside_own_network_dir(p, &own), "{p:?}");
    }
}

#[test]
fn own_network_dir_needs_an_own_folder_that_is_itself_on_a_share() {
    assert!(!is_inside_own_network_dir(r"\\nas\share\data\x", &[]));
    assert!(!is_inside_own_network_dir(r"\\nas\share\data\x", &[PathBuf::from(r"C:\data")]));
    assert!(!is_inside_own_network_dir(r"C:\data\x", &[PathBuf::from(r"C:\data")]), "a local path is judged elsewhere");
    let two = [PathBuf::from(r"C:\data"), PathBuf::from(r"\\nas\rec")];
    assert!(is_inside_own_network_dir(r"\\nas\rec\a.mp4", &two));
}

#[test]
fn size_error_names_the_limit_in_whole_megabytes() {
    assert_eq!(size_error(MAX_IMPORT_BYTES), "file is too large; the limit is 512 MB");
    assert_eq!(size_error(MAX_THEME_BYTES), "file is too large; the limit is 4 MB");
    assert_eq!(size_error(10 * 1024 * 1024), "file is too large; the limit is 10 MB");
}

#[test]
fn check_import_size_passes_up_to_the_limit_and_refuses_beyond_it() {
    let dir = util_dir("importsize");
    let f = dir.join("a.bin");
    fs::write(&f, vec![0u8; 10]).unwrap();
    assert!(check_import_size(&f, 10).is_ok(), "exactly at the limit passes");
    assert!(check_import_size(&f, 11).is_ok());
    assert!(check_import_size(&f, 9).is_err());
    let big = dir.join("big.bin");
    fs::write(&big, vec![0u8; 3 * 1024 * 1024 + 5]).unwrap();
    assert_eq!(
        check_import_size(&big, 2 * 1024 * 1024).unwrap_err(),
        "file is too large (3 MB); the limit is 2 MB"
    );
    let empty = dir.join("empty.bin");
    fs::write(&empty, b"").unwrap();
    assert!(check_import_size(&empty, 0).is_ok());
    assert!(check_import_size(&dir.join("missing.bin"), 10).is_err(), "an unreadable file is an error, not a pass");
    let _ = fs::remove_dir_all(&dir);
}

// ---------- model ----------

const ALL_LANGS: [&str; 11] = ["de", "es", "it", "pt", "pl", "zh", "ja", "nl", "tr", "ro", "fr"];

#[test]
fn resolve_lang_maps_a_chosen_language_or_tag_to_a_supported_code() {
    assert_eq!(LANG_CODES, ALL_LANGS);
    for code in ALL_LANGS {
        assert_eq!(resolve_lang(code), code);
    }
    assert_eq!(resolve_lang("de-AT"), "de");
    assert_eq!(resolve_lang("DE"), "de");
    assert_eq!(resolve_lang("fr-CA"), "fr");
    assert_eq!(resolve_lang("pt-BR"), "pt");
    assert_eq!(resolve_lang("zh-Hans-CN"), "zh");
    assert_eq!(resolve_lang("ja-JP"), "ja");
}

#[test]
fn resolve_lang_falls_back_to_english() {
    for pref in ["en", "en-US", "", "xx", "ru-RU", "ko", " de", "-"] {
        assert_eq!(resolve_lang(pref), "en", "{pref:?}");
    }
}

#[test]
fn resolve_lang_auto_follows_the_system_but_stays_supported() {
    let got = resolve_lang("auto");
    assert!(got == "en" || LANG_CODES.contains(&got), "{got}");
}

#[test]
fn every_language_has_every_tray_label_and_none_copies_the_english_text() {
    let en = (tray_labels("en"), tray_rec_stop_label("en"), tray_rec_pause_label("en", false), tray_rec_pause_label("en", true));
    assert_eq!(en.0, ("Open", "Quit"));
    assert_eq!(en.1, "Stop recording");
    assert_eq!(en.2, "Pause recording");
    assert_eq!(en.3, "Resume recording");
    assert_eq!(tray_section_labels("en"), ("Recently used", "Favorites"));
    assert_eq!(tray_search_placeholder("en"), "Search…");
    assert_eq!(tray_recording_label("en"), "Recording…");
    for lang in ALL_LANGS {
        let (open, quit) = tray_labels(lang);
        assert!(!open.is_empty() && !quit.is_empty(), "{lang}");
        assert_ne!(open, "Open", "{lang}");
        assert_ne!(quit, "Quit", "{lang}");
        assert_ne!(tray_rec_stop_label(lang), "Stop recording", "{lang}");
        assert_ne!(tray_rec_pause_label(lang, false), "Pause recording", "{lang}");
        assert_ne!(tray_rec_pause_label(lang, true), "Resume recording", "{lang}");
        assert_ne!(tray_rec_pause_label(lang, false), tray_rec_pause_label(lang, true), "{lang}");
        let (recent, fav) = tray_section_labels(lang);
        assert!(!recent.is_empty() && !fav.is_empty() && recent != fav, "{lang}");
        assert_ne!(recent, "Recently used", "{lang}");
        assert_ne!(fav, "Favorites", "{lang}");
        assert!(tray_search_placeholder(lang).ends_with('…'), "{lang}");
        assert_ne!(tray_search_placeholder(lang), "Search…", "{lang}");
        assert!(tray_recording_label(lang).ends_with('…'), "{lang}");
        assert_ne!(tray_recording_label(lang), "Recording…", "{lang}");
    }
}

#[test]
fn tray_labels_use_english_for_an_unknown_language() {
    for lang in ["", "xx", "DE", "de-DE"] {
        assert_eq!(tray_labels(lang), ("Open", "Quit"), "{lang:?}");
        assert_eq!(tray_rec_stop_label(lang), "Stop recording");
        assert_eq!(tray_rec_pause_label(lang, true), "Resume recording");
        assert_eq!(tray_section_labels(lang), ("Recently used", "Favorites"));
        assert_eq!(tray_search_placeholder(lang), "Search…");
    }
    assert_eq!(tray_labels("de"), ("Öffnen", "Beenden"));
    assert_eq!(tray_labels("ja"), ("開く", "終了"));
    assert_eq!(tray_rec_pause_label("fr", false), "Mettre l'enregistrement en pause");
}

#[test]
fn the_webview2_dialog_fills_in_the_product_name_in_every_language() {
    for lang in ["en", "de", "es", "it", "pt", "pl", "zh", "ja", "nl", "tr", "ro", "fr", "xx", ""] {
        let (title, msg) = webview2_dialog(lang);
        assert!(!title.is_empty() && !msg.is_empty(), "{lang:?}");
        assert!(!title.contains("{app}") && !msg.contains("{app}"), "{lang:?}");
        let (raw_title, raw_msg) = webview2_texts(lang);
        assert!(raw_title.contains("WebView2"), "{lang:?}");
        assert!(raw_msg.contains("{app}"), "{lang:?} needs the product name in its text");
        assert!(msg.contains(brand::DISPLAY_NAME), "{lang:?}");
    }
    assert_eq!(webview2_texts("xx"), webview2_texts("en"));
    assert_ne!(webview2_texts("de"), webview2_texts("en"));
}

#[test]
fn every_language_has_a_start_page_name_the_rename_check_knows() {
    for lang in ["en", "xx", "de", "es", "it", "pt", "pl", "zh", "ja", "nl", "tr", "ro", "fr"] {
        let name = home_name(lang);
        assert!(HOME_NAMES.contains(&name), "{lang}: {name}");
    }
    assert_eq!(home_name("de"), "Startseite");
    assert_eq!(home_name("it"), "Home");
    assert_eq!(home_name("nl"), "Home");
    assert_eq!(home_name("fr"), "Accueil");
    let mut sorted: Vec<&str> = HOME_NAMES.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), HOME_NAMES.len(), "a name listed twice");
}

#[test]
fn the_setting_defaults_are_the_documented_ones() {
    assert_eq!(default_theme(), "system");
    assert_eq!(default_cols(), 5);
    assert_eq!(default_rows(), 4);
    assert!(default_on());
    assert_eq!(default_language(), "auto");
    assert_eq!(default_tile_font(), "system");
    assert_eq!(default_tile_size(), 0);
    assert_eq!(default_rec_stop_hotkey(), "Ctrl+Alt+S");
    assert_eq!(default_rec_pause_hotkey(), "Ctrl+Alt+O");
    assert_ne!(default_rec_pause_hotkey(), Settings::default().hotkey, "the pause hotkey must not collide with the launcher's");
    assert_eq!(default_folder_scale(), 1.0);
    assert_eq!(default_custom_base(), "light");
    assert_eq!(legacy_update_consent(), UpdateConsent::Legacy);
    assert_eq!(grid_key(5, 4), "5x4");
    assert_eq!(grid_key(0, 0), "0x0");
}

#[test]
fn an_empty_settings_block_reads_as_the_defaults() {
    let s: Settings = serde_json::from_str("{}").unwrap();
    let d = Settings::default();
    assert_eq!(s.theme, "system");
    assert_eq!(s.language, "auto");
    assert_eq!(s.tile_font, "system");
    assert_eq!(s.tile_size, 0);
    assert_eq!(s.rec_stop_hotkey, d.rec_stop_hotkey);
    assert_eq!(s.rec_pause_hotkey, d.rec_pause_hotkey);
    assert_eq!(s.update_consent, UpdateConsent::Legacy, "a block from before the question existed counts as migrated");
    assert_eq!(d.update_consent, UpdateConsent::Unasked, "a new install has not answered yet");
}

#[test]
fn clamp_ui_value_holds_a_slider_to_its_range() {
    assert_eq!(clamp_ui_value("uiScale", 10.0), Some(75.0));
    assert_eq!(clamp_ui_value("uiScale", 75.0), Some(75.0));
    assert_eq!(clamp_ui_value("uiScale", 150.5), Some(150.5), "a slider value is not rounded");
    assert_eq!(clamp_ui_value("uiScale", 300.0), Some(300.0));
    assert_eq!(clamp_ui_value("uiScale", 9999.0), Some(300.0));
    assert_eq!(clamp_ui_value("uiScale", -1.0), Some(75.0));
    assert_eq!(clamp_ui_value("uiScale", 0.0), Some(75.0), "zero is only kept where the top stop means unlimited");
    assert_eq!(clamp_ui_value("floatOpacity", 0.0), Some(20.0));
    assert_eq!(clamp_ui_value("bubbleMs", -5.0), Some(0.0));
    assert_eq!(clamp_ui_value("libraryCols", 2.5), Some(2.5));
}

#[test]
fn clamp_ui_value_drops_a_number_that_is_not_finite() {
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(clamp_ui_value("uiScale", v), None);
        assert_eq!(clamp_ui_value("historyDays", v), None);
        assert_eq!(clamp_ui_value("noSuchKey", v), None);
    }
}

#[test]
fn clamp_ui_value_passes_an_unknown_key_through() {
    assert_eq!(clamp_ui_value("noSuchKey", -5.0), Some(-5.0));
    assert_eq!(clamp_ui_value("noSuchKey", 1e12), Some(1e12));
    assert_eq!(clamp_ui_value("", 3.5), Some(3.5));
}

#[test]
fn clamp_ui_value_snaps_a_select_to_a_whole_number() {
    // A retention select has a zero preset: anything below one means never.
    assert_eq!(clamp_ui_value("historyDays", 0.0), Some(0.0));
    assert_eq!(clamp_ui_value("historyDays", 0.4), Some(0.0));
    assert_eq!(clamp_ui_value("historyDays", -3.0), Some(0.0));
    assert_eq!(clamp_ui_value("historyDays", 30.0), Some(30.0));
    assert_eq!(clamp_ui_value("historyDays", 12.4), Some(12.0));
    assert_eq!(clamp_ui_value("historyDays", 12.5), Some(13.0));
    assert_eq!(clamp_ui_value("trashRetentionDays", 99999.0), Some(3650.0));
    // An interval select has no zero preset: below one means the smallest value.
    assert_eq!(clamp_ui_value("updateCheckH", 0.0), Some(1.0));
    assert_eq!(clamp_ui_value("updateCheckH", -9.0), Some(1.0));
    assert_eq!(clamp_ui_value("backupIntervalH", 0.2), Some(1.0));
    assert_eq!(clamp_ui_value("backupIntervalH", 3651.0), Some(3650.0));
}

#[test]
fn snap_select_value_reads_below_one_by_whether_the_select_has_a_zero_preset() {
    assert_eq!(snap_select_value(0.99, &[0.0, 7.0]), 0.0);
    assert_eq!(snap_select_value(-100.0, &[0.0, 7.0]), 0.0);
    assert_eq!(snap_select_value(0.99, &[6.0, 12.0]), 1.0);
    assert_eq!(snap_select_value(-100.0, &[]), 1.0);
    assert_eq!(snap_select_value(1.0, &[0.0]), 1.0);
    assert_eq!(snap_select_value(2.5, &[0.0]), 3.0);
    assert_eq!(snap_select_value(3650.0, &[]), 3650.0);
    assert_eq!(snap_select_value(3650.4, &[]), 3650.0);
    assert_eq!(snap_select_value(1e15, &[]), 3650.0);
}

#[test]
fn clamp_ui_values_fixes_a_whole_map_in_one_pass() {
    let mut m: HashMap<String, f64> = HashMap::new();
    m.insert("uiScale".into(), 10.0);
    m.insert("floatOpacity".into(), 50.0);
    m.insert("historyDays".into(), 0.3);
    m.insert("updateCheckH".into(), 1e9);
    m.insert("brokenA".into(), f64::NAN);
    m.insert("brokenB".into(), f64::INFINITY);
    m.insert("unknownKey".into(), -7.0);
    clamp_ui_values(&mut m);
    assert_eq!(m.get("uiScale"), Some(&75.0));
    assert_eq!(m.get("floatOpacity"), Some(&50.0));
    assert_eq!(m.get("historyDays"), Some(&0.0));
    assert_eq!(m.get("updateCheckH"), Some(&3650.0));
    assert_eq!(m.get("unknownKey"), Some(&-7.0));
    assert!(!m.contains_key("brokenA"), "a value that is not finite is dropped so the default applies");
    assert!(!m.contains_key("brokenB"));
    assert_eq!(m.len(), 5);
}

#[test]
fn clamp_ui_values_leaves_an_empty_map_and_missing_keys_alone() {
    let mut m: HashMap<String, f64> = HashMap::new();
    clamp_ui_values(&mut m);
    assert!(m.is_empty(), "keys the map lacks are not invented");
}

#[test]
fn every_range_table_row_is_a_usable_range() {
    for &(key, min, max, _) in crate::ui_ranges::UI_VALUE_RANGES {
        assert!(min <= max, "{key}");
        assert_eq!(clamp_ui_value(key, min), Some(min), "{key}");
        assert_eq!(clamp_ui_value(key, max), Some(max), "{key}");
        assert_eq!(clamp_ui_value(key, max + 1.0e6), Some(max), "{key}");
    }
    for &(key, options) in crate::ui_ranges::UI_SELECT_RANGES {
        assert!(!options.is_empty(), "{key}");
        for &o in options {
            let held = clamp_ui_value(key, o).unwrap();
            assert_eq!(held, o, "{key}: a preset survives the clamp");
        }
    }
}

#[test]
fn active_index_finds_the_active_view_or_falls_back_to_the_first() {
    let mut s = Settings::default();
    assert_eq!(s.active_index(), 0, "no views at all");
    s.views = vec![hotkeys_2::test_view("a"), hotkeys_2::test_view("b"), hotkeys_2::test_view("c")];
    s.active_view = "b".to_string();
    assert_eq!(s.active_index(), 1);
    s.active_view = "c".to_string();
    assert_eq!(s.active_index(), 2);
    s.active_view = "gone".to_string();
    assert_eq!(s.active_index(), 0);
    s.active_view = String::new();
    assert_eq!(s.active_index(), 0);
}

#[test]
fn active_view_mut_edits_the_active_view() {
    let mut s = Settings { views: vec![hotkeys_2::test_view("a"), hotkeys_2::test_view("b")], active_view: "b".to_string(), ..Default::default() };
    s.active_view_mut().name = "Second".to_string();
    assert_eq!(s.views[1].name, "Second");
    assert_eq!(s.views[0].name, "A");
}

#[test]
fn migrate_repoints_a_dangling_active_view_and_clamps_the_numbers() {
    let mut s = Settings { views: vec![hotkeys_2::test_view("a"), hotkeys_2::test_view("b")], active_view: "deleted".to_string(), ..Default::default() };
    s.ui_values.insert("uiScale".into(), 5000.0);
    s.migrate();
    assert_eq!(s.active_view, "a");
    assert_eq!(s.ui_values.get("uiScale"), Some(&300.0));
}

// ---------- update ----------

#[test]
fn version_eq_compares_component_wise_with_zero_padding() {
    assert!(version_eq("1.9", "1.9.0"));
    assert!(version_eq("1.9.0", "1.9"));
    assert!(version_eq("2.1.5", "2.1.5"));
    assert!(version_eq("2.1.5-rc1", "2.1.5"));
    assert!(version_eq("", "0"));
    assert!(version_eq("0.0.0", ""));
    assert!(!version_eq("1.9.1", "1.9"));
    assert!(!version_eq("1.9", "1.9.1"));
    assert!(!version_eq("1.10", "1.9"), "components compare as numbers, not text");
    assert!(!version_eq("2.0.0", "1.99.99"));
}

#[test]
fn clamp_update_hours_keeps_the_interval_inside_one_hour_to_one_week() {
    assert_eq!(clamp_update_hours(24.0), 24.0);
    assert_eq!(clamp_update_hours(12.5), 12.5);
    assert_eq!(clamp_update_hours(1.0), 1.0);
    assert_eq!(clamp_update_hours(168.0), 168.0);
    assert_eq!(clamp_update_hours(0.0), 1.0, "zero never means off here");
    assert_eq!(clamp_update_hours(0.5), 1.0);
    assert_eq!(clamp_update_hours(-5.0), 1.0);
    assert_eq!(clamp_update_hours(169.0), 168.0);
    assert_eq!(clamp_update_hours(1e9), 168.0);
}

#[test]
fn clamp_update_hours_uses_a_day_for_a_value_that_is_not_a_number() {
    assert_eq!(clamp_update_hours(f64::NAN), 24.0);
    assert_eq!(clamp_update_hours(f64::INFINITY), 24.0);
    assert_eq!(clamp_update_hours(f64::NEG_INFINITY), 24.0);
}

// ---------- colours ----------

#[test]
fn hex_rgb_reads_a_six_digit_colour_and_ignores_outer_spaces() {
    assert_eq!(hex_rgb("#FF8000"), Some([255, 128, 0]));
    assert_eq!(hex_rgb("#ff8000"), Some([255, 128, 0]));
    assert_eq!(hex_rgb("  #0A0b0C \n"), Some([10, 11, 12]));
    assert_eq!(hex_rgb("#000000"), Some([0, 0, 0]));
    assert_eq!(hex_rgb("#FFFFFF"), Some([255, 255, 255]));
}

#[test]
fn hex_rgb_refuses_anything_that_is_not_exactly_hash_and_six_hex_digits() {
    for s in ["", "#", "ff8000", "#fff", "#12345", "#1234567", "#GG0000", "##12345", "# 12345", "#日本", "rgb(1,2,3)"] {
        assert_eq!(hex_rgb(s), None, "{s:?}");
    }
}

#[test]
fn tray_relative_luminance_follows_the_contrast_formula() {
    let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
    assert_eq!(tray_relative_luminance([0, 0, 0]), 0.0);
    assert!(close(tray_relative_luminance([255, 255, 255]), 1.0));
    assert!(close(tray_relative_luminance([255, 0, 0]), 0.2126));
    assert!(close(tray_relative_luminance([0, 255, 0]), 0.7152));
    assert!(close(tray_relative_luminance([0, 0, 255]), 0.0722));
    // Mid grey sits on the curved part, a near-black on the linear start of it.
    assert!((tray_relative_luminance([128, 128, 128]) - 0.2159).abs() < 1e-3);
    assert!((tray_relative_luminance([10, 10, 10]) - 0.003035).abs() < 1e-5);
    assert!(tray_relative_luminance([20, 20, 20]) > tray_relative_luminance([10, 10, 10]));
}

#[test]
fn tray_mix_blends_by_percent_and_rounds_down() {
    assert_eq!(tray_mix([0, 0, 0], [255, 255, 255], 0), [0, 0, 0]);
    assert_eq!(tray_mix([0, 0, 0], [255, 255, 255], 100), [255, 255, 255]);
    assert_eq!(tray_mix([0, 0, 0], [255, 255, 255], 50), [127, 127, 127]);
    assert_eq!(tray_mix([100, 200, 50], [0, 0, 0], 25), [75, 150, 37]);
    assert_eq!(tray_mix([255, 255, 255], [0, 0, 0], 10), [229, 229, 229]);
    assert_eq!(tray_mix([10, 20, 30], [10, 20, 30], 63), [10, 20, 30]);
    assert_eq!(tray_mix([255, 255, 255], [255, 255, 255], 100), [255, 255, 255], "no overflow at the extremes");
}

#[test]
fn hex_to_color_reads_hash_and_six_hex_digits_with_full_alpha() {
    let c = hex_to_color("#102030").unwrap();
    assert_eq!((c.0, c.1, c.2, c.3), (0x10, 0x20, 0x30, 255));
    let c = hex_to_color("#FFFFFF").unwrap();
    assert_eq!((c.0, c.1, c.2, c.3), (255, 255, 255, 255));
    let c = hex_to_color("#abcdef").unwrap();
    assert_eq!((c.0, c.1, c.2), (0xab, 0xcd, 0xef));
}

#[test]
fn hex_to_color_refuses_a_wrong_shape() {
    for s in ["", "#", "102030", "#10203", "#1020304", "#10 030", "#GGGGGG", " #102030", "#102030 ", "#日本", "#ééé"] {
        assert!(hex_to_color(s).is_none(), "{s:?}");
    }
}

#[test]
fn hex_to_color_refuses_a_sign_in_place_of_a_digit() {
    assert!(hex_to_color("#+12345").is_none());
}

#[test]
fn picker_format_is_one_of_the_three_notations() {
    assert_eq!(color_format("rgb"), "rgb");
    assert_eq!(color_format("hsl"), "hsl");
    assert_eq!(color_format("hex"), "hex");
    for s in ["", "RGB", " rgb", "cmyk", "HSL ", "rgb(1,2,3)"] {
        assert_eq!(color_format(s), "hex", "{s:?}");
    }
}

#[test]
fn picker_hex_to_rgb_reads_a_trimmed_six_digit_colour() {
    assert_eq!(hex_to_rgb("#000000"), Some([0, 0, 0]));
    assert_eq!(hex_to_rgb("#FFFFFF"), Some([255, 255, 255]));
    assert_eq!(hex_to_rgb(" #0A0b0C "), Some([10, 11, 12]));
    assert_eq!(hex_to_rgb("#7f8081"), Some([127, 128, 129]));
}

#[test]
fn picker_hex_to_rgb_refuses_a_wrong_shape() {
    for s in ["", "#", "0A0B0C", "#0A0B0", "#0A0B0C0", "#0A0B0G", "##0A0B0", "#日本"] {
        assert_eq!(hex_to_rgb(s), None, "{s:?}");
    }
}

#[test]
fn picker_hex_to_rgb_refuses_a_sign_in_place_of_a_digit() {
    assert_eq!(hex_to_rgb("#+12345"), None);
}

#[test]
fn picker_colours_round_trip_through_hex() {
    for rgb in [[0u8, 0, 0], [255, 255, 255], [1, 2, 3], [250, 128, 10], [16, 32, 48]] {
        let hex = rgb_to_hex(rgb[0], rgb[1], rgb[2]);
        assert_eq!(hex_to_rgb(&hex), Some(rgb), "{hex}");
    }
}
