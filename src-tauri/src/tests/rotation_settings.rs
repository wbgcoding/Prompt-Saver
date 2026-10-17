use super::*;

// ---- Rotating prompts ----
// "Kopierfehler ⇒ kein Commit" is not tested here: it is a one-line structural
// guarantee in copy_prompt (`if ok { rotation_advance(...) }`), not a property
// of rotation_advance itself, which has no notion of clipboard success.

#[test]
fn rotation_order_mode_cycles_through_every_entry_in_sequence() {
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["b".into(), "c".into()]; // total = 3, "" = order
    let mut settings = Settings::default();
    let seen: Vec<usize> = (0..7)
        .map(|_| {
            let (idx, _, _) = rotation_next(&p, &settings);
            rotation_advance(&p, &mut settings);
            idx
        })
        .collect();
    assert_eq!(seen, vec![0, 1, 2, 0, 1, 2, 0]);
}

#[test]
fn rotation_random_bag_shows_each_entry_once_then_avoids_an_immediate_repeat() {
    let mut p = sample_prompts().remove(0);
    p.rotate_mode = "random".into();
    p.variants = vec!["b".into(), "c".into(), "d".into(), "e".into()]; // total = 5
    let mut settings = Settings::default();
    let total = 5;
    // Prime past the cold-start bootstrap: with no bag yet, rotation_next()
    // falls back to (idx 0, pos 1): the same "1" a freshly reshuffled bag
    // also reports for its own first, still-untouched peek, so pos briefly
    // repeats once (1, 1, 2, 3, 4, 5) before this one-time double 1 settles
    // into a clean cycle. This one throwaway draw gets a real bag seeded so
    // the loop below observes steady-state behaviour only.
    rotation_advance(&p, &mut settings);
    let mut first_cycle = std::collections::HashSet::new();
    let mut last_shown = None;
    let mut positions = Vec::new();
    for _ in 0..total {
        let (idx, pos, _) = rotation_next(&p, &settings);
        assert!(first_cycle.insert(idx), "the bag repeated an entry within one cycle");
        positions.push(pos);
        rotation_advance(&p, &mut settings);
        last_shown = Some(idx);
    }
    assert_eq!(first_cycle.len(), total, "the bag skipped an entry within one cycle");
    // The draw-number badge counts 1..total across one cycle, in order:
    // even though the entries themselves are shuffled.
    assert_eq!(positions, vec![1, 2, 3, 4, 5]);
    let (next_idx, next_pos, _) = rotation_next(&p, &settings);
    assert_ne!(Some(next_idx), last_shown, "reshuffle repeated the last-shown entry immediately");
    assert_eq!(next_pos, 1, "the draw-number badge must restart at 1 for the new cycle");
}

#[test]
fn rotation_stale_pointer_and_bag_never_go_out_of_range_after_entries_shrink() {
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["b".into()]; // total = 2 now, but state below predates the shrink
    let mut settings = Settings::default();
    settings.rotate_pos.insert(p.id.clone(), 9);
    let (idx, _, text) = rotation_next(&p, &settings);
    assert_eq!(idx, 9 % 2, "a stale order-mode pointer must clamp back into range");
    assert_eq!(text, p.variants[0]);

    p.rotate_mode = "random".into();
    settings.rotate_bag.insert(p.id.clone(), vec![4, 0]); // 4 no longer exists
    let (idx2, _, _) = rotation_next(&p, &settings);
    assert_eq!(idx2, 0, "a bag containing any stale index must be discarded, not partially trusted");
    rotation_advance(&p, &mut settings);
    let bag = settings.rotate_bag.get(&p.id).unwrap();
    assert!(bag.iter().all(|&i| (i as usize) < 2), "the reshuffled bag must only contain valid indices");
}

#[test]
fn rotation_disabled_globally_always_serves_entry_one_and_freezes_the_pointer() {
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["b".into(), "c".into()];
    p.rotate_mode = "random".into();
    let mut settings = Settings::default();
    settings.ui_flags.insert("rotatingPrompts".into(), false);
    for _ in 0..3 {
        let (idx, _, text) = rotation_next(&p, &settings);
        assert_eq!(idx, 0);
        assert_eq!(text, p.text);
        rotation_advance(&p, &mut settings); // must be a no-op while off
    }
    assert!(settings.rotate_pos.is_empty());
    assert!(settings.rotate_bag.is_empty());
}

#[test]
fn sanitize_variants_trims_drops_empties_and_caps_the_count() {
    let raw: Vec<String> = vec!["  b  ".into(), "".into(), "   ".into(), "c".into()];
    assert_eq!(sanitize_variants(raw), vec!["b".to_string(), "c".to_string()]);

    let oversized: Vec<String> = (0..80).map(|i| format!("v{i}")).collect();
    assert_eq!(sanitize_variants(oversized).len(), MAX_ROTATE_VARIANTS);
}

#[test]
fn sanitize_rotate_mode_is_empty_without_variants_and_only_random_survives() {
    let none: Vec<String> = Vec::new();
    let some = vec!["b".to_string()];
    assert_eq!(sanitize_rotate_mode("random".into(), &none), "");
    assert_eq!(sanitize_rotate_mode("random".into(), &some), "random");
    assert_eq!(sanitize_rotate_mode("garbage".into(), &some), "");
    assert_eq!(sanitize_rotate_mode("".into(), &some), "");
}

#[test]
fn rotate_auto_is_dropped_without_variants() {
    let none: Vec<String> = Vec::new();
    let some = vec!["b".to_string()];
    assert!(!sanitize_rotate_auto(true, &none));
    assert!(sanitize_rotate_auto(true, &some));
    assert!(!sanitize_rotate_auto(false, &some));
}

#[test]
fn an_unknown_restore_scope_is_refused_rather_than_guessed() {
    for ok in ["all", "data", "settings"] {
        assert_eq!(valid_scope(ok).unwrap(), ok);
    }
    // Left to their own reading, restore_backup treated an unknown scope as
    // "change nothing and report success" while import_all treated it as
    // "restore everything": opposite answers to the same input.
    for bad in ["", "ALL", "prompts", "data ", "everything"] {
        assert!(valid_scope(bad).is_err(), "{bad:?} must not be accepted");
    }
}

#[test]
fn a_capped_read_decides_from_the_bytes_it_actually_read() {
    let p = std::env::temp_dir().join("capped_read_test.bin");
    fs::write(&p, vec![b'x'; 64]).unwrap();

    // Exactly at the limit is still a valid file, and the caller gets all of it.
    assert_eq!(read_file_capped(&p, 64).unwrap().len(), 64);
    // One byte of headroom less and it is refused -- the extra byte the reader
    // takes is what proves the file is over, without a second look at the disk.
    assert!(read_file_capped(&p, 63).is_err());
    assert_eq!(read_text_capped(&p, 64).unwrap().len(), 64);
    assert!(read_text_capped(&p, 63).is_none());

    let _ = fs::remove_file(&p);
}

#[test]
fn attach_preview_extensions_match_the_four_text_kinds() {
    for ext in ["a.txt", "a.md", "a.json", "a.csv", "A.TXT", "a.Md"] {
        assert!(is_text_preview_ext(ext), "{ext:?} should be a text preview kind");
    }
    for ext in ["a.pdf", "a.png", "a.docx", "a", "a.txtx"] {
        assert!(!is_text_preview_ext(ext), "{ext:?} must not be a text preview kind");
    }
}

#[test]
fn a_short_attachment_reads_whole_with_no_truncation() {
    let p = std::env::temp_dir().join("attach_preview_short.txt");
    fs::write(&p, "hello attachment").unwrap();
    let (bytes, truncated) = read_bytes_preview(&p, 8 * 1024).unwrap();
    assert!(!truncated);
    assert_eq!(decode_text_preview(bytes).unwrap(), "hello attachment");
    let _ = fs::remove_file(&p);
}

#[test]
fn an_attachment_over_the_limit_is_cut_and_flagged() {
    let p = std::env::temp_dir().join("attach_preview_long.txt");
    fs::write(&p, vec![b'a'; 5000]).unwrap();
    let (bytes, truncated) = read_bytes_preview(&p, 1024).unwrap();
    assert!(truncated);
    let text = decode_text_preview(bytes).unwrap();
    assert_eq!(text.len(), 1024);
    let _ = fs::remove_file(&p);
}

#[test]
fn an_empty_attachment_previews_as_empty_not_truncated() {
    let p = std::env::temp_dir().join("attach_preview_empty.txt");
    fs::write(&p, "").unwrap();
    let (bytes, truncated) = read_bytes_preview(&p, 1024).unwrap();
    assert!(!truncated);
    assert_eq!(decode_text_preview(bytes).unwrap(), "");
    let _ = fs::remove_file(&p);
}

#[test]
fn a_cut_made_inside_a_multibyte_character_trims_back_cleanly() {
    // Three-byte characters throughout (the euro sign); the cap is set to
    // land one byte into the second one.
    let text = "e".repeat(10) + "\u{20AC}\u{20AC}\u{20AC}\u{20AC}"; // e's then euro signs
    let p = std::env::temp_dir().join("attach_preview_multibyte.txt");
    fs::write(&p, &text).unwrap();
    // 10 ascii bytes + one whole euro sign (3 bytes) + 1 byte into the next.
    let (bytes, truncated) = read_bytes_preview(&p, 14).unwrap();
    assert!(truncated);
    let decoded = decode_text_preview(bytes).unwrap();
    assert!(std::str::from_utf8(decoded.as_bytes()).is_ok());
    assert_eq!(decoded, "e".repeat(10) + "\u{20AC}");
    let _ = fs::remove_file(&p);
}

#[test]
fn a_non_utf8_attachment_is_reported_unreadable_not_guessed() {
    // Latin-1 bytes that are not valid UTF-8 (0xE9 alone is a lone
    // continuation-style byte with no valid lead byte before it).
    let p = std::env::temp_dir().join("attach_preview_binary.txt");
    fs::write(&p, [0x48u8, 0x65, 0xE9, 0x6C, 0x6C, 0x6F]).unwrap();
    let (bytes, _truncated) = read_bytes_preview(&p, 1024).unwrap();
    assert!(decode_text_preview(bytes).is_none());
    let _ = fs::remove_file(&p);
}

#[test]
fn colouring_every_tag_at_once_is_all_or_nothing() {
    let entry = |name: &str, color: &str| TagColor { name: name.into(), color: color.into() };
    let mut settings = Settings::default();
    settings.tag_colors.insert("kept".into(), "#111111".into());

    apply_tag_colors(&mut settings, vec![entry("Work", "#ff0000"), entry("Home", "#00ff00")]).unwrap();
    assert_eq!(settings.tag_colors.get("work"), Some(&"#ff0000".to_string()));
    assert_eq!(settings.tag_colors.get("home"), Some(&"#00ff00".to_string()));

    // A single bad colour rejects the whole batch: nothing before it in the
    // list may already have been written.
    let before = settings.tag_colors.clone();
    let err = apply_tag_colors(&mut settings, vec![entry("Fresh", "#0000ff"), entry("Bad", "red")]);
    assert!(err.is_err());
    assert_eq!(settings.tag_colors, before, "a rejected batch must change nothing");

    // An empty colour clears one, exactly like the single-tag command.
    apply_tag_colors(&mut settings, vec![entry("Work", "")]).unwrap();
    assert!(!settings.tag_colors.contains_key("work"));
    assert_eq!(settings.tag_colors.get("kept"), Some(&"#111111".to_string()));
}

#[test]
fn auto_eligible_rejects_placeholders_images_and_files() {
    // "placeholders" (variable text) are explicitly ALLOWED: this test
    // instead covers every gate auto_eligible actually enforces.
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["b".into(), "c".into()];
    p.rotate_auto = true;
    p.image = String::new();
    p.file_path = String::new();
    let settings = Settings::default();
    assert!(auto_eligible(&p, &settings), "a plain multi-entry text prompt must be eligible");

    let mut no_variants = p.clone();
    no_variants.variants.clear();
    assert!(!auto_eligible(&no_variants, &settings));

    let mut not_auto = p.clone();
    not_auto.rotate_auto = false;
    assert!(!auto_eligible(&not_auto, &settings));

    let mut has_image = p.clone();
    has_image.image = "data:image/png;base64,QUJD".into();
    assert!(!auto_eligible(&has_image, &settings));

    let mut has_file = p.clone();
    has_file.file_path = "C:\\tmp\\report.pdf".into();
    assert!(!auto_eligible(&has_file, &settings));

    let mut rotation_off = Settings::default();
    rotation_off.ui_flags.insert("rotatingPrompts".into(), false);
    assert!(!auto_eligible(&p, &rotation_off));

    let mut feature_off = Settings::default();
    feature_off.ui_flags.insert("rotateAuto".into(), false);
    assert!(!auto_eligible(&p, &feature_off));
}

#[test]
fn auto_session_order_matches_rotation_next() {
    // The auto flag must never change what rotation_next/rotation_advance
    // themselves return: same order-mode and random-mode guarantee as the
    // plain copy path, just proven again with rotate_auto set.
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["b".into(), "c".into()];
    p.rotate_auto = true;
    let mut settings = Settings::default();
    let seen: Vec<usize> = (0..6)
        .map(|_| {
            let (idx, _, _) = rotation_next(&p, &settings);
            rotation_advance(&p, &mut settings);
            idx
        })
        .collect();
    assert_eq!(seen, vec![0, 1, 2, 0, 1, 2]);

    p.rotate_mode = "random".into();
    let mut settings = Settings::default();
    let mut cycle: Vec<usize> = (0..3)
        .map(|_| {
            let (idx, _, _) = rotation_next(&p, &settings);
            rotation_advance(&p, &mut settings);
            idx
        })
        .collect();
    cycle.sort();
    assert_eq!(cycle, vec![0, 1, 2], "every entry exactly once per random cycle");
}

// The CF_HTML header's own numbers must point exactly at the fragment the
// header promises, byte for byte -- a reader that trusts the header over
// scanning for markers (most do) gets the wrong slice otherwise.
#[test]
fn cf_html_header_offsets_point_at_the_real_fragment_bytes() {
    let payload = build_cf_html("<b>hi</b>");
    let get = |key: &str| -> usize {
        let line = payload.lines().find(|l| l.starts_with(key)).unwrap();
        line[key.len()..].trim().parse().unwrap()
    };
    let start_html = get("StartHTML:");
    let end_html = get("EndHTML:");
    let start_fragment = get("StartFragment:");
    let end_fragment = get("EndFragment:");

    assert_eq!(&payload[start_html..end_html], &payload[start_html..], "EndHTML must reach the real end of the string");
    assert_eq!(end_html, payload.len(), "EndHTML must equal the payload's own byte length");
    assert_eq!(&payload[start_fragment..end_fragment], "<b>hi</b>", "the fragment slice must be exactly the fragment given");
    assert!(payload[start_html..].starts_with("<html>"), "StartHTML must point at the opening tag");
}

// A fragment with multi-byte UTF-8 characters must not shift the offsets by
// character count -- they are byte positions, and a wrong count here would
// slice a multi-byte character in half.
#[test]
fn cf_html_offsets_are_byte_positions_not_char_counts() {
    let fragment = "caf\u{e9} \u{20ac}100 \u{1f600}"; // café, euro sign, emoji
    assert_eq!(fragment.chars().count(), 11);
    assert_ne!(fragment.chars().count(), fragment.len(), "the fixture must actually contain multi-byte chars");
    let payload = build_cf_html(fragment);
    let get = |key: &str| -> usize {
        let line = payload.lines().find(|l| l.starts_with(key)).unwrap();
        line[key.len()..].trim().parse().unwrap()
    };
    let start_fragment = get("StartFragment:");
    let end_fragment = get("EndFragment:");
    assert_eq!(&payload[start_fragment..end_fragment], fragment);
    assert!(std::str::from_utf8(payload.as_bytes()).is_ok(), "the whole payload must stay valid UTF-8");
}

// Real clipboard access is a single shared OS resource -- serialised so this
// never races another test that also opens it.
#[cfg(windows)]
pub(super) static CLIPBOARD_TEST_LOCK: Mutex<()> = Mutex::new(());

// put_formatted's plain-text track must be exactly the Markdown source it was
// given, never the rendered HTML or a re-encoded copy of it -- an app that
// only reads plain text must see precisely what a normal copy would give it.
#[cfg(windows)]
#[test]
fn put_formatted_leaves_the_text_track_exactly_the_markdown_source() {
    let _guard = CLIPBOARD_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let text = "plain **not** rendered";
    let html = "<p>plain <strong>not</strong> rendered</p>";
    assert!(put_formatted(text, html));
    let read_back = arboard::Clipboard::new().and_then(|mut c| c.get_text()).unwrap_or_default();
    assert_eq!(read_back, text, "the plain-text track must be exactly the Markdown source, untouched");
}

#[cfg(windows)]
#[test]
fn counts_as_paste_gates_on_settle_cooldown_input_and_self_read() {
    let cfg = AutoCfg { settle_ms: 350, cooldown_ms: 250, input_ms: 3000, rearm_max: 12, timeout_min: 60 };
    // now, armed_at, last_paste, last_input, self_read, expected
    let cases: Vec<(u32, u32, u32, u32, bool, bool)> = vec![
        (10_000, 9_650, 0, 10_000, false, true),   // settle elapsed, no prior paste, fresh input -> counts
        (10_000, 9_800, 0, 10_000, false, false),  // still inside the settle window -> rejected
        (10_000, 9_650, 9_900, 10_000, false, false), // inside the cooldown since the last counted paste
        (10_000, 9_650, 9_600, 10_000, false, true),  // cooldown has elapsed -> counts
        (10_000, 9_650, 0, 6_000, false, false),   // last input too long ago -> a background actor, not a human
        (10_000, 9_650, 0, 10_000, true, false),   // our own get_text() triggered the render -> never counts
    ];
    for (now, armed_at, last_paste, last_input, self_read, expected) in cases {
        assert_eq!(
            counts_as_paste(now, armed_at, last_paste, last_input, self_read, &cfg),
            expected,
            "now={now} armed_at={armed_at} last_paste={last_paste} last_input={last_input} self_read={self_read}"
        );
    }
}

// Ten real-looking copies that must never reach the clip inbox, ten that must.
#[test]
fn looks_like_password_tells_secrets_from_ordinary_copies() {
    let secrets = [
        "Tr0ub4dor&3", "correcthorsebatterystaple9!", "P@ssw0rd2024", "xK9#mQ2$vL8p",
        "Zx7!Bq2#Wn9$", "a1B2c3D4e5F6", "9Fj!kLmN2#Qr", "Winter2024!Snow",
        "gH7#pL2vX9qR", "M0nk3y#Business",
    ];
    for s in secrets {
        assert!(looks_like_password(s, 8), "not flagged as a secret: {s}");
    }
    let ordinary = [
        "Write a short story about a dragon.",       // real sentence with spaces
        "p1",                                        // a prompt id, far too short
        "192.168.1.1",                                // an address, only two classes
        "short1!",                                    // below the 8-char floor
        "Rewrite this email so it sounds friendlier and more concise, please.\nThanks!", // multi-line
        "helloworld",                                 // one character class only
        "1234567890123",                               // digits only
        "ALLCAPSNOOTHERCLASS",                          // upper only
        "Prompt Saver",                                 // the app's own name (has a space anyway)
        "the quick brown fox",                          // ordinary prose, spaced
    ];
    for s in ordinary {
        assert!(!looks_like_password(s, 8), "wrongly flagged as a secret: {s}");
    }
}

#[test]
fn looks_like_password_respects_the_configurable_floor() {
    assert!(!looks_like_password("aB3!aB3!", 12), "8 chars must fail a 12-char floor");
    assert!(looks_like_password("aB3!aB3!", 8), "8 chars must pass an 8-char floor");
}

// The two new Windows clipboard-history/cloud-sync opt-out formats behave exactly
// like the pre-existing KeePass-style one in the one respect that matters: an
// explicit "do not include" (a stored 0, or a marker we could not even read)
// skips the content, while absence or an explicit "allow" never does.
#[test]
fn dword_clipboard_formats_are_excluded_only_on_an_explicit_zero() {
    assert!(!dword_format_excludes(false, None), "format absent, nothing to exclude on");
    assert!(!dword_format_excludes(false, Some(0)), "absent format's stray value must not matter");
    assert!(dword_format_excludes(true, Some(0)), "an explicit 0 excludes");
    assert!(!dword_format_excludes(true, Some(1)), "an explicit allow must not exclude");
    assert!(dword_format_excludes(true, None), "present but unreadable fails closed");
}

#[test]
fn rotation_forget_drops_both_maps_for_a_purged_prompt() {
    let mut settings = Settings::default();
    settings.rotate_pos.insert("p1".into(), 2);
    settings.rotate_bag.insert("p1".into(), vec![1, 0]);
    settings.rotate_pos.insert("p2".into(), 1);
    rotation_forget(&mut settings, &["p1".to_string()]);
    assert!(!settings.rotate_pos.contains_key("p1"));
    assert!(!settings.rotate_bag.contains_key("p1"));
    assert!(settings.rotate_pos.contains_key("p2"), "an unrelated prompt's pointer must survive");
}

// ---- Collected settings writes ----
//
// The disk is stood in for by a Settings clone the "writer" copies into, and
// SettingsDelay::take is the one place that decides whether a write happens;
// so counting its `true` answers counts disk writes exactly.

// One commit, exactly as rotation_commit does it: advance in memory, then mark.
pub(super) fn commit(p: &Prompt, settings: &mut Settings, delay: &SettingsDelay) {
    rotation_advance(p, settings);
    delay.mark();
}

#[test]
fn a_burst_of_rotation_commits_costs_at_most_two_settings_writes() {
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["b".into(), "c".into()]; // total = 3
    let delay = SettingsDelay::new(50);
    let mut settings = Settings::default();
    let mut disk = Settings::default();
    let mut writes = 0usize;
    for _ in 0..100 {
        commit(&p, &mut settings, &delay);
        // The timer thread looking at the mark between two commits.
        if delay.take(false) {
            writes += 1;
            disk = settings.clone();
        }
    }
    assert!(writes <= 2, "100 rapid commits took {writes} disk writes, expected at most 2");
    // Before the change this was 100. The in-memory pointer moved every time.
    assert_eq!(
        settings.rotate_pos.get(&p.id).copied(),
        Some((100 % 3) as u32),
        "the pointer must be right in memory after every single commit"
    );
    let _ = disk;
}

#[test]
fn a_collected_write_lands_once_the_marking_goes_quiet() {
    let delay = SettingsDelay::new(20);
    delay.mark();
    assert!(!delay.take(false), "nothing may be written while marks are still arriving");
    std::thread::sleep(Duration::from_millis(40));
    assert!(delay.take(false), "a quiet window must land the collected write");
    assert!(!delay.take(false), "and it must not be written twice");
}

#[test]
fn quitting_writes_a_collected_change_that_the_quiet_window_would_still_hold() {
    // A window long enough that the timer thread would never fire during the test.
    let delay = SettingsDelay::new(600_000);
    delay.mark();
    assert!(!delay.take(false), "the quiet window has not passed");
    assert!(delay.take(true), "an exit must write whatever is owed, window or not");
    assert!(!delay.take(true), "with nothing owed there is nothing to write");
}

#[test]
fn an_immediate_write_cancels_the_collected_one_instead_of_letting_it_land_later() {
    let delay = SettingsDelay::new(0);
    delay.mark();
    // save_settings writes the whole block, counters included, and discards.
    delay.discard();
    assert!(!delay.take(true), "a full write leaves nothing owed behind it");
}

#[test]
fn a_failed_collected_write_stays_owed_without_retrying_every_tick() {
    let delay = SettingsDelay::new(0);
    delay.mark();
    assert!(delay.take(false), "the tick takes the owed write");
    // The disk refused it.
    delay.failed();
    assert!(delay.owed(), "a write that did not land is still owed");
    assert!(!delay.take(false), "no retry inside the backoff, a broken disk is not hammered");
    assert!(delay.owed(), "the backoff must not drop the debt");
    assert!(delay.take(true), "an exit or a backup tries again regardless of the backoff");
    // That retry succeeded: nothing owed, and a later mark is not held back by the old failure.
    assert!(!delay.owed());
    delay.mark();
    assert!(delay.take(false), "a successful write clears the backoff");
}

#[test]
fn only_a_real_failure_owes_a_collected_write_again() {
    for (outcome, owed) in [
        (SettingsWrite::Written, false),
        (SettingsWrite::Refused, false),
        (SettingsWrite::Failed, true),
    ] {
        let delay = SettingsDelay::new(0);
        delay.mark();
        assert!(delay.take(false));
        delay.settle_collected(outcome);
        assert_eq!(delay.owed(), owed, "collected write, outcome {outcome:?}");
    }
}

#[test]
fn an_immediate_write_keeps_the_debt_only_when_it_failed() {
    for (outcome, owed) in [
        (SettingsWrite::Written, false),
        (SettingsWrite::Refused, false),
        (SettingsWrite::Failed, true),
    ] {
        let delay = SettingsDelay::new(0);
        delay.mark();
        delay.settle_immediate(outcome);
        assert_eq!(delay.owed(), owed, "immediate write, outcome {outcome:?}");
    }
}

#[test]
fn due_is_false_inside_the_backoff_and_true_after_it() {
    let delay = SettingsDelay::new(0);
    assert!(!delay.due(), "nothing owed");
    delay.mark();
    assert!(delay.due());
    assert!(delay.take(false));
    delay.failed();
    assert!(delay.owed() && !delay.due(), "owed but inside the backoff");
    *delay.retry_at.lock().unwrap() = Some(Instant::now() - Duration::from_millis(1));
    assert!(delay.due(), "the backoff has passed");
}

#[test]
fn a_failure_keeps_a_newer_mark_that_arrived_during_the_write() {
    let delay = SettingsDelay::new(0);
    delay.mark();
    assert!(delay.take(false));
    delay.mark(); // a copy lands while the write is running
    delay.failed();
    assert!(delay.owed());
    assert!(delay.take(true));
    assert!(!delay.take(true), "one debt, one retry");
}

#[test]
fn the_pointer_read_back_after_a_quit_is_the_one_the_last_commit_left() {
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["b".into(), "c".into(), "d".into()]; // total = 4
    let delay = SettingsDelay::new(600_000);
    let mut settings = Settings::default();
    let mut disk = Settings::default();
    for _ in 0..7 {
        commit(&p, &mut settings, &delay);
        if delay.take(false) {
            disk = settings.clone();
        }
    }
    // Nothing has landed yet: this is exactly the state a hard kill would leave.
    assert_ne!(
        disk.rotate_pos.get(&p.id).copied(),
        settings.rotate_pos.get(&p.id).copied(),
        "without the flush the disk is behind - that is what the quit has to fix"
    );
    // The quit.
    if delay.take(true) {
        disk = settings.clone();
    }
    // What the next launch loads.
    let restarted = disk;
    assert_eq!(
        restarted.rotate_pos.get(&p.id).copied(),
        settings.rotate_pos.get(&p.id).copied(),
        "the restored pointer must be the last commit's, not an earlier one"
    );
    assert_eq!(
        rotation_next(&p, &restarted).0,
        rotation_next(&p, &settings).0,
        "and the first copy after the restart must serve the same entry"
    );
}

// ---- Trash ----

#[test]
fn stale_trash_ids_respects_the_retention_window() {
    let mut trash = sample_prompts(); // p1, p2
    let now = 10_000_000u64;
    trash[0].trashed_at = now - 100; // just trashed: well inside any window
    trash[1].trashed_at = now - 1_000_000; // trashed long ago: past a 1-day window
    let max_age = 86_400; // 1 day
    let stale = stale_trash_ids(&trash, max_age, now);
    assert_eq!(stale, vec!["p2".to_string()], "only the old entry should be stale");
}

#[test]
fn stale_trash_ids_never_touches_a_zero_timestamp() {
    // trashed_at == 0 is the "never trashed" sentinel every non-trashed Prompt
    // carries: it must never be treated as "trashed an eternity ago".
    let mut trash = sample_prompts();
    trash[0].trashed_at = 0;
    trash[1].trashed_at = 0;
    assert!(stale_trash_ids(&trash, 1, 1_000_000).is_empty());
}

#[test]
fn trash_retention_zero_days_means_forever() {
    let mut settings = Settings::default();
    settings.ui_values.insert("trashRetentionDays".to_string(), 0.0);
    assert_eq!(trash_retention_secs(&settings), None);
    settings.ui_values.insert("trashRetentionDays".to_string(), 7.0);
    assert_eq!(trash_retention_secs(&settings), Some(7 * 86_400));
}
