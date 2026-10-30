use super::*;

#[test]
fn a_file_in_the_previous_format_does_not_read_as_an_outside_edit() {
    let mut p = sync_test_prompt();
    p.notes = "line one\nline two".into();
    p.text = "ends with a newline\n".into();
    let placement = SyncPlacement::default();
    let old_form = prompt_to_md_impl(&p, &placement, true);
    assert_ne!(old_form, prompt_to_md(&p, &placement));
    assert!(sync_md_matches(&p, &placement, &old_form));
    assert!(sync_md_matches(&p, &placement, &prompt_to_md(&p, &placement)));
    let mut other = p.clone();
    other.name = "renamed".into();
    assert!(!sync_md_matches(&other, &placement, &old_form));
}

#[test]
fn a_new_sync_folder_clears_a_paused_health() {
    let mut h = SyncHealth::default();
    for _ in 0..SYNC_FAIL_MAX {
        sync_health_note_fail(&mut h, SYNC_FAIL_MAX, "the drive is gone");
    }
    assert!(h.paused);
    sync_health_after_dir_change(&mut h);
    assert!(!h.paused);
    assert_eq!(h.fails, 0);
    assert!(h.reason.is_empty());
}

#[test]
fn sync_delete_dir_removes_only_the_files_the_app_wrote() {
    let dir = sync_tmp_dir("del-ok");
    write_ours(&dir, "p1.md", "one");
    write_ours(&dir, "p2.md", "two");
    sync_known_note(&dir.join("p1.md"));
    sync_known_note(&dir.join("p2.md"));
    sync_seen_record_own_write(&dir.join("p1.md"));
    sync_seen_record_own_write(&dir.join("p2.md"));
    fs::write(dir.join("AGENTS.md"), sync_agents_md()).unwrap();
    fs::write(dir.join("p1.md.lock"), "1234").unwrap();
    let data = sync_tmp_dir("del-ok-data");
    assert_eq!(sync_delete_dir(&dir, &data).unwrap(), 4);
    assert!(!dir.exists(), "an emptied folder goes with the files");
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn sync_delete_dir_aborts_whole_on_one_file_the_app_did_not_write() {
    let dir = sync_tmp_dir("del-foreign");
    write_ours(&dir, "p1.md", "one");
    fs::write(dir.join("my-taxes.txt"), "not yours").unwrap();
    fs::write(dir.join("plain-note.md"), "# just a note\n").unwrap();
    let data = sync_tmp_dir("del-foreign-data");
    let err = sync_delete_dir(&dir, &data).expect_err("a stranger's file stops the delete");
    assert!(err.contains("my-taxes.txt"), "the reason names what is in the way: {err}");
    assert!(err.contains("plain-note.md"), "every unknown file is named: {err}");
    // Nothing at all is removed on a refusal, not even our own files.
    assert!(dir.join("p1.md").exists());
    assert!(dir.join("my-taxes.txt").exists());
    assert!(dir.exists());
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn sync_delete_dir_keeps_a_file_the_library_never_had() {
    let dir = sync_tmp_dir("del-unknown");
    write_ours(&dir, "p1.md", "one");
    sync_known_note(&dir.join("p1.md"));
    // Another machine's new file: our header, a valid id, never imported here.
    write_ours(&dir, "fresh-from-elsewhere.md", "not in this library yet");
    let data = sync_tmp_dir("del-unknown-data");
    let err = sync_delete_dir(&dir, &data).expect_err("an unimported file stops the delete");
    assert!(err.contains("fresh-from-elsewhere.md"), "the file in the way is named: {err}");
    assert!(dir.join("fresh-from-elsewhere.md").exists());
    assert!(dir.join("p1.md").exists());
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn sync_delete_dir_is_never_recursive_and_never_the_data_folder() {
    let dir = sync_tmp_dir("del-sub");
    write_ours(&dir, "p1.md", "one");
    let sub = dir.join("holiday photos");
    fs::create_dir_all(&sub).unwrap();
    write_ours(&sub, "p9.md", "a file below, must not be reached");
    let data = sync_tmp_dir("del-sub-data");
    let err = sync_delete_dir(&dir, &data).expect_err("a sub-folder stops the delete");
    assert!(err.contains("holiday photos"), "the sub-folder is named: {err}");
    assert!(sub.join("p9.md").exists(), "nothing below the folder is ever touched");

    // The folder that holds the database itself can never be the one deleted,
    // whatever the settings claim.
    assert!(sync_delete_dir(&data, &data).is_err());
    assert!(data.exists());
    let err = sync_delete_dir(&data.join("sub"), &data.join("sub").join("deeper")).unwrap_err();
    assert!(!err.is_empty());
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn sync_dir_scan_counts_our_files_apart_from_everything_else() {
    let dir = sync_tmp_dir("scan");
    write_ours(&dir, "p1.md", "one");
    sync_known_note(&dir.join("p1.md"));
    sync_seen_record_own_write(&dir.join("p1.md"));
    fs::write(dir.join("AGENTS.md"), "the user rewrote this note").unwrap();
    fs::write(dir.join("notes.md"), "no header at all").unwrap();
    let scan = sync_dir_scan(&dir);
    assert_eq!(scan.ours.len(), 1);
    assert_eq!(scan.foreign.len(), 2, "an edited AGENTS.md is the user's text now");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_dir_scan_knows_a_leftover_permission_probe() {
    let dir = sync_tmp_dir("scan-probe");
    fs::write(dir.join("p1.md.lock.probe-4242"), "").unwrap();
    fs::write(dir.join("not an id.md.lock.probe-1"), "").unwrap();
    let scan = sync_dir_scan(&dir);
    assert_eq!(scan.ours.len(), 1, "our own probe file does not block removing the folder");
    assert_eq!(scan.foreign.len(), 1);
    let _ = fs::remove_dir_all(&dir);
}

// ---- Reading the folder: settle first, then read, then leave it alone ----

#[test]
fn sync_scan_reads_a_file_only_once_it_has_stopped_changing() {
    let a = SyncStamp { mtime_ns: 10, len: 100 };
    let b = SyncStamp { mtime_ns: 20, len: 140 };
    // Never seen: remember it, read it next round. An editor that writes straight
    // into the file would otherwise be caught halfway through.
    assert_eq!(sync_scan_decision(None, a), SyncScan::Settle);
    assert_eq!(sync_scan_decision(Some(SyncSeen { stamp: a, handled: false, hash: 0 }), b), SyncScan::Settle);
    // Unchanged since the last look: now it is whole and worth reading.
    assert_eq!(sync_scan_decision(Some(SyncSeen { stamp: a, handled: false, hash: 0 }), a), SyncScan::Read);
    // Already dealt with (read, or written by the app itself): no read at all.
    assert_eq!(sync_scan_decision(Some(SyncSeen { stamp: a, handled: true, hash: 0 }), a), SyncScan::Skip);
    assert_eq!(sync_scan_decision(Some(SyncSeen { stamp: a, handled: true, hash: 0 }), b), SyncScan::Settle);
}

#[test]
fn sync_stamp_notices_a_rewrite_of_the_same_length() {
    let dir = sync_tmp_dir("stamp");
    let path = dir.join("p1.md");
    fs::write(&path, "aaaa").unwrap();
    let first = sync_stamp_of(&path).unwrap();
    // Same size, different content: the modification time is what catches it.
    std::thread::sleep(Duration::from_millis(20));
    fs::write(&path, "bbbb").unwrap();
    let second = sync_stamp_of(&path).unwrap();
    assert_ne!(first, second);
    assert!(sync_stamp_of(&dir.join("nope.md")).is_none());
    let _ = fs::remove_dir_all(&dir);
}

// ---- One poll loop at a time, and none at all once the feature is off ----

#[test]
fn sync_poll_claim_lets_only_one_loop_hold_the_slot_at_once() {
    // A stray claim left by another test in this binary would make this one
    // flaky, so start from a known-released state regardless of history.
    sync_poll_release();
    assert!(sync_poll_try_claim(), "the first caller gets the slot");
    assert!(!sync_poll_try_claim(), "a second caller must not start a second timer");
    sync_poll_release();
    assert!(sync_poll_try_claim(), "released, the next caller can claim it again");
    sync_poll_release();
}

// ---- An unreachable folder pauses instead of failing forever ----

#[test]
fn sync_pauses_after_the_set_number_of_failed_rounds_and_says_so_once() {
    let mut h = SyncHealth::default();
    assert!(!sync_health_note_fail(&mut h, 3, "drive not there"));
    assert!(!sync_health_note_fail(&mut h, 3, "drive not there"));
    assert!(h.reason.contains("drive"));
    assert!(sync_health_note_fail(&mut h, 3, "drive not there"), "the round that reaches the limit reports once");
    assert!(h.paused);
    assert!(!sync_health_note_fail(&mut h, 3, "drive not there"), "and never again while it stays paused");
    // Paused rounds come back far apart, never stop entirely.
    assert_eq!(sync_wait_secs(true, 5), 50);
    assert_eq!(sync_wait_secs(false, 5), 5);
    // One good round clears everything.
    sync_health_note_ok(&mut h, 42, 1700);
    assert!(!h.paused);
    assert_eq!(h.fails, 0);
    assert_eq!(h.files, 42);
    assert_eq!(h.last_ok, 1700);
    assert!(h.reason.is_empty());
}

#[test]
fn sync_fail_max_is_the_fixed_five_rounds() {
    assert_eq!(SYNC_FAIL_MAX, 5);
}

// ---- What a hand-written file may put into the library ----

#[test]
fn incoming_tags_obey_the_same_rules_as_every_other_way_in() {
    let p = sync_test_prompt();
    let long = "x".repeat(MAX_TAG_LEN + 1);
    let many = (0..MAX_TAGS_PER_PROMPT + 5).map(|i| format!("t{i}")).collect::<Vec<_>>().join(", ");
    let content = format!("---\nname: n\ncolor: \ntags: ok, {long}, with\tcontrol, ok, {many}\nnotes: \n---\nbody\n");
    let out = apply_md_to_prompt(&p, &content).expect("a bad tag never costs the file its text");
    assert_eq!(out.text, "body", "the prompt itself still arrives");
    assert!(out.tags.len() <= MAX_TAGS_PER_PROMPT, "never more tags than the app allows: {:?}", out.tags);
    assert!(out.tags.iter().all(|t| t.chars().count() <= MAX_TAG_LEN), "no over-long tag: {:?}", out.tags);
    assert!(out.tags.iter().all(|t| tag_chars_ok(t)), "no control characters: {:?}", out.tags);
    assert_eq!(out.tags.iter().filter(|t| t.as_str() == "ok").count(), 1, "the duplicate is dropped");
}

#[test]
fn incoming_tags_take_the_casing_the_library_already_uses() {
    let p = sync_test_prompt();
    let known = HashMap::from([(tag_key("Work"), "Work".to_string())]);
    let content = "---\nname: n\ncolor: \ntags: work, WORK, fresh\nnotes: \n---\nbody\n";
    let out = apply_md_with_tags(&p, content, &known).expect("valid frontmatter");
    assert_eq!(out.tags, ["Work", "fresh"]);
}

#[test]
fn md_roundtrip_survives_umlauts_emoji_separators_and_the_rotation_marker() {
    let mut p = sync_test_prompt();
    p.name = "Grüße; Ümläute 🎉".into();
    p.notes = "Semikolon; Doppelpunkt: Emoji 🙂".into();
    p.text = "Zeile eins\n\nZeile zwei mit ; und : und 🎯\nund ein wörtliches <!-- next --> im Text".into();
    p.variants = vec!["Zweiter Eintrag 🌍".into(), "Dritter; mit Umlaut ä".into()];
    let content = prompt_to_md(&p, &SyncPlacement::default());
    let back = apply_md_to_prompt(&p, &content).expect("valid frontmatter");
    assert_eq!(back.name, p.name);
    assert_eq!(back.notes, p.notes);
    assert_eq!(back.text, p.text, "a literal rotation marker in the text is not a separator");
    assert_eq!(back.variants, p.variants);
    assert_eq!(back.variants.len(), p.variants.len(), "the number of entries is the same both ways");
    // Byte-identical the second time round, or the folder would rewrite itself
    // on every poll.
    assert_eq!(prompt_to_md(&back, &SyncPlacement::default()), content);
    assert!(content.is_char_boundary(content.len()), "written as plain UTF-8");

    // A separator someone broke by hand is simply not a separator any more: the
    // entries merge into one, and not a single character is lost.
    let hand_edited = content.replace("<!-- next -->", "<!-- nxt -->");
    let merged = apply_md_to_prompt(&p, &hand_edited).expect("still our file");
    assert!(merged.variants.is_empty());
    assert!(merged.text.contains("Zweiter Eintrag 🌍") && merged.text.contains("Dritter; mit Umlaut ä"));
}

#[test]
fn sync_lock_is_stale_once_the_holder_pid_is_dead() {
    // Bypasses the age check by asserting the pid-liveness half directly:
    // the age half needs real elapsed wall-clock time, not worth a sleeping test.
    assert!(pid_is_alive(std::process::id()), "this test's own process is alive");
}

// The poll interval and the staleness window are independent numbers. What keeps
// a lock held through a long write from being stolen by the next round is not the
// window at all, it is that the writing process is still alive.
#[test]
fn a_lock_held_by_a_living_process_never_goes_stale_however_old_it_is() {
    let dir = sync_tmp_dir("lock-age");
    let md = dir.join("p1.md");
    fs::write(&md, "x").unwrap();
    assert!(sync_lock_acquire(&md));
    let lock_path = sync_lock_path(&md);
    backdate(&lock_path, Duration::from_secs(SYNC_LOCK_STALE_SECS * 20));
    assert!(!sync_lock_is_stale(&lock_path), "our own still-running write keeps its lock");
    assert!(!sync_lock_acquire(&md), "and nobody else takes it from us");
    // A lock whose writer is gone (no readable pid) is picked up once it is old.
    fs::write(&lock_path, "not a pid").unwrap();
    backdate(&lock_path, Duration::from_secs(SYNC_LOCK_STALE_SECS * 20));
    assert!(sync_lock_is_stale(&lock_path), "an abandoned lock does not block the folder forever");
    sync_lock_release(&md);
    let _ = fs::remove_dir_all(&dir);
}

// A thousand entries, checked every few seconds: the round must not read the
// whole folder every time. Prints both numbers so a regression is visible.
// Repeats each side and keeps the smallest time: this test runs inside the
// full suite alongside a lot of unrelated disk and ffmpeg activity, and a
// single sample of a few dozen milliseconds was getting swamped by someone
// else's scheduling slice often enough to fail on a clean tree. The minimum
// of several still reflects the real cost of each approach - a regression
// back to a full read every round would still lose every single time.
#[test]
fn a_thousand_entries_cost_metadata_and_not_a_full_read_each_round() {
    const ROUNDS: u32 = 7;
    let dir = sync_tmp_dir("perf-1000");
    let mut prompts = Vec::new();
    for i in 0..1000 {
        let mut p = sync_blank_prompt(format!("perf{i}"));
        p.name = format!("Entry {i}");
        p.text = format!("Line one of entry {i}\nLine two, a bit of text to make the file realistic.\n");
        fs::write(dir.join(format!("{}.md", p.id)), prompt_to_md(&p, &SyncPlacement::default())).unwrap();
        prompts.push(p);
    }
    let mut seen: SyncSeenMap = HashMap::new();
    for p in &prompts {
        let path = dir.join(format!("{}.md", p.id));
        seen.insert(sync_seen_key(&path), SyncSeen { stamp: sync_stamp_of(&path).unwrap(), handled: true, hash: 0 });
    }
    let mut full_best = Duration::MAX;
    let mut stamped_best = Duration::MAX;
    let (mut read, mut skipped) = (0usize, 0usize);
    for _ in 0..ROUNDS {
        // What the round used to cost: every file read and compared, every time.
        let t0 = std::time::Instant::now();
        read = 0;
        for p in &prompts {
            let path = dir.join(format!("{}.md", p.id));
            if let Some(c) = read_text_capped(&path, MAX_SYNC_MD_BYTES) {
                if c != prompt_to_md(p, &SyncPlacement::default()) {
                    panic!("written and re-read content must match");
                }
                read += 1;
            }
        }
        full_best = full_best.min(t0.elapsed());
        // What it costs now once everything has settled: one stat per file.
        let t1 = std::time::Instant::now();
        skipped = 0;
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            let Some(stamp) = sync_stamp_of(&path) else { continue };
            if sync_scan_decision(seen.get(&sync_seen_key(&path)).copied(), stamp) == SyncScan::Skip {
                skipped += 1;
            }
        }
        stamped_best = stamped_best.min(t1.elapsed());
    }
    println!(
        "sync round over {read} entries: full read {full_best:?} (best of {ROUNDS}), \
             unchanged-check {stamped_best:?} (best of {ROUNDS}), skipped {skipped}"
    );
    assert_eq!(read, 1000);
    assert_eq!(skipped, 1000, "an unchanged folder is skipped file by file, not re-read");
    assert!(stamped_best < full_best, "the settled round must be the cheaper one: {stamped_best:?} vs {full_best:?}");
    let _ = fs::remove_dir_all(&dir);
}

// Every identifier a rebrand may NOT quietly change. These literals are a
// deliberate second copy of brand.rs: an installation finds its data, its
// autostart entry and its update source through them, so a refactor that moves
// one has to fail here instead of in someone's %APPDATA%. tools/brand.js
// rewrites this block (BRAND-PIN markers) when a different brand is stamped.
#[test]
fn brand_values_are_pinned() {
    // BRAND-PIN-START
    assert_eq!(brand::DISPLAY_NAME, "Prompt Saver");
    assert_eq!(brand::SLUG, "prompt-saver");
    assert_eq!(brand::IDENTIFIER, "com.bgcoding.promptsaver");
    assert_eq!(brand::DATA_FOLDER, "Prompt-Saver");
    assert_eq!(brand::AUTOSTART_KEY, "PromptSaver");
    assert_eq!(brand::TEMP_DIR, "prompt-saver");
    assert_eq!(brand::TEMP_PREFIX, "promptsaver");
    assert_eq!(brand::WINDOW_CLASS, "PromptSaverClipWatch");
    assert_eq!(brand::USER_AGENT, "PromptSaver");
    assert_eq!(brand::REPO, "wbgcoding/Prompt-Saver");
    assert_eq!(brand::LEGACY_DATA_FOLDER, "com.bgcoding.promptsaver");
    assert_eq!(brand::LEGACY_AUTOSTART_KEY, "");
    assert_eq!(brand::LEGACY_SCREENSHOT_PREFIX, "PromptSaver-");
    assert_eq!(env!("CARGO_PKG_NAME"), "prompt-saver");
    let conf = include_str!("../../tauri.conf.json");
    assert!(conf.contains("\"productName\": \"Prompt Saver\""));
    assert!(conf.contains("\"identifier\": \"com.bgcoding.promptsaver\""));
    let js = include_str!("../../../ui/brand.js");
    assert!(js.contains("APP_NAME = \"Prompt Saver\";"));
    // BRAND-PIN-END
    // The consts the app actually reads must be the ones pinned above.
    assert_eq!(AUTOSTART_KEY, brand::AUTOSTART_KEY);
    assert_eq!(DATA_FOLDER, brand::DATA_FOLDER);
    // Backup crypto is NOT brand-derived: these two are frozen for every build
    // of every brand, or existing .psb files stop opening.
    assert_eq!(BACKUP_PEPPER, &b"prompt-saver::backup::pepper::v1"[..]);
    assert_eq!(BACKUP_FIXED_KEY[0], 0x8f);
    assert_eq!(BACKUP_FIXED_KEY[31], 0x84);
}

#[test]
fn update_url_pin_rejects_escapes() {
    let base = format!("https://github.com/{}/releases/download", brand::REPO);
    // The one legit shape is accepted.
    assert!(valid_update_url(&format!("{base}/v2.5.0/App_2.5.0_x64-setup.exe")));
    // Path-traversal to another repo, still github.com and still prefix-matching.
    assert!(!valid_update_url(&format!(
        "{base}/../../attacker/repo/releases/download/v1/x-setup.exe"
    )));
    // Encoded dot-segment, backslash, extra scheme/authority, query, non-exe.
    assert!(!valid_update_url(&format!("{base}/%2e%2e/x/y-setup.exe")));
    assert!(!valid_update_url(&format!("{base}/v1\\x/y-setup.exe")));
    assert!(!valid_update_url(&format!("{base}/v1/y-setup.exe?x=1")));
    assert!(!valid_update_url(&format!("{base}/v1/y.txt")));
    assert!(!valid_update_url(&format!("http://github.com/{}/releases/download/v1/y-setup.exe", brand::REPO)));
    // Extra path segments beyond tag/asset.
    assert!(!valid_update_url(&format!("{base}/v1/sub/y-setup.exe")));
    // A different repo under the same host is not ours.
    assert!(!valid_update_url("https://github.com/someone/else/releases/download/v1/y-setup.exe"));
}

#[test]
fn install_update_rejects_a_release_older_than_the_installed_version() {
    let base = format!("https://github.com/{}/releases/download", brand::REPO);
    // An older or equal tag than APP_VERSION must never pass, even at an
    // otherwise perfectly pinned, valid-shaped URL.
    assert!(!update_url_is_newer_release(&format!("{base}/v0.0.1/App_0.0.1_x64-setup.exe")));
    assert!(!update_url_is_newer_release(&format!(
        "{base}/v{APP_VERSION}/App_{APP_VERSION}_x64-setup.exe"
    )));
    // A genuinely newer tag still passes.
    assert!(update_url_is_newer_release(&format!(
        "{base}/v99.0.0/App_99.0.0_x64-setup.exe"
    )));
    // A malformed/missing tag is rejected rather than treated as newer.
    assert!(!update_url_is_newer_release(&format!("{base}//App-setup.exe")));
}

// The asset protocol allows paths added at run time (the data folder and the files
// the user picked). Everything that holds the store itself is still denied by name,
// so a crafted attachment path inside an allowed folder cannot hand the page the
// whole database. Backups count: a snapshot folder holds a
// gzipped copy of data.db, and a .psb is the same store in one file.
#[test]
fn asset_scope_denies_every_store_file() {
    // A narrow glob match good enough for this fixed pattern shape: "**/name",
    // "**/prefix*" and "**/dir/**": not a general glob engine.
    fn wild(pat: &str, name: &str) -> bool {
        match pat.split_once('*') {
            None => pat == name,
            Some((head, tail)) => {
                name.strip_prefix(head).is_some_and(|rest| (0..=rest.len()).any(|i| rest.is_char_boundary(i) && wild(tail, &rest[i..])))
            }
        }
    }
    fn covers(pattern: &str, path: &str) -> bool {
        let Some(rest) = pattern.strip_prefix("**/") else { return false };
        if let Some(dir) = rest.strip_suffix("/**") {
            path.split('/').any(|seg| seg == dir)
        } else {
            path.rsplit('/').next().is_some_and(|name| wild(rest, name))
        }
    }

    let conf: serde_json::Value =
        serde_json::from_str(include_str!("../../tauri.conf.json")).expect("tauri.conf.json parses");
    let deny = conf["app"]["security"]["assetProtocol"]["scope"]["deny"]
        .as_array()
        .expect("a deny list");
    let patterns: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
    for path in [
        "C:/Users/x/AppData/Roaming/Prompt-Saver/data.db",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/data.db-wal",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/data.db-shm",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/data.db.locked.bak",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/data.db.tmp",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/settings.json",
        // The atomic-write fallback: write_sealed_json's `path.with_extension("tmp")`
        // replaces the extension, so this is "settings.tmp", never "settings.json.tmp".
        "C:/Users/x/AppData/Roaming/Prompt-Saver/settings.tmp",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/prompts.json",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/prompts.tmp",
        // The atomic writer names its temp file "<file>.tmp-<id>" next to the target.
        "C:/Users/x/AppData/Roaming/Prompt-Saver/settings.json.tmp-1f3a9c",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/prompts.json.tmp-1f3a9c",
        // An export_all leftover beside a user-picked backup file.
        "D:/Backups/mine.psb.tmp-p1760000000000000000_3",
        "D:/Backups/mine.psb",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/backups/2026-08-19.psb",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/backups/subdir/2026-08-19.psb",
        "C:/Users/x/AppData/Roaming/Prompt-Saver/error.log",
    ] {
        assert!(
            patterns.iter().any(|p| covers(p, path)),
            "asset scope must deny {path}"
        );
    }
}

#[test]
fn store_crypt_roundtrip() {
    let secret = "prompt with sécret 🔐 {?x}";
    assert_eq!(dec_str(&enc_str(secret)), secret);
    // Legacy plaintext (no prefix) passes through unchanged for migration.
    assert_eq!(dec_str("legacy plaintext"), "legacy plaintext");
}

// The store's JSON files are the DB-write fallback: they hold full prompt bodies
// and private notes, so they must reach disk sealed, not as readable JSON.
#[test]
fn store_json_fallback_is_written_sealed() {
    let _guard = SEAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("ps-sealed-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("prompts.json");
    let secret = vec!["confidential prompt title".to_string()];

    assert!(write_sealed_json(&path, &secret));
    let raw = fs::read_to_string(&path).expect("written");
    assert!(raw.starts_with(ENC_PREFIX), "fallback must be sealed, got: {raw}");
    assert!(!raw.contains("confidential"), "plaintext leaked into the fallback file");
    match read_sealed_json::<Vec<String>>(&path) {
        SealedRead::Loaded(back) => assert_eq!(back, secret),
        _ => panic!("expected a loaded value"),
    }

    // A genuinely old plaintext install still migrates in.
    fs::write(&path, r#"["legacy entry"]"#).expect("write legacy");
    match read_sealed_json::<Vec<String>>(&path) {
        SealedRead::Loaded(legacy) => assert_eq!(legacy, vec!["legacy entry".to_string()]),
        _ => panic!("expected legacy plaintext to load"),
    }

    let _ = fs::remove_dir_all(&dir);
}

// A sync file write must never be an in-place truncate: a reader (this
// machine's own next poll, or a cloud-sync client) must only ever see either the
// old content or the new content, never a partial body.
#[test]
fn write_bytes_atomic_replaces_without_leaving_a_truncated_body() {
    let dir = std::env::temp_dir().join(format!("ps-atomic-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("p1.md");
    fs::write(&path, "---\nname: Old\n---\n\nold body").unwrap();

    let new_content = "---\nname: New\n---\n\n".to_string() + &"x".repeat(5000);
    assert!(write_bytes_atomic(&path, new_content.as_bytes()).is_ok());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        new_content,
        "must be fully replaced, never a partial write"
    );

    // The temp name must not collide with sync_sweep_orphans' ".md" filter: a
    // leftover temp file from an interrupted rename must never be treated as a
    // real synced document.
    let tmp = path.with_extension("tmp");
    assert!(!tmp.exists(), "temp file must be gone after a successful rename");
    assert!(!tmp.to_string_lossy().ends_with(".md"), "temp name must not look like a real sync file");

    let _ = fs::remove_dir_all(&dir);
}

// A data-dir move must bring backups/ along, nested subfolders included --
// otherwise the Backup panel shows 0 snapshots right after moving.
#[test]
fn copy_dir_all_copies_nested_files() {
    let base = std::env::temp_dir().join(format!("ps-copydir-{}", std::process::id()));
    let src = base.join("src");
    let dst = base.join("dst");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(src.join("2026-01-01_00-00-00")).unwrap();
    fs::write(src.join("2026-01-01_00-00-00").join("data.db.gz"), b"snapshot").unwrap();
    fs::write(src.join("prompts.names.json"), b"[]").unwrap();

    copy_dir_all(&src, &dst).unwrap();

    assert_eq!(
        fs::read(dst.join("2026-01-01_00-00-00").join("data.db.gz")).unwrap(),
        b"snapshot"
    );
    assert_eq!(fs::read(dst.join("prompts.names.json")).unwrap(), b"[]");

    let _ = fs::remove_dir_all(&base);
}

// With multiViewPrompt off (default), a synced placement must strip the
// prompt from every OTHER view: otherwise re-syncing an unrelated content
// change duplicates it across every view it was ever placed in on any machine.
#[test]
fn sync_place_in_view_strips_other_views_unless_multi_view_is_on() {
    let mut settings = Settings {
        views: vec![test_view("v1"), test_view("v2")],
        ..Default::default()
    };
    let key1 = grid_key(settings.views[0].cols, settings.views[0].rows);
    settings.views[0].layouts.entry(key1.clone()).or_default().insert("p1".to_string(), [2, 2]);

    // Default: placing into v2 must remove the v1 placement.
    sync_place_in_view(&mut settings, "p1", "v2", [0, 0]);
    assert!(
        !settings.views[0].layouts.get(&key1).is_some_and(|m| m.contains_key("p1")),
        "must be removed from v1"
    );
    let key2 = grid_key(settings.views[1].cols, settings.views[1].rows);
    assert_eq!(settings.views[1].layouts.get(&key2).and_then(|m| m.get("p1")), Some(&[0, 0]));

    // With multiViewPrompt on, a placement into a THIRD view must not touch v2's.
    settings.ui_flags.insert("multiViewPrompt".to_string(), true);
    settings.views.push(test_view("v3"));
    sync_place_in_view(&mut settings, "p1", "v3", [1, 1]);
    let key3 = grid_key(settings.views[2].cols, settings.views[2].rows);
    assert_eq!(settings.views[2].layouts.get(&key3).and_then(|m| m.get("p1")), Some(&[1, 1]));
    assert_eq!(
        settings.views[1].layouts.get(&key2).and_then(|m| m.get("p1")),
        Some(&[0, 0]),
        "v2 placement must survive with multi-view on"
    );
}

// Backdates a path's mtime so a staleness check sees it as old. FILE_FLAG_BACKUP_
// SEMANTICS is what lets Windows open a directory handle at all here.
pub(super) fn backdate(path: &std::path::Path, age: Duration) {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    let f = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .unwrap();
    let old = SystemTime::now() - age;
    f.set_times(std::fs::FileTimes::new().set_modified(old)).unwrap();
}

// A snip preview or an update stage dir surviving a crash/abnormal exit is
// never cleaned by anything else: must be swept at the next startup. But
// NOT a fresh one: "Allow multiple instances" lets a second launch run this
// same sweep while a concurrently open instance still has its own snip preview
// in active use, so only files older than STALE_TEMP_FILE_AGE are fair game.
pub(super) fn staging_test_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ps-stage-{}-{}-{}", tag, std::process::id(), gen_id()));
    fs::create_dir_all(&d).unwrap();
    d
}

// The race: a staged copy sits in <data>/files while no prompt references it yet;
// a concurrent sweep (whose referenced sets lack it) must leave it alone, and it
// must still survive once the insert completed and the pending entry is released.
#[test]
fn sweep_keeps_a_staged_copy_while_the_add_is_in_flight() {
    let base = staging_test_dir("race");
    let files = base.join("files");
    let orig = base.join("orig.txt");
    fs::write(&orig, b"data").unwrap();
    let mut staged = StagedCopies::new();
    let copy = staged.stage(&files, &orig.to_string_lossy());
    assert_ne!(copy, orig.to_string_lossy());
    assert!(Path::new(&copy).is_file());
    let empty = std::collections::HashSet::new();
    sweep_orphan_files_in(&files, &empty, || Some(std::collections::HashSet::new()));
    assert!(Path::new(&copy).is_file(), "an in-flight staged copy was swept");
    // the insert completes: the store references it, the pending entry goes away
    staged.commit();
    drop(staged);
    let mut live = std::collections::HashSet::new();
    live.insert(copy.replace('/', "\\").to_lowercase());
    sweep_orphan_files_in(&files, &empty, || Some(live));
    assert!(Path::new(&copy).is_file(), "a referenced copy was swept after the insert");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn a_panic_with_a_live_guard_keeps_the_staged_copy_but_releases_the_pending_entry() {
    let base = staging_test_dir("panic");
    let files = base.join("files");
    let orig = base.join("orig.txt");
    fs::write(&orig, b"data").unwrap();
    let mut copy = String::new();
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut staged = StagedCopies::new();
        copy = staged.stage(&files, &orig.to_string_lossy());
        panic!("boom");
    }));
    assert!(res.is_err());
    assert!(Path::new(&copy).is_file(), "a copy staged before a panic must survive");
    let pending = PENDING_ATTACHMENTS.lock().unwrap_or_else(|e| e.into_inner());
    assert!(!pending.as_ref().is_some_and(|s| s.contains(&pending_key(&copy))), "pending entry released");
    drop(pending);
    let _ = fs::remove_dir_all(&base);
}

// A failed add (validation error after staging) must not leave the copy behind.
#[test]
fn a_failed_add_leaves_no_staged_copy_behind() {
    let base = staging_test_dir("fail");
    let files = base.join("files");
    let orig = base.join("orig.txt");
    fs::write(&orig, b"data").unwrap();
    let copy = {
        let mut staged = StagedCopies::new();
        staged.stage(&files, &orig.to_string_lossy())
        // dropped without commit: the add failed
    };
    assert!(!Path::new(&copy).exists(), "the copy of a failed add stayed on disk");
    assert!(!pending_attachments_contain(&copy), "the pending entry leaked");
    assert!(orig.is_file(), "the user's original must never be touched");
    let _ = fs::remove_dir_all(&base);
}

// Every production source file of the crate: src/*.rs except the test file,
// each cut at its own inline test module, in name order, with \n line ends.
// The structural checks below read this instead of one fixed file, so they
// keep finding a function after it moves to another file of the crate.
pub(super) fn production_sources() -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("read the src dir")
        .map(|e| e.expect("src dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .filter(|p| p.file_name().is_some_and(|n| n != "tests.rs" && n != "cli_tests.rs"))
        .collect();
    files.sort();
    let mut out = String::new();
    for f in &files {
        let text = fs::read_to_string(f).expect("read a source file").replace("\r\n", "\n");
        let cut = text.find("\n#[cfg(test)]\nmod tests {").map_or(text.len(), |i| i + 1);
        out.push_str(&text[..cut]);
        out.push('\n');
    }
    out
}

// Structural check (no timing-based red exists for "the lock is not held
// during the copy"): in add_prompt the copy, the probe and the OCR pass all
// sit before the lock that inserts, and update_prompt stages the same three
// before its locked section.
#[test]
fn add_and_update_prompt_do_the_slow_work_before_the_insert_lock() {
    let src = production_sources();
    let body = |from: &str, to: &str| {
        assert_eq!(src.matches(from).count(), 1, "{from} must occur exactly once in the crate sources");
        let a = src.find(from).expect("start marker");
        let b = a + src[a..].find(to).expect("end marker");
        src[a..b].to_string()
    };
    let add = body("async fn add_prompt(", "// One example prompt as the frontend supplies it");
    let insert_lock = add.rfind(concat!("let mut store = lock(&", "state);")).expect("insert lock");
    for call in ["staged_copies.stage(", "probe_media_meta(", "ocr_index_text(img"] {
        let at = add.find(call).unwrap_or_else(|| panic!("{call} missing"));
        assert!(at < insert_lock, "add_prompt must run {call} before taking the insert lock");
        assert!(!add[insert_lock..].contains(call), "add_prompt must not run {call} under the insert lock");
    }
    let upd = body("async fn update_prompt(", "// List a prompt's version snapshots");
    let staged_end = upd.find("let updated = {").expect("locked section");
    for call in ["staged_copies.stage(", "probe_media_meta(", "ocr_index_text(img"] {
        assert!(upd[..staged_end].contains(call), "update_prompt must stage {call} before its locked section");
    }
}

#[test]
fn the_vault_reports_how_long_until_the_idle_timeout_locks_it() {
    assert_eq!(secret_relock_in_secs(15, 0), Some(900));
    assert_eq!(secret_relock_in_secs(15, 899), Some(1));
    assert_eq!(secret_relock_in_secs(15, 5000), Some(0));
    assert_eq!(secret_relock_in_secs(0, 5000), None, "a timeout of 0 means until quit");
}

// A delete that fails to save rolls the prompt back, so its pill must still be
// open: the window may only be closed once the trash move is on disk.
#[test]
fn delete_prompt_closes_the_pill_only_after_the_trash_move_saved() {
    let src = production_sources();
    let a = src.find("async fn delete_prompt(").expect("delete_prompt");
    let body = &src[a..a + src[a..].find("// The in-memory half of a batch delete").expect("end marker")];
    let saved = body.find("save_prompts_and_trash(&app, s)").expect("the trash save");
    let closed = body.find("close_floating_window(&app, &id)").expect("the pill close");
    assert!(closed > saved, "the pill must be closed after the save, not before it");
}

#[test]
fn only_one_picture_text_pass_runs_at_a_time() {
    let first = OcrIndexPass::claim().expect("nothing is running");
    assert!(OcrIndexPass::claim().is_none(), "a second pass must not start while one runs");
    drop(first);
    assert!(OcrIndexPass::claim().is_some(), "the claim is released when the pass ends");
}

// A write that sends no text and no entries (a hotkey or the auto-rotate switch
// set straight from the tile menu) keeps what is stored, whatever the entry is.
// For a confidential entry that means the ciphertext, with the vault locked too.
#[test]
fn an_edit_without_text_keeps_a_confidential_prompts_ciphertext() {
    let key = [7u8; 32];
    let mut p = image_text_prompt("s", "");
    p.secret = true;
    p.text = secret_encrypt("the real secret", &key);
    p.variants = vec![secret_encrypt("second", &key)];

    let (text, variants) = edited_text_and_variants(&p, None, None, None).unwrap();
    assert_eq!((text.as_str(), variants.as_slice()), (p.text.as_str(), p.variants.as_slice()));
    let (text, variants) = edited_text_and_variants(&p, None, None, Some(key)).unwrap();
    assert_eq!(secret_decrypt(&text, &key).unwrap(), "the real secret");
    assert_eq!(secret_decrypt(&variants[0], &key).unwrap(), "second");

    // New plain text is sealed, and refused while there is no key.
    let (text, _) = edited_text_and_variants(&p, Some("changed".into()), Some(vec![]), Some(key)).unwrap();
    assert_eq!(secret_decrypt(&text, &key).unwrap(), "changed");
    assert_eq!(edited_text_and_variants(&p, Some("changed".into()), None, None), Err("secret-locked".to_string()));

    // A plain prompt takes new content as it is and keeps what is not sent.
    let mut plain = image_text_prompt("p", "");
    plain.text = "old".into();
    plain.variants = vec!["v".into()];
    let (text, variants) = edited_text_and_variants(&plain, Some("new".into()), None, None).unwrap();
    assert_eq!((text.as_str(), variants.as_slice()), ("new", &["v".to_string()][..]));
}

// An entry sealed under another vault's key shows as locked and refuses a save,
// so its original ciphertext survives until the right password opens it.
#[test]
fn a_theme_name_becomes_a_usable_export_file_name() {
    assert_eq!(export_file_name_from("Blue/Grey", "theme"), "Blue-Grey");
    assert_eq!(export_file_name_from("a:b*c?", "theme"), "a-b-c-");
    assert_eq!(export_file_name_from("Plain name", "theme"), "Plain name");
    assert_eq!(export_file_name_from("..", "theme"), "theme");
    assert_eq!(export_file_name_from("CON", "theme"), "theme");
    assert_eq!(export_file_name_from("Dots...", "theme"), "Dots");
    assert!(export_file_name_is_valid(&export_file_name_from("x\\y|z\"", "theme")));
}

#[test]
fn an_entry_sealed_with_another_key_stays_locked_and_refuses_an_edit() {
    let (mine, theirs) = ([7u8; 32], [9u8; 32]);
    let mut p = image_text_prompt("s", "");
    p.secret = true;
    p.text = secret_encrypt("their secret", &theirs);
    let slim = slim_prompts(std::slice::from_ref(&p), Some(mine));
    assert!(slim[0].locked && slim[0].text.is_empty() && slim[0].variants.is_empty());
    assert_eq!(edited_text_and_variants(&p, Some("typed".into()), None, Some(mine)), Err("secret-wrong-key".to_string()));
    assert_eq!(secret_decrypt(&p.text, &theirs).unwrap(), "their secret");
    let slim = slim_prompts(std::slice::from_ref(&p), Some(theirs));
    assert!(!slim[0].locked && slim[0].text == "their secret");
}

// The direct hotkey and auto-rotate writes must not carry the cached text.
#[test]
fn the_direct_prompt_writes_never_send_the_cached_text() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/hotkeys.js"))
        .unwrap()
        .replace("\r\n", "\n");
    for name in ["export async function commitPromptHotkeyDirect", "export async function toggleRotateAutoDirect"] {
        let a = src.find(name).expect(name);
        let body = &src[a..a + src[a..].find("\n}\n").unwrap()];
        let call = &body[body.find("invoke(\"update_prompt\"").unwrap()..];
        assert!(!call.contains("text: p.text") && !call.contains("variants:"), "{name} must send neither text nor entries");
    }
}

// The stat scan alone decides whether anything needs reading; with nothing to
// read the poll returns before it copies the library.
#[test]
fn sync_scan_folder_asks_for_a_read_only_once_a_changed_file_has_settled() {
    let dir = std::env::temp_dir().join(format!("ps-scan-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("abc123.md");
    fs::write(&file, "hello").unwrap();
    fs::write(dir.join("AGENTS.md"), "readme").unwrap();
    fs::write(dir.join("notes.txt"), "not markdown").unwrap();
    // First sight: the stamp is only remembered, nothing is read yet.
    let (to_read, files, _) = sync_scan_folder(&dir).unwrap();
    assert!(to_read.is_empty() && files == 1, "a file seen for the first time is not read yet");
    // Next round, stamp unchanged: now it settles into a read.
    let (to_read, _, _) = sync_scan_folder(&dir).unwrap();
    assert_eq!(to_read.len(), 1);
    assert_eq!(to_read[0].stem, "abc123");
    // Once handled, an unchanged folder asks for nothing.
    sync_seen_lock().insert(to_read[0].key.clone(), SyncSeen { stamp: to_read[0].stamp, handled: true, hash: 0 });
    let (to_read, _, _) = sync_scan_folder(&dir).unwrap();
    assert!(to_read.is_empty(), "an unchanged, handled folder needs no read and therefore no snapshot");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_known_current_holds_until_the_file_or_the_content_changes() {
    let dir = std::env::temp_dir().join(format!("ps-written-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("p1.md");
    let age = |f: &Path, secs: u64| {
        let h = fs::OpenOptions::new().write(true).open(f).unwrap();
        h.set_times(std::fs::FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(secs))).unwrap();
    };
    fs::write(&file, "one").unwrap();
    let h1 = sync_content_hash("one");
    assert!(!sync_file_known_current(&file, h1), "a file never noted is read once");
    // a stamp from the last couple of seconds is not trusted (coarse file system clocks)
    sync_file_note_current(&file, h1);
    assert!(!sync_file_known_current(&file, h1), "a fresh stamp is compared by content, not skipped");
    age(&file, 60);
    sync_file_note_current(&file, h1);
    assert!(sync_file_known_current(&file, h1), "unchanged file, same render: skipped");
    assert!(!sync_file_known_current(&file, sync_content_hash("two")), "a changed render is compared again");
    fs::write(&file, "edited outside").unwrap();
    assert!(!sync_file_known_current(&file, h1), "an outside edit changes the stamp and is never skipped");
    // same length, mtime restored to the noted value: a settled stamp still matches
    // by design, but a differing length never does
    fs::write(&file, "edited outside!!").unwrap();
    age(&file, 60);
    assert!(!sync_file_known_current(&file, h1), "a different length is never trusted");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn trash_rows_carry_what_the_trash_list_draws_and_no_picture() {
    let mut p = test_prompt("t1");
    p.name = "Report".into();
    p.text = "x".repeat(TRASH_PREVIEW_CHARS + 50);
    p.image = "data:image/png;base64,AAAA".into();
    p.file_path = "C:/files/a.pdf".into();
    p.trashed_at = 1234;
    let mut s = test_prompt("t2");
    s.secret = true;
    s.text = "ciphertext-that-must-not-show".into();
    let rows = trash_rows(&[p, s]);
    let json = serde_json::to_string(&rows).unwrap();
    assert!(!json.contains("base64,AAAA") && !json.contains("\"image\""), "no picture in the trash payload: {json}");
    assert_eq!(rows[0].id, "t1");
    assert_eq!(rows[0].name, "Report");
    assert_eq!(rows[0].text.chars().count(), TRASH_PREVIEW_CHARS);
    assert_eq!(rows[0].file_path, "C:/files/a.pdf");
    assert_eq!(rows[0].trashed_at, 1234);
    assert!(rows[0].has_image);
    assert_eq!(rows[1].text, "", "a confidential entry shows no stored text");
}

#[test]
#[cfg(windows)]
fn read_key_sidecar_at_refuses_a_file_past_the_size_cap() {
    let dir = std::env::temp_dir().join(format!("ps-keyread-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let small = dir.join("small.keys.json");
    fs::write(&small, br#"{"events":[],"gaps":[],"hold_ms":450}"#).unwrap();
    assert_eq!(read_key_sidecar_at(&small).map(|s| s.hold_ms), Some(450));
    let big = dir.join("big.keys.json");
    fs::write(&big, vec![b' '; (KEY_SIDECAR_MAX_COPY_BYTES + 1) as usize]).unwrap();
    assert!(read_key_sidecar_at(&big).is_none(), "an oversized sidecar is never read whole");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn video_folder_durations_keep_the_order_of_the_names_and_zero_a_missing_file() {
    let dir = std::env::temp_dir().join(format!("ps-durations-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let names: Vec<String> = (0..12).map(|i| format!("rec-missing-{i}.mp4")).collect();
    let got = video_folder_file_durations_ms_impl(None, &dir, &names, &dir);
    assert_eq!(got, vec![0u64; 12], "one value per name, unknown files as 0");
    // Structural: the command runs the probes on the blocking pool, not a runtime worker.
    let src = production_sources();
    assert_eq!(src.matches("async fn video_folder_file_durations_ms(").count(), 1, "one durations command");
    let at = src.find("async fn video_folder_file_durations_ms(").unwrap();
    assert!(src[at..at + 600].contains("spawn_blocking"), "the durations command must use spawn_blocking");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn video_folder_durations_skip_foreign_dirs_and_non_bare_names() {
    assert!(is_bare_recording_name("rec-1.mp4"));
    assert!(is_bare_recording_name("rec-1.MKV"));
    for bad in ["", "rec-1", "rec-1.txt", "x.mp4", "..\rec-1.mp4", "../rec-1.mp4", "sub/rec-1.mp4", "C:\rec-1.mp4", "rec-1.mp4\\", "rec-1.mp4:s"] {
        assert!(!is_bare_recording_name(bad), "{bad:?} must be refused");
    }
    let configured = std::env::temp_dir().join(format!("ps-durations-cfg-{}", std::process::id()));
    let foreign = std::env::temp_dir().join(format!("ps-durations-foreign-{}", std::process::id()));
    fs::create_dir_all(&configured).unwrap();
    fs::create_dir_all(&foreign).unwrap();
    // A refused entry reads 0 without any probe being spawned.
    let names = vec!["rec-a.mp4".to_string(), "..\rec-b.mp4".to_string()];
    assert_eq!(video_folder_file_durations_ms_impl(None, &foreign, &names, &configured), vec![0, 0]);
    assert_eq!(video_folder_file_durations_ms_impl(None, &configured, &names, &configured), vec![0, 0]);
    let _ = fs::remove_dir_all(&configured);
    let _ = fs::remove_dir_all(&foreign);
}

#[test]
fn dup_sources_keep_only_what_the_scan_reads_and_skip_confidential_entries() {
    let mut a = test_prompt("a");
    a.name = "Alpha".into();
    a.text = "some body".into();
    a.image = "data:image/png;base64,BBBB".into();
    a.tags = vec!["t".into()];
    a.variants = vec!["v".into()];
    let mut s = test_prompt("s");
    s.secret = true;
    let got = dup_sources(&[a, s]);
    assert_eq!(got.len(), 1, "the confidential entry is not scanned");
    assert_eq!((got[0].id.as_str(), got[0].name.as_str(), got[0].text.as_str()), ("a", "Alpha", "some body"));
}

#[test]
fn take_still_output_removes_a_partial_frame_after_a_failed_run() {
    let dir = std::env::temp_dir().join(format!("ps-still-fail-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let partial = dir.join(format!("{}-still-1-2.png", brand::TEMP_PREFIX));
    fs::write(&partial, b"half a frame").unwrap();
    assert!(take_still_output(&partial, false).is_err());
    assert!(!partial.exists(), "a failed extraction must not leave its partial frame");
    let good = dir.join(format!("{}-still-1-3.png", brand::TEMP_PREFIX));
    fs::write(&good, b"frame").unwrap();
    assert_eq!(take_still_output(&good, true).unwrap(), b"frame");
    assert!(!good.exists(), "a read frame is removed as before");
    let _ = fs::remove_dir_all(&dir);
}

// A temp root of its own for one sweep test. The sweeps look at every file
// carrying the app's prefixes, so a folder shared between tests (or between two
// test runs, or left over from an earlier one) lets one test's files change
// another's counts; a fresh folder per test keeps each sweep to its own files.
pub(super) fn sweep_test_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("ps-sweeptest-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn sweep_stale_temp_files_also_reclaims_an_abandoned_still_frame() {
    let root = sweep_test_root("still");
    let sub = root.join(brand::TEMP_DIR);
    fs::create_dir_all(&sub).unwrap();
    let pid = std::process::id();
    let old = sub.join(format!("{}-still-{}-sweeptest-old.png", brand::TEMP_PREFIX, pid));
    let fresh = sub.join(format!("{}-still-{}-sweeptest-fresh.png", brand::TEMP_PREFIX, pid));
    fs::write(&old, b"x").unwrap();
    fs::write(&fresh, b"x").unwrap();
    backdate(&old, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    sweep_stale_temp_files_in(&root);
    assert!(!old.exists(), "a stale still frame must be swept");
    assert!(fresh.exists(), "a young still frame may belong to a running extraction");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn remove_own_temp_leftovers_takes_only_this_pids_files() {
    let root = std::env::temp_dir().join(format!("ps-ownexit-{}", std::process::id()));
    let dir = root.join(brand::TEMP_DIR);
    let mine_copy = dir.join(format!("{}-openedit-20260101-1000-4242", brand::TEMP_PREFIX));
    let other_copy = dir.join(format!("{}-openedit-20260101-1000-4243", brand::TEMP_PREFIX));
    fs::create_dir_all(&mine_copy).unwrap();
    fs::create_dir_all(&other_copy).unwrap();
    fs::write(mine_copy.join("a.png"), b"x").unwrap();
    let mine_still = dir.join(format!("{}-still-4242-9.png", brand::TEMP_PREFIX));
    let other_still = dir.join(format!("{}-still-4243-9.png", brand::TEMP_PREFIX));
    fs::write(&mine_still, b"x").unwrap();
    fs::write(&other_still, b"x").unwrap();
    remove_own_temp_leftovers_in(&root, 4242);
    assert!(!mine_copy.exists() && !mine_still.exists(), "this process's own scratch files go on exit");
    assert!(other_copy.exists() && other_still.exists(), "another instance's files stay");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn sweep_stale_temp_files_removes_only_its_own_stale_leftovers() {
    let dir = sweep_test_root("own-stale");
    let pid = std::process::id();
    let snip = dir.join(format!("{}-snip-{}.jpg", brand::TEMP_PREFIX, pid));
    let update_dir = dir.join(format!("{}-update-{}-1", brand::SLUG, pid));
    let wv2_dir = dir.join(format!("{}-wv2-{}-1", brand::SLUG, pid));
    let fresh_snip = dir.join(format!("{}-snip-{}-fresh.jpg", brand::TEMP_PREFIX, pid));
    let unrelated_ext = dir.join(format!("{}-snip-{}.png", brand::TEMP_PREFIX, pid)); // wrong extension
    let unrelated_file = dir.join(format!("ps-sweep-unrelated-{}.txt", pid));
    let ocr_subdir = dir.join(brand::TEMP_DIR);
    let ocr_png = ocr_subdir.join(format!("{}-ocr-{}.png", brand::TEMP_PREFIX, pid));
    let ocr_json = ocr_subdir.join(format!("{}-ocr-install-{}.json", brand::TEMP_PREFIX, pid));
    let ocr_script = ocr_subdir.join(format!("{}-ocr-install-{}.ps1", brand::TEMP_PREFIX, pid));
    let caps_json = ocr_subdir.join(format!("{}-ocr-caps.json", brand::TEMP_PREFIX));
    let fresh_ocr_png = ocr_subdir.join(format!("{}-ocr-{}-fresh.png", brand::TEMP_PREFIX, pid));
    let pdfium_cache = ocr_subdir.join(format!("pdfium-test-{}.dll", pid));

    fs::write(&snip, b"jpeg").unwrap();
    fs::create_dir_all(&update_dir).unwrap();
    fs::write(update_dir.join("setup.exe"), b"x").unwrap();
    fs::create_dir_all(&wv2_dir).unwrap();
    fs::write(wv2_dir.join("MicrosoftEdgeWebview2Setup.exe"), b"x").unwrap();
    fs::write(&fresh_snip, b"jpeg").unwrap();
    fs::write(&unrelated_ext, b"x").unwrap();
    fs::write(&unrelated_file, b"x").unwrap();
    fs::create_dir_all(&ocr_subdir).unwrap();
    fs::write(&ocr_png, b"png").unwrap();
    fs::write(&ocr_json, b"{}").unwrap();
    fs::write(&ocr_script, b"# elevated install").unwrap();
    fs::write(&caps_json, b"[]").unwrap();
    fs::write(&fresh_ocr_png, b"png").unwrap();
    fs::write(&pdfium_cache, b"dll").unwrap();
    backdate(&snip, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&update_dir, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&wv2_dir, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&ocr_png, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&ocr_json, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&ocr_script, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&caps_json, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    backdate(&pdfium_cache, STALE_TEMP_FILE_AGE + Duration::from_secs(60));

    sweep_stale_temp_files_in(&dir);

    assert!(!snip.exists(), "a stale leftover snip preview must be swept");
    assert!(!update_dir.exists(), "a stale leftover update stage dir must be swept");
    assert!(!wv2_dir.exists(), "a stale leftover WebView2 bootstrapper dir must be swept");
    assert!(!ocr_png.exists(), "a stale leftover OCR image must be swept");
    assert!(!ocr_json.exists(), "a stale leftover OCR install-result json must be swept");
    assert!(!ocr_script.exists(), "a stale leftover OCR elevation script must be swept");
    assert!(!caps_json.exists(), "a stale leftover elevated capability listing must be swept");
    assert!(fresh_snip.exists(), "a snip preview still in active use must survive");
    assert!(fresh_ocr_png.exists(), "an OCR image still in active use must survive");
    assert!(unrelated_ext.exists(), "a differently-suffixed file must survive");
    assert!(unrelated_file.exists(), "an unrelated file must survive");
    assert!(pdfium_cache.exists(), "the pdfium dll cache must never be touched by this sweep");

    let _ = fs::remove_dir_all(&dir);
}

// The clip-export sweep has its own configurable age, separate from
// STALE_TEMP_FILE_AGE: must only touch its own prefix, and must respect
// whatever age it is called with rather than the fixed constant above.
#[test]
fn sweep_clip_export_temp_respects_its_own_configurable_age() {
    let root = sweep_test_root("clipage");
    let dir = root.join(brand::TEMP_DIR);
    fs::create_dir_all(&dir).unwrap();
    let pid = std::process::id();
    let stale = dir.join(format!("{}-clipexport-{}-stale.mp4", brand::TEMP_PREFIX, pid));
    let fresh = dir.join(format!("{}-clipexport-{}-fresh.mp4", brand::TEMP_PREFIX, pid));
    let unrelated = dir.join(format!("{}-ocr-{}-unrelated.png", brand::TEMP_PREFIX, pid));
    fs::write(&stale, b"mp4").unwrap();
    fs::write(&fresh, b"mp4").unwrap();
    fs::write(&unrelated, b"png").unwrap();
    let age = Duration::from_secs(120);
    backdate(&stale, age + Duration::from_secs(5));
    backdate(&fresh, Duration::from_secs(5));
    backdate(&unrelated, age + Duration::from_secs(5));

    let (count, bytes) = sweep_clip_export_temp_in(&dir, age);

    assert!(!stale.exists(), "a clip-export file older than the given age must be swept");
    assert!(fresh.exists(), "a clip-export file younger than the given age must survive");
    assert!(unrelated.exists(), "a differently-prefixed file must never be touched by this sweep");
    assert_eq!(count, 1, "the swept count must report exactly the one stale file");
    assert_eq!(bytes, 3, "the swept bytes must report that one file's own size");
    let _ = fs::remove_dir_all(&root);
}

// The overview row's amount readout counts every clip-export file
// regardless of age: unlike the sweep above, a fresh file still counts.
#[test]
fn clip_export_temp_stats_counts_every_clip_export_file_regardless_of_age() {
    let root = sweep_test_root("clipstats");
    let dir = root.join(brand::TEMP_DIR);
    fs::create_dir_all(&dir).unwrap();
    let pid = std::process::id();
    let old = dir.join(format!("{}-clipexport-{}-statsold.mp4", brand::TEMP_PREFIX, pid));
    let young = dir.join(format!("{}-clipexport-{}-statsyoung.mp4", brand::TEMP_PREFIX, pid));
    let unrelated = dir.join(format!("{}-ocr-{}-statsunrelated.png", brand::TEMP_PREFIX, pid));
    fs::write(&old, b"mp4data").unwrap();
    fs::write(&young, b"mp4").unwrap();
    fs::write(&unrelated, b"png").unwrap();
    backdate(&old, Duration::from_secs(999_999));

    let (count, bytes) = clip_export_temp_stats_in(&dir);

    assert_eq!(count, 2, "both clip-export files, and only those, must be counted regardless of age");
    assert_eq!(bytes, 10, "bytes must sum both clip-export files own sizes");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn clip_export_path_impl_lands_in_temp_dir_with_a_safe_extension() {
    let path = clip_export_path_impl("mp4").unwrap();
    let dir = std::env::temp_dir().join(brand::TEMP_DIR);
    assert!(
        std::path::Path::new(&path).starts_with(&dir),
        "must be created inside the app's own temp subfolder"
    );
    assert!(path.ends_with(".mp4"));
    assert!(path.contains("clipexport"));
    // A hostile/garbage extension (path separators, empty) must never leak
    // into the filename: falls back to mp4 instead.
    let fallback = clip_export_path_impl("../evil").unwrap();
    assert!(fallback.ends_with(".mp4"), "an unsafe extension must fall back to mp4");
    let empty = clip_export_path_impl("").unwrap();
    assert!(empty.ends_with(".mp4"), "an empty extension must fall back to mp4");
}

#[test]
fn clip_export_ttl_is_one_hour() {
    // Never zero: a sweep with a zero age would delete a file the instant
    // it is written, mid-export.
    assert_eq!(clip_export_ttl(), Duration::from_secs(3600));
}

#[test]
fn video_folder_stats_counts_only_known_video_extensions() {
    let dir = std::env::temp_dir().join(format!("ps-video-folder-stats-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("rec-a.mp4"), b"1234").unwrap();
    fs::write(dir.join("rec-b.webm"), b"12").unwrap();
    fs::write(dir.join("rec-c.gif"), b"123").unwrap();
    fs::write(dir.join("notes.txt"), b"ignored, wrong extension").unwrap();
    fs::create_dir_all(dir.join("MP4")).unwrap(); // a directory named like an extension is not a file
    let (count, bytes) = video_folder_stats_impl(&dir.to_string_lossy(), &dir);
    assert_eq!(count, 3, "only the three video files must be counted, not the txt or the subfolder");
    assert_eq!(bytes, 4 + 2 + 3);
    let _ = fs::remove_dir_all(&dir);
}

// Finding 3 (security pass 348): the extension check alone let this count
// and clean up any matching file in a shared folder, not only the app's own
// recordings: the same "rec-" naming proof discard already requires.
#[test]
fn video_folder_stats_ignores_a_video_file_without_the_rec_prefix() {
    let dir = std::env::temp_dir().join(format!("ps-video-folder-noprefix-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("rec-mine.mp4"), b"1234").unwrap();
    fs::write(dir.join("someone-elses-video.mp4"), b"999999").unwrap();
    let (count, bytes) = video_folder_stats_impl(&dir.to_string_lossy(), &dir);
    assert_eq!(count, 1, "only the app's own rec-prefixed file must be counted");
    assert_eq!(bytes, 4);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn video_folder_stats_on_a_missing_folder_is_empty_not_an_error() {
    let dir = std::env::temp_dir().join(format!("ps-video-folder-missing-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir); // guarantee it really does not exist
    assert_eq!(video_folder_stats_impl(&dir.to_string_lossy(), &dir), (0, 0));
}

#[test]
fn a_file_written_before_the_view_id_existed_does_not_read_as_an_outside_edit() {
    let p = sync_test_prompt();
    let placement = SyncPlacement { view: "Main".into(), view_id: "v1".into(), grid: "5x4".into(), cell: "1,1".into(), ..Default::default() };
    let old_form = prompt_to_md_form(&p, &placement, false, false);
    assert!(!old_form.contains("view_id"));
    assert_ne!(old_form, prompt_to_md(&p, &placement));
    assert!(prompt_to_md(&p, &placement).contains("view_id: v1\n"));
    assert!(sync_md_matches(&p, &placement, &old_form));
}

// ---- a queued write waits out a long first read of the folder ----

#[test]
fn a_write_held_back_by_a_long_boot_read_is_not_given_up() {
    let (mut retries, mut waited) = (0u32, 0u64);
    // Twenty-three refused rounds in a row, far more than SYNC_WRITE_RETRIES.
    for round in 0..23 {
        assert!(sync_write_again(1, true, &mut retries, &mut waited), "gave up after {round} refused rounds");
    }
    assert_eq!(retries, 0, "waiting for the boot read used up retries");
    // The read ends and the write goes through.
    assert!(!sync_write_again(0, false, &mut retries, &mut waited));
    assert_eq!((retries, waited), (0, 0));
}

#[test]
fn the_wait_for_the_boot_read_is_bounded_and_then_the_normal_retries_apply() {
    let (mut retries, mut waited) = (0u32, 0u64);
    let mut rounds = 0;
    while sync_write_again(1, true, &mut retries, &mut waited) {
        rounds += 1;
        assert!(rounds < 1000, "the wait never ends");
    }
    assert_eq!(rounds as u64, SYNC_BOOT_WAIT_SECS / SYNC_POLL_SECS + SYNC_WRITE_RETRIES as u64);
}

#[test]
fn a_failed_write_outside_the_boot_window_still_gets_only_the_normal_retries() {
    let (mut retries, mut waited) = (0u32, 0u64);
    for _ in 0..SYNC_WRITE_RETRIES {
        assert!(sync_write_again(2, false, &mut retries, &mut waited));
    }
    assert!(!sync_write_again(2, false, &mut retries, &mut waited));
    assert_eq!(waited, 0);
}

// The working copy of a file opened for editing keeps its folder time from the moment it was
// copied; the sweep must not take it away from an editor that is still running.
#[test]
fn sweep_stale_temp_files_keeps_working_copies_of_living_processes() {
    let root = sweep_test_root("openedit");
    let sub = root.join(brand::TEMP_DIR);
    let mine = sub.join(format!("{}-openedit-20260101-1000-{}", brand::TEMP_PREFIX, std::process::id()));
    let gone = sub.join(format!("{}-openedit-20260101-1000-4000000001", brand::TEMP_PREFIX));
    for d in [&mine, &gone] {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("clip.mp4"), b"x").unwrap();
        backdate(d, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    }
    sweep_stale_temp_files_in(&root);
    assert!(mine.join("clip.mp4").exists(), "a long edit lost its source file");
    assert!(!gone.exists(), "a copy whose process is gone is still swept");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_folder_switch_that_wrote_nothing_says_so_with_or_without_a_delete() {
    for delete_old in [false, true] {
        let ended = sync_dir_switch_early_end(3, delete_old).expect("a failed write ends the switch");
        let err = ended.expect_err("a failed write is an error, not a quiet success");
        assert!(err.contains("3 files"), "the count is named: {err}");
    }
    assert_eq!(sync_dir_switch_early_end(0, false), Some(Ok(0)), "nothing to delete: done");
    assert_eq!(sync_dir_switch_early_end(0, true), None, "the delete is next");
}
