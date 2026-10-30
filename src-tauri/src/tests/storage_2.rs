use super::*;

// The running session's own output is excluded unconditionally, not by
// age or by moov presence: a recording in progress has no moov either.
#[test]
fn sweep_stale_recordings_in_never_touches_the_active_path() {
    let dir = std::env::temp_dir().join(format!("ps-test-active-sweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let active = dir.join("2026-09-04_10-00-00.mp4");
    fs::write(&active, b"not a real box structure").unwrap();
    backdate(&active, Duration::from_secs(3600));

    let empty = std::collections::HashSet::new();
    let (count, bytes) = sweep_stale_recordings_in(&dir, &empty, std::slice::from_ref(&active), true, Duration::from_secs(60), None);

    assert_eq!(count, 0);
    assert_eq!(bytes, 0);
    assert!(active.exists(), "the currently recording session's own file must survive every sweep");

    let _ = fs::remove_dir_all(&dir);
}

// A camera take abandoned after a failed chunk write keeps its partial
// file (and its marker) through every sweep, however old; an ordinary
// stale file next to it is still removed.
#[test]
fn sweep_stale_recordings_in_keeps_an_abandoned_camera_take() {
    let dir = std::env::temp_dir().join(format!("ps-test-abandoned-sweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let kept = dir.join("rec-cam-2026-09-29_100000.webm");
    let marker = camera::abandoned_marker_path(&kept);
    let stale = dir.join("rec-cam-2026-09-29_090000.webm");
    for p in [&kept, &marker, &stale] {
        fs::write(p, b"partial").unwrap();
        backdate(p, Duration::from_secs(3 * 24 * 3600));
    }

    let empty = std::collections::HashSet::new();
    let (count, _) = sweep_stale_recordings_in(&dir, &empty, &[], true, Duration::from_secs(60), Some(Duration::from_secs(3600)));

    assert_eq!(count, 1);
    assert!(kept.exists(), "the abandoned take must survive the sweep");
    assert!(marker.exists(), "its marker must survive too");
    assert!(!stale.exists(), "an ordinary stale file is still swept");

    let _ = fs::remove_dir_all(&dir);
}

// A broken (moov-less) mp4 past the short grace period is removed even
// though it is nowhere near STALE_TEMP_FILE_AGE: the whole point of 281.
#[test]
fn sweep_stale_recordings_in_removes_broken_past_grace_period() {
    let dir = std::env::temp_dir().join(format!("ps-test-broken-sweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let broken = dir.join("2026-09-04_11-00-00.mp4");
    fs::write(&broken, b"not a real box structure").unwrap();
    backdate(&broken, Duration::from_secs(10 * 60));
    let healthy_young = dir.join("2026-09-04_11-05-00.mp4");
    let mut moov = Vec::new();
    moov.extend_from_slice(&8u32.to_be_bytes());
    moov.extend_from_slice(b"moov");
    fs::write(&healthy_young, &moov).unwrap();
    backdate(&healthy_young, Duration::from_secs(10 * 60));

    let empty = std::collections::HashSet::new();
    let (count, bytes) = sweep_stale_recordings_in(&dir, &empty, &[], true, Duration::from_secs(5 * 60), None);

    assert_eq!(count, 1);
    assert!(bytes > 0);
    assert!(!broken.exists(), "a broken mp4 past the grace period must be swept");
    assert!(healthy_young.exists(), "a healthy mp4 well under STALE_TEMP_FILE_AGE must survive");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sweep_stale_pdfium_dlls_keeps_only_the_current_version() {
    let dir = std::env::temp_dir().join(format!("ps-test-pdfium-sweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let current = dir.join("pdfium-2.7.0.dll");
    let older = dir.join("pdfium-2.6.3.dll");
    let unrelated = dir.join("pdfium-LICENSE");
    fs::write(&current, b"current").unwrap();
    fs::write(&older, b"stale").unwrap();
    fs::write(&unrelated, b"license text").unwrap();

    sweep_stale_pdfium_dlls(&current);

    assert!(current.exists(), "the current version's own DLL must survive its own sweep");
    assert!(!older.exists(), "a previous version's DLL must be removed");
    assert!(unrelated.exists(), "a same-prefix non-.dll file must survive");

    let _ = fs::remove_dir_all(&dir);
}

// A fallback file that exists but cannot be decrypted/parsed (DPAPI from a
// different Windows account, truncated write) must never be silently treated as
// "nothing here": that used to erase the whole DB on the next load_store.
#[test]
fn sealed_json_distinguishes_missing_from_unreadable() {
    let dir = std::env::temp_dir().join(format!("ps-sealed-missing-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);

    let missing = dir.join("nope.json");
    assert!(matches!(
        read_sealed_json::<Vec<String>>(&missing),
        SealedRead::Missing
    ));

    let garbled = dir.join("garbled.json");
    fs::write(&garbled, "not json at all {{{").expect("write garbled");
    assert!(matches!(
        read_sealed_json::<Vec<String>>(&garbled),
        SealedRead::Unreadable
    ));

    // A prefixed value that fails to decrypt yields "" from dec_str, which is
    // likewise not valid JSON: the exact path that used to silently default.
    let sealed_garbage = dir.join("sealed.json");
    fs::write(&sealed_garbage, format!("{}not-real-base64!!", ENC_PREFIX)).expect("write");
    assert!(matches!(
        read_sealed_json::<Vec<String>>(&sealed_garbage),
        SealedRead::Unreadable
    ));

    let _ = fs::remove_dir_all(&dir);
}

// A cache written by a build before BackupPrompt existed is a sealed
// Vec<String>: must read back as name-only items instead of an empty list,
// and the caller must be told to re-write it (once), not on every read.
#[test]
fn backup_names_cache_upgrades_a_legacy_string_list() {
    let dir = std::env::temp_dir().join(format!("ps-backup-names-legacy-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let cache = dir.join("prompts.names.json");

    let legacy = enc_str(&serde_json::to_string(&vec!["Draft reply", "Meeting notes"]).unwrap());
    fs::write(&cache, legacy).expect("write legacy cache");
    let (items, legacy) = read_prompt_names_cache(&cache);
    assert!(legacy);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].name, "Draft reply");
    assert_eq!(items[0].chars, 0);
    assert_eq!(items[0].kind, "text");

    // Current-shape cache round-trips without the migration flag.
    write_prompt_names(&cache, &items.iter().map(|i| BackupPrompt {
        name: i.name.clone(), chars: 42, kind: "image".into(),
    }).collect::<Vec<_>>());
    let (items2, legacy2) = read_prompt_names_cache(&cache);
    assert!(!legacy2);
    assert_eq!(items2[0].chars, 42);
    assert_eq!(items2[0].kind, "image");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn history_retention_default_and_explicit_values() {
    // A new store prunes after 30 days; stores that predate that default are
    // pinned to 0 (keep forever) by migrate(), so nobody loses history silently.
    let mut s = Settings::default();
    assert_eq!(history_max_age(&s), Some(30 * 86_400), "no historyDays set => the new-install default");
    s.ui_values.insert("historyDays".into(), 0.0);
    assert_eq!(history_max_age(&s), None, "0 => keep forever");
    s.ui_values.insert("historyDays".into(), 7.0);
    assert_eq!(history_max_age(&s), Some(7 * 86_400), "explicit days still prune");
}

#[test]
fn backup_fixed_key_roundtrip_is_portable() {
    // Default backups carry no password: any install must be able to restore them.
    let plain = br#"{"prompts":[],"settings":{}}"#;
    let blob = backup_encrypt(plain, None).expect("encrypt");
    assert_eq!(&blob[0..4], BACKUP_MAGIC, "header magic");
    assert_eq!(blob[4], 2, "mode 2 = fixed key + compressed");
    assert_ne!(&blob[5..], &plain[..], "payload is not stored in the clear");
    let out = backup_decrypt(&blob, None).expect("decrypt").expect("psb format");
    assert_eq!(out, plain);
}

#[test]
fn backup_password_roundtrip_and_wrong_password_fails() {
    let plain = b"secret prompts";
    let blob = backup_encrypt(plain, Some("correct horse")).expect("encrypt");
    assert_eq!(blob[4], 3, "mode 3 = password + compressed");
    let out = backup_decrypt(&blob, Some("correct horse")).expect("decrypt").expect("psb");
    assert_eq!(out, plain);
    assert!(backup_decrypt(&blob, Some("wrong")).is_err(), "wrong password rejected");
    assert!(backup_decrypt(&blob, None).is_err(), "missing password rejected");
    // The fixed key must NOT open a password-protected backup.
    assert!(backup_decrypt(&blob, Some("")).is_err());
}

#[test]
fn backup_is_due_fires_for_an_aged_backup_and_for_none_at_all() {
    assert!(backup_is_due(None, 24), "no backup on disk is always due");
    assert!(backup_is_due(Some(24 * 3600), 24), "exactly one interval old is due");
    assert!(backup_is_due(Some(25 * 3600), 24), "older than the interval is due");
}

#[test]
fn a_snapshot_stamped_in_the_future_does_not_hold_back_the_next_backup() {
    let now = 1_800_000_000i64;
    let day = 86_400i64;
    // Only a snapshot from a clock that ran a month ahead: nothing real, so a backup is due.
    assert!(backup_is_due(newest_stamp_age(now, &[now + 30 * day]), 24));
    // Beside a genuine old one, the old one decides the age.
    assert_eq!(newest_stamp_age(now, &[now + 30 * day, now - 2 * day]), Some(2 * day as u64));
    // Slack for local-time shifts: a stamp an hour ahead still counts as fresh.
    assert_eq!(newest_stamp_age(now, &[now + 3600]), Some(0));
    assert_eq!(newest_stamp_age(now, &[]), None);
}

#[test]
fn backup_is_due_stays_false_right_after_a_backup_just_ran() {
    // The same check that lets a due backup fire at start also keeps a
    // second app start moments later from taking a second one: it sees the
    // first start's backup as a few seconds old, nowhere near the interval.
    assert!(!backup_is_due(Some(0), 24));
    assert!(!backup_is_due(Some(30), 24), "30s old is nowhere near a 24h interval");
}

#[test]
fn retention_keeps_the_right_backups_and_never_all_of_them() {
    let names: Vec<String> = [
        "2024-03-01_02-00-00", // older year
        "2025-11-30_02-00-00", // older month
        "2026-07-20_02-00-00", // older week
        "2026-07-25_02-00-00",
        "2026-07-26_02-00-00", // newest
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    // Plain count: the oldest ones go, the newest `keep` stay.
    assert_eq!(backups_to_drop(&names, 2, None), names[..3].to_vec());
    assert!(backups_to_drop(&names, 99, None).is_empty());
    // A count of 0 must not be read as "delete everything".
    assert_eq!(backups_to_drop(&names, 0, None).len(), names.len() - 1);

    // Tiers count PERIODS, newest first: 3 years reach back to 2024, while one
    // day / week / month only ever pins the newest backup. `keep` is a floor, so
    // it has to stay at 1 here for the tiers alone to decide.
    let tiers = GfsConfig { daily: 1, weekly: 1, monthly: 1, yearly: 3 };
    let dropped = backups_to_drop(&names, 1, Some(&tiers));
    assert!(!dropped.contains(&names[4]), "the newest backup always survives");
    assert!(!dropped.contains(&names[1]), "second-newest year kept");
    assert!(!dropped.contains(&names[0]), "third-newest year kept");
    assert_eq!(dropped, vec![names[2].clone(), names[3].clone()], "no tier asks for these");

    // Every tier at zero must not wipe the lot: the newest one stays.
    let empty = GfsConfig { daily: 0, weekly: 0, monthly: 0, yearly: 0 };
    let dropped = backups_to_drop(&names, 1, Some(&empty));
    assert_eq!(dropped.len(), names.len() - 1);
    assert!(!dropped.contains(&names[4]));

    // Reserved placeholders are written with an @ after the upgrade, exactly once.
    assert_eq!(upgrade_placeholders("Hi {#{date}#} and {#{name}#}"), "Hi {?@date} and {?name}");
    assert_eq!(upgrade_placeholders("{?@date}"), "{?@date}", "already upgraded stays put");
    assert_eq!(upgrade_placeholders("plain text"), "plain text");
    // A choice variable that happens to start with a reserved word stays an ordinary variable.
    assert_eq!(upgrade_placeholders("{#{date|a,b}#}"), "{?date|a,b}");

    // The schedule reads the folder name, not a modification time.
    let a = stamp_secs("2026-07-29_08-00-00").unwrap();
    let b = stamp_secs("2026-07-29_10-30-00").unwrap();
    assert_eq!(b - a, 2 * 3600 + 30 * 60);
    assert_eq!(stamp_secs("2026-07-30_08-00-00").unwrap() - a, 86_400);
    assert!(stamp_secs("not-a-stamp").is_none());

    // `keep` survives the tiers: several backups on ONE day must not delete each
    // other just because the daily tier only pins one per date.
    let today: Vec<String> = ["2026-07-29_08-00-00", "2026-07-29_09-00-00", "2026-07-29_10-00-00"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let dropped = backups_to_drop(&today, 10, Some(&tiers));
    assert!(dropped.is_empty(), "same-day backups stay while `keep` has room");
    // With no room left, the tiers thin them out as before.
    let dropped = backups_to_drop(&today, 1, Some(&tiers));
    assert_eq!(dropped, today[..2].to_vec());
}

#[test]
fn backups_are_compressed_and_old_ones_still_open() {
    // Repetitive JSON is what a real backup looks like; it must shrink a lot.
    let plain = br#"{"prompts":[{"id":"a","name":"Review","text":"Review the code."}]}"#
        .repeat(40);
    let blob = backup_encrypt(&plain, None).expect("encrypt");
    assert_eq!(blob[4], 2, "mode 2 = fixed key + compressed");
    assert!(blob.len() * 4 < plain.len(), "compressed to well under a quarter");
    assert_eq!(backup_decrypt(&blob, None).expect("decrypt").expect("psb"), plain);

    // An uncompressed mode-0 file (written before 2.6.0) must still restore.
    let mut legacy = Vec::from(&BACKUP_MAGIC[..]);
    legacy.push(0);
    {
        let nonce = [7u8; 12];
        legacy.extend_from_slice(&nonce);
        legacy.extend_from_slice(&aead_seal(&BACKUP_FIXED_KEY, nonce, &plain[..]).unwrap());
    }
    assert_eq!(backup_decrypt(&legacy, None).expect("decrypt").expect("psb"), plain);
}

#[test]
fn password_errors_stay_recognisable_for_the_ui() {
    // The restore flow decides whether to show the password prompt by matching
    // "password" in the error text. Rewording these would silently disable it.
    let blob = backup_encrypt(b"x", Some("pw")).expect("encrypt");
    let missing = backup_decrypt(&blob, None).unwrap_err();
    let wrong = backup_decrypt(&blob, Some("nope")).unwrap_err();
    assert_eq!(missing, "password-required");
    for msg in [&missing, &wrong] {
        assert!(msg.to_lowercase().contains("password"), "must mention password: {msg}");
    }
    // A backup without a password must NOT trigger the prompt.
    let fixed = backup_encrypt(b"x", None).expect("encrypt");
    assert!(backup_decrypt(&fixed, None).is_ok(), "fixed-key backup opens silently");
}

// ---------- Confidential entries ----------

#[test]
fn secret_encrypt_then_decrypt_round_trips() {
    let key = secret_derive_key("correct horse", b"0123456789abcdef").expect("derive");
    let plain = "the real, sensitive text of this prompt";
    let ct = secret_encrypt(plain, &key);
    assert_ne!(ct, plain, "ciphertext must not equal the plain text");
    assert!(!ct.contains(plain), "ciphertext must not contain the plain text as a substring");
    assert_eq!(secret_decrypt(&ct, &key).expect("decrypt"), plain);
}

#[test]
fn secret_decrypt_with_wrong_password_fails_cleanly_without_touching_the_entry() {
    let salt = b"0123456789abcdef";
    let right_key = secret_derive_key("right password", salt).expect("derive");
    let wrong_key = secret_derive_key("wrong password", salt).expect("derive");
    let plain = "do not leak this";
    let ct = secret_encrypt(plain, &right_key);
    let err = secret_decrypt(&ct, &wrong_key).unwrap_err();
    assert_eq!(err, "wrong-password");
    // The stored ciphertext itself is untouched by the failed attempt and
    // still opens with the real key: a wrong password never destroys it.
    assert_eq!(secret_decrypt(&ct, &right_key).expect("decrypt"), plain);
}

#[test]
fn secret_derive_key_is_deterministic_and_password_sensitive() {
    let salt = b"0123456789abcdef";
    let a1 = secret_derive_key("same password", salt).expect("derive");
    let a2 = secret_derive_key("same password", salt).expect("derive");
    let b = secret_derive_key("different password", salt).expect("derive");
    assert_eq!(a1, a2, "same password + salt must derive the same key every time");
    assert_ne!(a1, b, "a different password must derive a different key");
}

#[test]
fn reveal_secret_text_passes_through_a_plain_prompt_unchanged() {
    let mut p = test_prompt("a");
    p.text = "plain text, never encrypted".into();
    let (text, variants) = reveal_secret_text(&p, None).expect("plain prompt never needs a key");
    assert_eq!(text, p.text);
    assert!(variants.is_empty());
}

#[test]
fn reveal_secret_text_refuses_a_confidential_prompt_with_no_key() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut p = test_prompt("a");
    p.secret = true;
    p.text = secret_encrypt("the real text", &key);
    let err = reveal_secret_text(&p, None).unwrap_err();
    assert_eq!(err, "secret-locked");
}

#[test]
fn reveal_secret_text_decrypts_a_confidential_prompt_and_its_variants_when_unlocked() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut p = test_prompt("a");
    p.secret = true;
    p.text = secret_encrypt("main entry", &key);
    p.variants = vec![secret_encrypt("entry two", &key), secret_encrypt("entry three", &key)];
    let (text, variants) = reveal_secret_text(&p, Some(key)).expect("decrypt");
    assert_eq!(text, "main entry");
    assert_eq!(variants, vec!["entry two".to_string(), "entry three".to_string()]);
}

#[test]
fn vault_first_setup_failed_save_leaves_no_salt_or_verifier() {
    let mut settings = Settings::default();
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let verify = secret_encrypt(SECRET_VERIFY_MARKER, &key);
    let r = secret_vault_first_setup(&mut settings, b"0123456789abcdef", verify, |_| false);
    assert_eq!(r, Err("save-failed".to_string()));
    assert!(!settings.ui_texts.contains_key(SECRET_SALT_KEY), "no salt may survive");
    assert!(!settings.ui_texts.contains_key(SECRET_VERIFY_KEY), "no verifier may survive");
}

#[test]
fn vault_first_setup_failed_save_restores_old_empty_values() {
    let mut settings = Settings::default();
    settings.ui_texts.insert(SECRET_SALT_KEY.into(), String::new());
    settings.ui_texts.insert(SECRET_VERIFY_KEY.into(), "old".into());
    let r = secret_vault_first_setup(&mut settings, b"0123456789abcdef", "new".into(), |_| false);
    assert!(r.is_err());
    assert_eq!(settings.ui_texts.get(SECRET_SALT_KEY).map(String::as_str), Some(""));
    assert_eq!(settings.ui_texts.get(SECRET_VERIFY_KEY).map(String::as_str), Some("old"));
}

#[test]
fn vault_first_setup_success_persists_and_rederives_the_same_key() {
    let mut settings = Settings::default();
    let salt = b"0123456789abcdef";
    let key = secret_derive_key("pw", salt).expect("derive");
    let verify = secret_encrypt(SECRET_VERIFY_MARKER, &key);
    let mut saved_salt = None;
    let r = secret_vault_first_setup(&mut settings, salt, verify, |s| {
        saved_salt = s.ui_texts.get(SECRET_SALT_KEY).cloned();
        true
    });
    assert_eq!(r, Ok(true));
    assert!(saved_salt.is_some(), "the salt was in the settings when they were saved");
    let salt_b64 = settings.ui_texts.get(SECRET_SALT_KEY).cloned().unwrap();
    let verify_b64 = settings.ui_texts.get(SECRET_VERIFY_KEY).cloned().unwrap();
    assert_eq!(secret_vault_verify("pw", &salt_b64, &verify_b64), Ok(key));
    assert_eq!(secret_vault_verify("other", &salt_b64, &verify_b64), Err("wrong-password".to_string()));
}

#[test]
fn vault_first_setup_never_replaces_an_existing_salt() {
    let mut settings = Settings::default();
    settings.ui_texts.insert(SECRET_SALT_KEY.into(), "existing".into());
    settings.ui_texts.insert(SECRET_VERIFY_KEY.into(), "existing-verify".into());
    let mut saves = 0;
    let r = secret_vault_first_setup(&mut settings, b"0123456789abcdef", "new".into(), |_| {
        saves += 1;
        true
    });
    assert_eq!(r, Ok(false));
    assert_eq!(saves, 0);
    assert_eq!(settings.ui_texts.get(SECRET_SALT_KEY).map(String::as_str), Some("existing"));
    assert_eq!(settings.ui_texts.get(SECRET_VERIFY_KEY).map(String::as_str), Some("existing-verify"));
}

pub(super) fn secret_test_store(prompts: Vec<Prompt>) -> Store {
    Store {
        prompts,
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: Default::default(),
    }
}

// A backfill pass is mid-recognition when the switch goes off and the clear runs:
// the text it returns afterwards must not land anywhere.
#[test]
fn a_pass_started_before_the_ocr_clear_never_writes_its_text() {
    let _epoch = OCR_EPOCH_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = secret_test_store(vec![image_text_prompt("a", "")]);
    store.settings.ui_flags.insert("ocrIndex".into(), true);
    let pic = store.prompts[0].image.clone();
    // sanity: an uninterrupted pass writes
    let started = ocr_epoch();
    assert!(apply_ocr_backfill_text(&mut store, "a", &pic, "first".into(), started));
    assert_eq!(store.prompts[0].image_text, "first");
    // a picture swapped while the pass ran blocks the write
    store.prompts[0].image_text.clear();
    assert!(!apply_ocr_backfill_text(&mut store, "a", "other-picture.png", "wrong".into(), started));
    assert!(store.prompts[0].image_text.is_empty());
    store.prompts[0].image_text = "first".into();
    // second pass: the recognizer "flips the flag off and clears" before returning
    let started = ocr_epoch();
    let recognize = |store: &mut Store| {
        store.settings.ui_flags.insert("ocrIndex".into(), false);
        assert!(clear_ocr_index_text(store));
        "late text".to_string()
    };
    let text = recognize(&mut store);
    assert!(!apply_ocr_backfill_text(&mut store, "a", &pic, text, started));
    assert!(store.prompts.iter().chain(store.trash.iter()).all(|p| p.image_text.is_empty()));
    // switched back on, but the stale pass (old epoch) still must not write
    store.settings.ui_flags.insert("ocrIndex".into(), true);
    assert!(!apply_ocr_backfill_text(&mut store, "a", &pic, "stale".into(), started));
    assert!(store.prompts[0].image_text.is_empty());
}

#[test]
fn a_failed_write_stops_the_ocr_backfill_and_takes_its_text_back_out() {
    let _epoch = OCR_EPOCH_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = secret_test_store(vec![image_text_prompt("a", "")]);
    store.settings.ui_flags.insert("ocrIndex".into(), true);
    let pic = store.prompts[0].image.clone();
    let started = ocr_epoch();
    let r = apply_ocr_backfill_and_save(&mut store, "a", &pic, "text".into(), started, |_| false);
    assert_eq!(r, Err("save-failed".to_string()));
    assert!(store.prompts[0].image_text.is_empty(), "memory matches the disk");
    let r = apply_ocr_backfill_and_save(&mut store, "a", &pic, "text".into(), started, |_| true);
    assert_eq!(r, Ok(()));
    assert_eq!(store.prompts[0].image_text, "text");
    // a stale pass writes nothing and asks for no save
    let r = apply_ocr_backfill_and_save(&mut store, "a", "other.png", "x".into(), started, |_| panic!("no write for a skipped text"));
    assert_eq!(r, Ok(()));
}

#[test]
fn a_refused_hotkey_is_cleared_only_when_both_writes_land() {
    let mut p = test_prompt("p1");
    p.hotkey = "Ctrl+Alt+1".into();
    let mut store = secret_test_store(vec![p]);
    store.settings.hotkey = "Ctrl+Alt+A".into();
    store.settings.voice_hotkey = "Ctrl+Alt+V".into();
    let dropped = vec!["p1".to_string()];

    let ok = clear_refused_hotkeys(&mut store, false, false, &dropped, &mut |_| true, &mut |_| true);
    assert!(ok);
    assert!(store.settings.hotkey.is_empty() && store.settings.voice_hotkey.is_empty() && store.prompts[0].hotkey.is_empty());

    store.settings.hotkey = "Ctrl+Alt+A".into();
    store.settings.voice_hotkey = "Ctrl+Alt+V".into();
    store.prompts[0].hotkey = "Ctrl+Alt+1".into();
    let ok = clear_refused_hotkeys(&mut store, false, false, &dropped, &mut |_| true, &mut |_| false);
    assert!(!ok);
    assert_eq!(store.settings.hotkey, "Ctrl+Alt+A", "a failed settings write keeps the key");
    assert_eq!(store.settings.voice_hotkey, "Ctrl+Alt+V");
    assert_eq!(store.prompts[0].hotkey, "Ctrl+Alt+1");

    let mut settings_calls = 0;
    let ok = clear_refused_hotkeys(&mut store, true, true, &dropped, &mut |_| false, &mut |_| {
        settings_calls += 1;
        true
    });
    assert!(!ok);
    assert_eq!(store.prompts[0].hotkey, "Ctrl+Alt+1", "a failed prompt write keeps the key");
    assert_eq!(settings_calls, 0, "settings are not written after a failed prompt write");
}

pub(super) fn prompts_json(store: &Store) -> String {
    serde_json::to_string(&store.prompts).unwrap()
}

#[test]
fn set_secret_decrypt_with_a_corrupt_middle_variant_changes_nothing() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut p = test_prompt("a");
    p.secret = true;
    p.text = secret_encrypt("main", &key);
    p.variants = vec![secret_encrypt("one", &key), "not-a-ciphertext".into(), secret_encrypt("three", &key)];
    let mut store = secret_test_store(vec![p]);
    let before = prompts_json(&store);
    let mut saved = false;
    let r = apply_prompt_secret(&mut store, "a", false, &key, |_| { saved = true; true });
    assert!(r.is_err());
    assert!(!saved, "nothing may be persisted after a failed conversion");
    assert_eq!(prompts_json(&store), before, "prompt must stay byte for byte unchanged");
    assert!(store.prompts[0].secret);
}

#[test]
fn older_versions_of_a_prompt_made_confidential_are_sealed_with_the_vault_key() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut conn = Connection::open_in_memory().unwrap();
    assert!(db_conn_init_schema(&conn));
    db_add_version(&conn, "a", 1, "Name", "old api key", &["old variant".to_string()], 20).unwrap();
    db_add_version(&conn, "b", 2, "Other", "other prompt", &[], 20).unwrap();
    assert_eq!(db_seal_versions(&mut conn, "a", &key).unwrap(), 1);
    let raw: (String, String) = conn
        .query_row("SELECT text, variants FROM prompt_versions WHERE prompt_id = 'a'", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    let (text, variants) = (dec_str(&raw.0), dec_str(&raw.1));
    assert!(!text.contains("old api key") && !variants.contains("old variant"));
    let v = db_list_versions(&conn, "a");
    let (t, vars) = open_snapshot(&v[0].text, &v[0].variants, Some(key)).unwrap();
    assert_eq!((t.as_str(), vars), ("old api key", vec!["old variant".to_string()]));
    assert_eq!(db_list_versions(&conn, "b")[0].text, "other prompt", "other prompts stay as they were");
    // Running it again must not seal twice.
    db_seal_versions(&mut conn, "a", &key).unwrap();
    let v = db_list_versions(&conn, "a");
    assert_eq!(open_snapshot(&v[0].text, &v[0].variants, Some(key)).unwrap().0, "old api key");
}

#[test]
fn set_secret_save_failure_restores_the_prompt_in_both_directions() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut plain = test_prompt("a");
    plain.text = "plain main".into();
    plain.variants = vec!["v1".into(), "v2".into()];
    let mut store = secret_test_store(vec![plain]);
    let before = prompts_json(&store);
    let r = apply_prompt_secret(&mut store, "a", true, &key, |_| false);
    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(prompts_json(&store), before);

    let mut sealed = test_prompt("b");
    sealed.secret = true;
    sealed.text = secret_encrypt("main", &key);
    sealed.variants = vec![secret_encrypt("v1", &key)];
    let mut store = secret_test_store(vec![sealed]);
    let before = prompts_json(&store);
    let r = apply_prompt_secret(&mut store, "b", false, &key, |_| false);
    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(prompts_json(&store), before);
}

#[test]
fn set_secret_success_changes_text_variants_and_flag_together() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut plain = test_prompt("a");
    plain.text = "plain main".into();
    plain.variants = vec!["v1".into(), "v2".into()];
    let mut store = secret_test_store(vec![plain]);
    apply_prompt_secret(&mut store, "a", true, &key, |_| true).expect("encrypt");
    assert!(store.prompts[0].secret);
    assert_ne!(store.prompts[0].text, "plain main");
    assert_eq!(secret_decrypt(&store.prompts[0].variants[1], &key).unwrap(), "v2");
    apply_prompt_secret(&mut store, "a", false, &key, |_| true).expect("decrypt");
    assert!(!store.prompts[0].secret);
    assert_eq!(store.prompts[0].text, "plain main");
    assert_eq!(store.prompts[0].variants, vec!["v1".to_string(), "v2".to_string()]);
}

#[test]
fn secret_encrypt_if_plain_never_double_encrypts_an_already_sealed_value() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let plain = "a version-history snapshot from before this entry was confidential";
    // A snapshot taken while NOT yet confidential: plain text, must be encrypted.
    let sealed_once = secret_encrypt_if_plain(plain, &key);
    assert_ne!(sealed_once, plain);
    assert_eq!(secret_decrypt(&sealed_once, &key).expect("decrypt"), plain);
    // A snapshot taken while ALREADY confidential: already ciphertext, must be
    // kept exactly as-is rather than encrypted a second time (which would make
    // it decrypt back to the ciphertext string, not the original text).
    let already_sealed = secret_encrypt("already confidential text", &key);
    let kept = secret_encrypt_if_plain(&already_sealed, &key);
    assert_eq!(kept, already_sealed, "must not double-encrypt");
    assert_eq!(secret_decrypt(&kept, &key).expect("decrypt"), "already confidential text");
}

#[test]
fn a_confidential_prompt_never_serializes_its_plain_text_to_disk() {
    // save_prompts ultimately serializes the same Prompt struct this builds --
    // this proves the plain text is nowhere in that serialized form once the
    // entry is confidential, only the ciphertext (and the unrelated name).
    let key = secret_derive_key("vault password", b"0123456789abcdef").expect("derive");
    let secret_value = "the confidential text nobody but the vault password should reveal";
    let mut p = test_prompt("a");
    p.name = "Ordinary visible name".into();
    p.secret = true;
    p.text = secret_encrypt(secret_value, &key);
    let json = serde_json::to_string(&p).expect("serialize");
    assert!(!json.contains(secret_value), "plain text must never appear in the serialized prompt");
    assert!(json.contains(&p.name), "the name stays plain on purpose");
    assert!(json.contains("\"secret\":true"));
}

#[test]
fn export_filter_keeps_or_drops_confidential_prompts_on_request() {
    let mut open = test_prompt("open");
    open.secret = false;
    let mut locked = test_prompt("locked");
    locked.secret = true;
    let prompts = vec![open.clone(), locked.clone()];

    let excluded = filter_export_secrets(prompts.clone(), false);
    assert_eq!(excluded.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["open"]);

    let included = filter_export_secrets(prompts, true);
    assert_eq!(included.len(), 2, "both choices must be honoured, including keeping it");
}

#[test]
fn markdown_sync_never_picks_up_a_confidential_prompt() {
    let mut p = test_prompt("a");
    assert!(sync_eligible(&p), "a plain prompt with a safe id is eligible");
    p.secret = true;
    assert!(!sync_eligible(&p), "a confidential prompt must never be synced as plain text");
}

#[test]
fn reset_settings_leaves_a_usable_state() {
    // reset_settings replaces the settings with Settings::default() + migrate().
    // If that produced no view, the first grid mutation would index views[0] and
    // panic (the same trap import_all guards against), leaving a dead app.
    let mut s = Settings::default();
    s.migrate();
    assert!(!s.views.is_empty(), "reset must leave at least one view");
    assert!(
        s.views.iter().any(|v| v.id == s.active_view),
        "active_view must point at an existing view"
    );
    assert_eq!(s.views[0].cols, default_cols());
    assert_eq!(s.views[0].rows, default_rows());
    assert_eq!(s.theme, default_theme(), "theme back to default");
    assert!(s.ui_flags.is_empty() && s.ui_values.is_empty(), "expert tweaks cleared");
}

#[test]
fn a_failed_settings_write_keeps_the_old_settings_and_the_corrupt_row_lock_on_reset() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut settings = Settings::default();
    settings.migrate();
    settings.ui_texts.insert("marker".into(), "OLD".into());
    let mut store = Store { prompts: vec![test_prompt("keep")], trash: Vec::new(), settings, prompts_locked: false, written: Default::default() };
    SETTINGS_LOCKED.store(true, Ordering::Relaxed);

    let r = reset_settings_core(&mut store, |_| false);

    assert_eq!(r.map(|_| ()), Err("save-failed".to_string()));
    assert_eq!(store.settings.ui_texts.get("marker").map(String::as_str), Some("OLD"), "memory keeps the old settings");
    assert!(SETTINGS_LOCKED.load(Ordering::Relaxed), "the corrupt-row lock stays");
    assert_eq!(store.prompts.len(), 1, "the prompts are untouched");

    let ok = reset_settings_core(&mut store, |_| true);
    assert!(ok.is_ok());
    assert!(!store.settings.ui_texts.contains_key("marker"), "a written reset takes effect");
    assert!(!SETTINGS_LOCKED.load(Ordering::Relaxed), "a written reset releases the lock");
}

#[test]
fn a_failed_flush_aborts_the_switch_into_demo_mode() {
    assert_eq!(demo_flush_result(false, || true, || true), Ok(()));
    assert_eq!(demo_flush_result(false, || false, || true), Err("save-failed".to_string()));
    assert_eq!(demo_flush_result(false, || true, || false), Err("save-failed".to_string()));
    assert_eq!(demo_flush_result(true, || panic!("a locked prompt set is never written"), || true), Ok(()));
    assert_eq!(demo_flush_result(true, || true, || false), Err("save-failed".to_string()));
}

#[test]
fn a_failed_trash_restore_keeps_the_receiver_secrets_in_the_live_settings() {
    let mut settings = Settings::default();
    settings.migrate();
    let mut store = Store { prompts: Vec::new(), trash: Vec::new(), settings, prompts_locked: false, written: Default::default() };
    let mut p = test_prompt("trashed");
    p.id = "t1".into();
    p.trashed_at = 5;
    store.trash.push(p);
    store.settings.ui_texts.insert(BACKUP_PW_KEY.into(), "hash".into());
    store.settings.ui_texts.insert(SECRET_SALT_KEY.into(), "sealed".into());
    let r = restore_trashed_core(&mut store, "t1", |_| false);
    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(store.settings.ui_texts.get(BACKUP_PW_KEY).map(String::as_str), Some("hash"));
    assert_eq!(store.settings.ui_texts.get(SECRET_SALT_KEY).map(String::as_str), Some("sealed"));
    assert_eq!(store.trash.len(), 1);
    assert_eq!(store.trash[0].trashed_at, 5);
    assert!(store.prompts.is_empty());
}

pub(super) fn placement_fixture() -> Store {
    let mut settings = Settings::default();
    settings.migrate();
    settings.views = serde_json::from_value(serde_json::json!([
        {"id": "a", "name": "A", "cols": 4, "rows": 3,
        "members": ["p1", "p2", "p3", "p4", "p5"],
        "layouts": {"4x3": {"p3": [0, 0], "p4": [1, 0], "p5": [3, 0]}}},
        {"id": "b", "name": "B", "cols": 4, "rows": 3,
        "members": ["p3", "p6"],
        "layouts": {"4x3": {"p3": [1, 1], "p6": [0, 0]}}}
    ]))
    .unwrap();
    settings.active_view = "a".into();
    settings.tile_folders = serde_json::from_value(serde_json::json!([
        {"id": "tf", "name": "T", "view_id": "a", "cell": [2, 0], "members": ["p1", "p2"]}
    ]))
    .unwrap();
    settings.float_folders = serde_json::from_value(serde_json::json!([
        {"id": "ff", "name": "F", "members": ["p4", "p5"]}
    ]))
    .unwrap();
    settings.floating.insert("p3".into(), Pos { x: 40, y: 50, screen: String::new() });
    settings.float_scale.insert("p3".into(), 1.5);
    let prompts = ["p1", "p2", "p3", "p4", "p5", "p6"].iter().map(|id| test_prompt(id)).collect();
    Store { prompts, trash: Vec::new(), settings, prompts_locked: false, written: Default::default() }
}

pub(super) fn view_of<'a>(store: &'a Store, id: &str) -> &'a View {
    store.settings.views.iter().find(|v| v.id == id).unwrap()
}

#[test]
fn undoing_a_batch_delete_restores_every_placement() {
    let mut store = placement_fixture();
    let ids: Vec<String> = ["p1", "p3", "p4"].iter().map(|s| s.to_string()).collect();
    let snapshot = capture_placements(&store.settings, &ids);
    move_batch_to_trash(&mut store, &ids, 100);
    assert!(store.settings.tile_folders.is_empty() && store.settings.float_folders.is_empty(), "both folders dissolved by the delete");
    assert!(!view_of(&store, "a").members.contains(&"p3".to_string()));
    // Another tile takes p3's old cell in view a before the undo.
    store.prompts.push(test_prompt("p9"));
    let a = store.settings.views.iter_mut().find(|v| v.id == "a").unwrap();
    a.members.push("p9".into());
    a.layouts.get_mut("4x3").unwrap().insert("p9".into(), [0, 0]);

    assert_eq!(restore_placed_core(&mut store, &snapshot, |_| true), Ok(3));
    assert!(store.trash.is_empty());
    let a = view_of(&store, "a");
    let b = view_of(&store, "b");
    assert_eq!(a.members[0], "p1", "member order kept for the view");
    assert!(a.members.contains(&"p3".to_string()) && a.members.contains(&"p4".to_string()));
    assert_eq!(b.members, vec!["p3".to_string(), "p6".to_string()], "the second view membership is back");
    assert_eq!(b.layouts["4x3"].get("p3"), Some(&[1, 1]), "its cell in the second view is back");
    let la = &a.layouts["4x3"];
    assert_eq!(la.get("p9"), Some(&[0, 0]), "the tile that took the cell stays");
    let p3_cell = la.get("p3").copied().expect("p3 is on the grid again");
    assert_ne!(p3_cell, [0, 0], "a taken cell falls back to a free one");
    let mut cells: Vec<[u32; 2]> = la.values().copied().collect();
    cells.sort();
    cells.dedup();
    assert_eq!(cells.len(), la.len(), "no two tiles share a cell");
    assert!(!la.contains_key("p1"), "a folder member owns no cell");
    let tf = &store.settings.tile_folders[0];
    assert_eq!((tf.id.as_str(), tf.cell, tf.members.clone()), ("tf", [2, 0], vec!["p1".to_string(), "p2".to_string()]));
    assert_eq!(store.settings.float_folders[0].members, vec!["p4".to_string(), "p5".to_string()]);
    assert_eq!(store.settings.floating.get("p3").map(|p| (p.x, p.y)), Some((40, 50)));
    assert_eq!(store.settings.float_scale.get("p3"), Some(&1.5));
}

#[test]
fn a_failed_batch_undo_leaves_the_trash_and_settings_untouched() {
    let mut store = placement_fixture();
    let ids: Vec<String> = vec!["p3".to_string(), "p4".to_string()];
    let snapshot = capture_placements(&store.settings, &ids);
    move_batch_to_trash(&mut store, &ids, 100);
    let members_before = view_of(&store, "a").members.clone();
    assert_eq!(restore_placed_core(&mut store, &snapshot, |_| false), Err("save-failed".to_string()));
    assert_eq!(store.trash.len(), 2);
    assert_eq!(store.prompts.len(), 4);
    assert_eq!(view_of(&store, "a").members, members_before);
    assert!(store.settings.floating.is_empty());
    assert_eq!(restore_placed_core(&mut store, &[], |_| true), Err("not-found".to_string()));
}

#[test]
fn reset_expert_keeps_the_keys_and_drops_the_preferences() {
    let mut s = Settings::default();
    s.ui_texts.insert(SECRET_SALT_KEY.to_string(), "salt".into());
    s.ui_texts.insert(SECRET_VERIFY_KEY.to_string(), "verify".into());
    s.ui_texts.insert(BACKUP_PW_KEY.to_string(), "sealed".into());
    s.ui_texts.insert("copyFont".to_string(), "mono".into());
    s.ui_flags.insert("clipWatcher".to_string(), true);
    s.ui_values.insert("trayRecentN".to_string(), 9.0);
    reset_expert_settings(&mut s);
    for (k, v) in [(SECRET_SALT_KEY, "salt"), (SECRET_VERIFY_KEY, "verify"), (BACKUP_PW_KEY, "sealed")] {
        assert_eq!(s.ui_texts.get(k).map(String::as_str), Some(v), "{k} must survive");
    }
    assert!(!s.ui_texts.contains_key("copyFont"));
    assert!(s.ui_flags.is_empty() && s.ui_values.is_empty());
}

#[test]
fn reset_settings_keeps_the_backup_password() {
    // Mirrors what reset_settings does with the store's settings. The sealed
    // backup password must survive: without it every password-protected
    // backup on disk would be undecryptable.
    let mut old = Settings { theme: "ocean".into(), ..Default::default() };
    old.ui_texts.insert(BACKUP_PW_KEY.to_string(), "sealed-blob".into());
    old.ui_texts.insert("screenshotDir".to_string(), "C:/shots".into());

    let backup_pw = old.ui_texts.remove(BACKUP_PW_KEY);
    let mut fresh = Settings::default();
    fresh.migrate();
    if let Some(pw) = backup_pw {
        fresh.ui_texts.insert(BACKUP_PW_KEY.to_string(), pw);
    }

    assert_eq!(fresh.ui_texts.get(BACKUP_PW_KEY).map(String::as_str), Some("sealed-blob"));
    assert!(!fresh.ui_texts.contains_key("screenshotDir"), "other texts are reset");
    assert_eq!(fresh.theme, default_theme());
}

#[test]
fn delete_all_data_clears_data_tables_only() {
    // The data-only wipe must empty every derived table but touch no settings.
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');
             CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
             CREATE TABLE edit_state(id TEXT PRIMARY KEY, kind TEXT NOT NULL, target TEXT NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL, ops TEXT NOT NULL, updated_at INTEGER NOT NULL);
             CREATE TABLE editor_drafts(id TEXT PRIMARY KEY, prompt_id TEXT NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '', updated_at INTEGER NOT NULL);
             INSERT INTO editor_drafts VALUES('current', 'a', 'n', 'draft text', '', 1);
             INSERT INTO prompts VALUES('a', 0, '{}');
             INSERT INTO trash VALUES('b', 0, '{}');
             INSERT INTO meta VALUES('settings', '{\"theme\":\"ocean\"}');
             INSERT INTO copy_log VALUES(1, 'a');
             INSERT INTO prompt_versions(prompt_id, ts, name, text) VALUES('a', 1, 'n', 't');
             INSERT INTO clip_inbox(ts, text) VALUES(1, 'clip');
             INSERT INTO edit_state VALUES('snip:c', 'snip', 'c', 1, 1, '{}', 1);",
    )
    .unwrap();
    // A prompts row no save of this process knows about (sealed under another account).
    conn.execute("INSERT INTO prompts VALUES('foreign', 1, 'sealed')", []).unwrap();
    // Exactly the wipe delete_all_data runs.
    assert!(wipe_data_tables(&conn).is_empty());
    let count = |t: &str| -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0)).unwrap()
    };
    assert_eq!(count("prompts"), 0);
    assert_eq!(count("trash"), 0, "a data wipe must not leave trashed prompts behind");
    assert_eq!(count("copy_log"), 0);
    assert_eq!(count("prompt_versions"), 0);
    assert_eq!(count("clip_inbox"), 0);
    assert_eq!(count("edit_state"), 0);
    assert_eq!(count("editor_drafts"), 0, "a stale editor draft must not outlive the wipe");
    assert_eq!(count("meta"), 1, "settings must survive a data-only wipe");
}

pub(super) fn edit_state_test_conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE edit_state(id TEXT PRIMARY KEY, kind TEXT NOT NULL, target TEXT NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL, ops TEXT NOT NULL, updated_at INTEGER NOT NULL);",
    )
    .unwrap();
    conn
}

#[test]
fn edit_state_is_sealed_and_keyed_without_the_path_and_reads_back() {
    let conn = edit_state_test_conn();
    let path = "c:/clips/secret-meeting.mp4";
    edit_state_write(&conn, "video", path, 10, 20, "{\"v\":1,\"ops\":[\"hello overlay text\"]}", (now_ms() / 1000) as i64).unwrap();
    let (id, target, ops): (String, String, String) = conn
        .query_row("SELECT id, target, ops FROM edit_state", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    for stored in [&id, &target, &ops] {
        assert!(!stored.contains("secret-meeting") && !stored.contains("hello overlay"), "no plain path or overlay text at rest: {stored}");
    }
    let (_, back) = edit_state_read(&conn, "video", path, 10, 20).unwrap();
    assert!(back.contains("hello overlay text"));
    assert!(edit_state_read(&conn, "video", path, 10, 21).is_none(), "a changed mtime still misses");
    // The sweep reads the sealed path and drops the row once the file is gone.
    assert_eq!(sweep_edit_state_conn(&conn, |p| p == path, 30.0), 0);
    assert_eq!(sweep_edit_state_conn(&conn, |_| false, 30.0), 1);
}

#[test]
fn edit_state_reads_an_old_plain_row_and_replaces_it_on_save() {
    let conn = edit_state_test_conn();
    let path = "c:/clips/old.mp4";
    let now = (now_ms() / 1000) as i64;
    conn.execute(
        "INSERT INTO edit_state VALUES('video:c:/clips/old.mp4', 'video', 'c:/clips/old.mp4', 1, 2, '{\"v\":1}', ?1)",
        params![now],
    )
    .unwrap();
    assert_eq!(edit_state_read(&conn, "video", path, 1, 2).unwrap().1, "{\"v\":1}");
    assert_eq!(sweep_edit_state_conn(&conn, |_| true, 30.0), 0, "a plain legacy target still resolves in the sweep");
    edit_state_write(&conn, "video", path, 1, 2, "{\"v\":1,\"x\":1}", now).unwrap();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM edit_state", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1, "the legacy row is replaced, not duplicated");
    edit_state_discard(&conn, "video", path).unwrap();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM edit_state", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0);
}

#[test]
fn sweep_edit_state_removes_a_row_whose_target_is_gone() {
    let conn = edit_state_test_conn();
    let now = (now_ms() / 1000) as i64;
    conn.execute(
        "INSERT INTO edit_state VALUES('snip:c:\\gone.png', 'snip', 'c:\\gone.png', 1, 1, '{}', ?1)",
        params![now],
    )
    .unwrap();
    let removed = sweep_edit_state_conn(&conn, |_| false, 30.0);
    assert_eq!(removed, 1);
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM edit_state", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0);
}

#[test]
fn sweep_edit_state_keeps_a_fresh_row_whose_target_still_exists() {
    let conn = edit_state_test_conn();
    let now = (now_ms() / 1000) as i64;
    conn.execute(
        "INSERT INTO edit_state VALUES('snip:c:\\there.png', 'snip', 'c:\\there.png', 1, 1, '{}', ?1)",
        params![now],
    )
    .unwrap();
    let removed = sweep_edit_state_conn(&conn, |_| true, 30.0);
    assert_eq!(removed, 0);
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM edit_state", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn sweep_edit_state_removes_a_row_older_than_keep_days() {
    let conn = edit_state_test_conn();
    let stale = (now_ms() / 1000) as i64 - 40 * 86400; // 40 days ago, past a 30-day keep
    conn.execute(
        "INSERT INTO edit_state VALUES('video:c:\\old.mp4', 'video', 'c:\\old.mp4', 1, 1, '{}', ?1)",
        params![stale],
    )
    .unwrap();
    let removed = sweep_edit_state_conn(&conn, |_| true, 30.0);
    assert_eq!(removed, 1, "the file still exists, but the entry itself aged out");
}

#[test]
fn sweep_edit_state_zero_keep_days_means_forever() {
    let conn = edit_state_test_conn();
    let ancient = (now_ms() / 1000) as i64 - 3650 * 86400; // ten years ago
    conn.execute(
        "INSERT INTO edit_state VALUES('video:c:\\old.mp4', 'video', 'c:\\old.mp4', 1, 1, '{}', ?1)",
        params![ancient],
    )
    .unwrap();
    let removed = sweep_edit_state_conn(&conn, |_| true, 0.0);
    assert_eq!(removed, 0, "0 keep-days means never expire by age, same as trashRetentionDays");
}

#[test]
fn edit_state_ops_version_reads_v_and_rejects_junk() {
    assert_eq!(edit_state_ops_version("{\"v\":1,\"ops\":[]}"), Some(1));
    assert_eq!(edit_state_ops_version("{\"v\":2,\"ops\":[]}"), Some(2));
    assert_eq!(edit_state_ops_version("not json"), None, "corrupt JSON has no readable version");
    assert_eq!(edit_state_ops_version("{\"ops\":[]}"), None, "missing v field is unreadable, not v0");
    assert_eq!(edit_state_ops_version("{\"v\":\"one\"}"), None, "a non-numeric v is unreadable too");
}

#[test]
fn save_load_discard_edit_state_roundtrip_matches_only_on_the_exact_triple() {
    let conn = edit_state_test_conn();
    let id = "snip:c:\\shot.png";
    conn.execute(
        "INSERT INTO edit_state VALUES(?1, 'snip', 'c:\\shot.png', 100, 5000, '{\"v\":1,\"ops\":[]}', ?2)",
        params![id, (now_ms() / 1000) as i64],
    )
    .unwrap();
    let hit: Option<String> = conn
        .query_row(
            "SELECT ops FROM edit_state WHERE id = ?1 AND size = ?2 AND mtime = ?3",
            params![id, 100i64, 5000i64],
            |r| r.get(0),
        )
        .ok();
    assert_eq!(hit.as_deref(), Some("{\"v\":1,\"ops\":[]}"), "an exact path/size/mtime match restores the saved ops");
    let miss: Option<String> = conn
        .query_row(
            "SELECT ops FROM edit_state WHERE id = ?1 AND size = ?2 AND mtime = ?3",
            params![id, 100i64, 9999i64],
            |r| r.get(0),
        )
        .ok();
    assert!(miss.is_none(), "a changed mtime must never restore onto a file the ops were not drawn against");
    conn.execute("DELETE FROM edit_state WHERE id = ?1", params![id]).unwrap();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM edit_state", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0, "discard removes the row outright");
}

pub(super) fn editor_drafts_test_conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE editor_drafts(id TEXT PRIMARY KEY, prompt_id TEXT NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '', updated_at INTEGER NOT NULL);",
    )
    .unwrap();
    conn
}

#[test]
fn editor_draft_is_sealed_at_rest_and_reads_back_plain() {
    let conn = editor_drafts_test_conn();
    editor_draft_write(&conn, "p1", "Secret name", "Secret body text", "[\"variant one\"]", 100).unwrap();
    let raw: (String, String, String) = conn
        .query_row("SELECT name, text, variants FROM editor_drafts WHERE id = 'current'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    for (stored, plain) in [(&raw.0, "Secret name"), (&raw.1, "Secret body text"), (&raw.2, "variant one")] {
        assert!(!stored.contains(plain), "the draft column must not hold the plaintext");
    }
    let d = editor_draft_read(&conn).unwrap();
    assert_eq!((d.name.as_str(), d.text.as_str(), d.variants.as_str()), ("Secret name", "Secret body text", "[\"variant one\"]"));
}

#[test]
fn an_undecryptable_sealed_draft_is_dropped_not_offered_empty() {
    let conn = editor_drafts_test_conn();
    conn.execute(
        "INSERT INTO editor_drafts(id, prompt_id, name, text, variants, updated_at) VALUES('current','p1','enc:v1:AAAA','enc:v1:AAAA','enc:v1:AAAA',5)",
        [],
    )
    .unwrap();
    assert!(editor_draft_read(&conn).is_none());
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0, "the unreadable row is deleted");
}

#[test]
fn an_old_plain_edit_state_row_is_sealed_on_first_read() {
    let conn = edit_state_test_conn();
    conn.execute(
        "INSERT INTO edit_state VALUES('video:c:/clips/old.mp4', 'video', 'c:/clips/old.mp4', 1, 2, '{\"v\":1,\"t\":\"overlay words\"}', 77)",
        [],
    )
    .unwrap();
    let (_, ops) = edit_state_read(&conn, "video", "c:/clips/old.mp4", 1, 2).unwrap();
    assert!(ops.contains("overlay words"));
    let (id, target, stored_ops, updated): (String, String, String, i64) = conn
        .query_row("SELECT id, target, ops, updated_at FROM edit_state", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap();
    assert!(!id.contains("old.mp4"), "the legacy id is gone");
    assert!(target.starts_with(ENC_PREFIX) && stored_ops.starts_with(ENC_PREFIX), "target and ops are sealed now");
    assert_eq!(updated, 77, "the retention clock keeps running");
    assert!(edit_state_read(&conn, "video", "c:/clips/old.mp4", 1, 2).unwrap().1.contains("overlay words"));
}

#[test]
fn startup_pass_seals_every_plain_edit_state_row_once() {
    let conn = edit_state_test_conn();
    for (id, target, ts) in [("video:c:/a/one.mp4", "c:/a/one.mp4", 11), ("video:c:/a/two.mp4", "c:/a/two.mp4", 22)] {
        conn.execute("INSERT INTO edit_state VALUES(?1, 'video', ?2, 1, 2, '{\"v\":1,\"t\":\"words\"}', ?3)", params![id, target, ts]).unwrap();
    }
    assert_eq!(edit_state_seal_all(&conn), 2);
    let mut st = conn.prepare("SELECT id, target, ops, updated_at FROM edit_state ORDER BY updated_at").unwrap();
    let rows: Vec<(String, String, String, i64)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap().flatten().collect();
    assert_eq!(rows.len(), 2);
    for ((id, target, ops, ts), (want_ts, path)) in rows.iter().zip([(11, "c:/a/one.mp4"), (22, "c:/a/two.mp4")]) {
        assert_eq!(*id, edit_state_id("video", path));
        assert!(target.starts_with(ENC_PREFIX) && ops.starts_with(ENC_PREFIX));
        assert_eq!(*ts, want_ts);
    }
    assert_eq!(edit_state_seal_all(&conn), 0, "a second run writes nothing");
}

#[test]
fn editor_draft_reads_an_old_unsealed_row() {
    let conn = editor_drafts_test_conn();
    conn.execute("INSERT INTO editor_drafts(id, prompt_id, name, text, variants, updated_at) VALUES('current','p1','Old','Old body','[]',5)", []).unwrap();
    let d = editor_draft_read(&conn).unwrap();
    assert_eq!((d.name.as_str(), d.text.as_str(), d.variants.as_str()), ("Old", "Old body", "[]"));
}

// A read-only instance (another machine owns the folder) reads edit states and drafts without
// sealing or deleting anything in the owner's database.
#[test]
fn a_read_only_read_of_edit_state_and_draft_writes_nothing() {
    let conn = edit_state_test_conn();
    conn.execute(
        "INSERT INTO edit_state VALUES('video:c:/clips/old.mp4', 'video', 'c:/clips/old.mp4', 1, 2, '{\"v\":1,\"t\":\"words\"}', 77)",
        [],
    )
    .unwrap();
    let (_, ops) = edit_state_read_with(&conn, "video", "c:/clips/old.mp4", 1, 2, false).unwrap();
    assert!(ops.contains("words"));
    let (id, ops_stored): (String, String) = conn.query_row("SELECT id, ops FROM edit_state", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(id, "video:c:/clips/old.mp4", "the row keeps its legacy id");
    assert!(!ops_stored.starts_with(ENC_PREFIX), "the row stays unsealed");

    let conn = editor_drafts_test_conn();
    conn.execute("INSERT INTO editor_drafts(id, prompt_id, name, text, variants, updated_at) VALUES('current','p1','Old','Old body','[]',5)", []).unwrap();
    assert_eq!(editor_draft_read_with(&conn, false).unwrap().text, "Old body");
    let raw: String = conn.query_row("SELECT text FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(raw, "Old body", "the draft stays unsealed");
    conn.execute("UPDATE editor_drafts SET name='enc:v1:AAAA', text='enc:v1:AAAA', variants='enc:v1:AAAA'", []).unwrap();
    assert!(editor_draft_read_with(&conn, false).is_none());
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1, "an unreadable draft is not deleted from the owner's folder");
}

// Same upsert SQL as save_editor_draft, run directly against a bare
// in-memory connection - the async command itself only exists to fetch its
// own AppHandle-bound connection, no test harness for that here (same
// reasoning as the edit_state roundtrip test right above).
#[test]
fn editor_draft_write_then_overwrite_keeps_a_single_current_row() {
    let conn = editor_drafts_test_conn();
    let upsert = "INSERT INTO editor_drafts(id, prompt_id, name, text, variants, updated_at) VALUES('current',?1,?2,?3,?4,?5)
             ON CONFLICT(id) DO UPDATE SET prompt_id=excluded.prompt_id, name=excluded.name, text=excluded.text, variants=excluded.variants, updated_at=excluded.updated_at";
    conn.execute(upsert, params!["neu", "First", "Hello", "[]", 100i64]).unwrap();
    conn.execute(upsert, params!["p1", "First edited", "Hello there", "[]", 200i64]).unwrap();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1, "a second write overwrites the single current row, it never accumulates rows");
    let (prompt_id, name, text): (String, String, String) = conn
        .query_row("SELECT prompt_id, name, text FROM editor_drafts WHERE id = 'current'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    assert_eq!((prompt_id.as_str(), name.as_str(), text.as_str()), ("p1", "First edited", "Hello there"));
}

#[test]
fn editor_draft_delete_removes_the_row_the_same_way_a_save_does() {
    let conn = editor_drafts_test_conn();
    conn.execute(
        "INSERT INTO editor_drafts(id, prompt_id, name, text, variants, updated_at) VALUES('current','p1','A','B','[]',100)",
        [],
    )
    .unwrap();
    conn.execute("DELETE FROM editor_drafts WHERE id = 'current'", []).unwrap();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0, "an orderly close, save or discard alike, must leave no draft behind");
}

#[test]
fn discard_editor_draft_if_removes_the_row_only_when_its_own_entry_matches() {
    let conn = editor_drafts_test_conn();
    conn.execute(
        "INSERT INTO editor_drafts(id, prompt_id, name, text, variants, updated_at) VALUES('current','p1','A','B','[]',100)",
        [],
    )
    .unwrap();
    discard_editor_draft_if(&conn, |t| t == "p2"); // an unrelated prompt was deleted
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1, "a draft for a different entry must survive an unrelated delete");
    discard_editor_draft_if(&conn, |t| t == "p1"); // its own entry was deleted
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0, "the draft disappears once the entry it belongs to is gone");
}

#[test]
fn discard_editor_draft_if_covers_a_batch_delete_set_too() {
    let conn = editor_drafts_test_conn();
    conn.execute(
        "INSERT INTO editor_drafts(id, prompt_id, name, text, variants, updated_at) VALUES('current','p3','A','B','[]',100)",
        [],
    )
    .unwrap();
    let ids: std::collections::HashSet<&str> = ["p1", "p2", "p3"].into_iter().collect();
    discard_editor_draft_if(&conn, |t| ids.contains(t));
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0, "batch_prompts's delete branch matches on the same idset a single delete uses a plain equality for");
}

#[test]
fn discard_editor_draft_if_is_a_no_op_when_no_draft_exists() {
    let conn = editor_drafts_test_conn();
    discard_editor_draft_if(&conn, |_| true); // nothing to delete, must not panic or error
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM editor_drafts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0);
}

#[test]
fn prompts_and_trash_write_together_or_not_at_all() {
    // The whole point of the combined write - a failure partway through must
    // roll back BOTH tables, not just the one it failed on (the old two-
    // transaction version could durably commit prompts and then fail on trash,
    // stranding a moved prompt in neither table).
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             INSERT INTO prompts VALUES('before', 0, '{}');
             INSERT INTO trash VALUES('trashed-before', 0, '{}');",
    )
    .unwrap();
    let base = sample_prompts()[0].clone();
    let good_prompts = vec![Prompt { id: "a".into(), ..base.clone() }];
    // A duplicate id makes the second trash INSERT violate its PRIMARY KEY,
    // forcing the write to fail after the prompts half (same transaction)
    // already succeeded.
    let dup_trash = vec![
        Prompt { id: "dup".into(), ..base.clone() },
        Prompt { id: "dup".into(), ..base },
    ];
    let mut written = WrittenRows::new();
    assert!(db_write_prompts_and_trash(&mut conn, &good_prompts, &dup_trash, &mut written).is_err());
    assert!(
        written.is_empty(),
        "a write that never committed must not leave rows recorded as written"
    );
    let ids = |table: &str| -> Vec<String> {
        conn.prepare(&format!("SELECT id FROM {table} ORDER BY id"))
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .flatten()
            .collect()
    };
    assert_eq!(
        ids("prompts"), vec!["before".to_string()],
        "a failed write must not touch prompts either, even though the prompts half alone would have succeeded"
    );
    assert_eq!(ids("trash"), vec!["trashed-before".to_string()]);
}

// Known answers produced by the previous crypto crate generation: files and vault entries
// written by older versions must keep opening, so key derivation and the cipher output are
// pinned byte for byte.
#[test]
fn argon2_and_aes_gcm_output_match_older_releases() {
    let hex = |b: &[u8]| crate::ffmpeg::hex_lower(b);
    let mut key = [0u8; 32];
    argon2::Argon2::default()
        .hash_password_into(b"correct horse", &[9u8; 16], &mut key)
        .expect("derive");
    assert_eq!(hex(&key), "c5ddcc8506428944319bd9efd13cab76e4f46bd5d76e09a4aa7aa710a8002aaf");
    let ct = aead_seal(&key, [7u8; 12], &b"hello vault"[..]).expect("encrypt");
    assert_eq!(hex(&ct), "48df4ad09197b35aafb5262d4a06e31d0d7a9e9771952bcf318cb5");
    assert_eq!(aead_open(&key, &[7u8; 12], &ct).expect("decrypt"), b"hello vault");
    assert!(aead_open(&key, &[8u8; 12], &ct).is_none(), "another nonce does not open it");
    assert!(aead_open(&key, &[7u8; 11], &ct).is_none(), "a short nonce is refused");
    assert!(aead_open(&key, &[7u8; 12], &ct[..8]).is_none(), "shorter than a tag is refused");
}

#[test]
fn pill_box_prefers_the_free_size_over_the_scale_box() {
    let p = test_prompt("p1");
    let mut s = Settings::default();
    assert_eq!(pill_box_of(&s, &p), pill_dims(is_image_prompt(&p), 1.0));
    s.float_scale.insert("p1".into(), 1.5);
    assert_eq!(pill_box_of(&s, &p), pill_dims(is_image_prompt(&p), 1.5));
    s.float_size.insert("p1".into(), [300.0, 60.0]);
    assert_eq!(pill_box_of(&s, &p), (300.0, 60.0));
}

#[test]
fn trash_round_trip_keeps_the_free_pill_size() {
    let mut store = placement_fixture();
    store.settings.float_size.insert("p3".into(), [210.0, 44.0]);
    let ids = vec!["p3".to_string()];
    let snapshot = capture_placements(&store.settings, &ids);
    assert_eq!(snapshot[0].float_size, Some([210.0, 44.0]));
    move_batch_to_trash(&mut store, &ids, 100);
    assert!(!store.settings.float_size.contains_key("p3"), "delete drops the free size");
    assert_eq!(restore_placed_core(&mut store, &snapshot, |_| true), Ok(1));
    assert_eq!(store.settings.float_size.get("p3"), Some(&[210.0, 44.0]));
}

#[test]
fn an_old_trash_entry_without_a_free_size_still_restores() {
    let old: PromptPlacement = serde_json::from_str(r#"{"id":"p3","float_scale":1.5}"#).expect("old snapshot parses");
    assert_eq!(old.float_size, None);
    let mut store = placement_fixture();
    let ids = vec!["p3".to_string()];
    move_batch_to_trash(&mut store, &ids, 100);
    assert_eq!(restore_placed_core(&mut store, &[old], |_| true), Ok(1));
    assert_eq!(store.settings.float_scale.get("p3"), Some(&1.5));
    assert!(!store.settings.float_size.contains_key("p3"));
}
