use super::*;

// ---- Per-prompt hotkeys ----

#[test]
fn accelerators_are_validated_before_they_reach_the_os() {
    for good in ["CommandOrControl+Shift+P", "ctrl+shift+p", "Alt+F5", "Ctrl+Alt+K", " Ctrl+Shift+P "] {
        assert!(parse_accel(good).is_some(), "rejected a usable accelerator: {good:?}");
    }
    for bad in [
        "",
        "   ",
        "P",                       // no modifier: would swallow the key everywhere
        "Shift+",                  // empty token
        "Ctrl+Shift+NotAKey",
        "Ctrl+A+B",                // two main keys
        "Ctrl+Shift+\u{7}",        // control character
    ] {
        assert!(parse_accel(bad).is_none(), "accepted an unusable accelerator: {bad:?}");
    }
    // An over-long string is rejected outright, never handed to the OS.
    let long = format!("Ctrl+Shift+{}", "A".repeat(MAX_ACCEL_LEN));
    assert!(parse_accel(&long).is_none());
    // Spelling and case do not make two accelerators different.
    assert_eq!(parse_accel("CommandOrControl+Shift+P"), parse_accel("ctrl+SHIFT+p"));
}

// A global hotkey SWALLOWS its keystroke in every other program, so the
// blocklist decides what the app is allowed to take away from the user.
#[test]
fn system_and_universal_shortcuts_are_refused() {
    let reason = |s: &str| accel_error(s).err();
    // The universal editing set: losing these breaks every program at once.
    for s in [
        "Ctrl+C", "Ctrl+V", "Ctrl+X", "Ctrl+A", "Ctrl+Z", "Ctrl+Y", "Ctrl+S",
        "Ctrl+F", "Ctrl+P", "Ctrl+N", "Ctrl+O", "Ctrl+W", "Ctrl+T",
        "CommandOrControl+C", // the spelling the UI actually stores
    ] {
        assert_eq!(reason(s), Some(HK_SYSTEM), "not refused: {s}");
    }
    // Windows-reserved, and the ones the OS would never hand over anyway.
    for s in [
        "Ctrl+Alt+Delete", "Ctrl+Shift+Escape", "Ctrl+Escape", "Alt+Tab",
        "Alt+Shift+Tab", "Alt+F4", "Ctrl+F4", "Alt+Space", "Alt+Escape",
        "Ctrl+Shift+PrintScreen", "Alt+Enter", "Ctrl+Backspace", "Ctrl+Insert",
        "Ctrl+Alt+CapsLock", "Ctrl+Space",
    ] {
        assert_eq!(reason(s), Some(HK_SYSTEM), "not refused: {s}");
    }
    // Anything with the Windows key: that namespace belongs to the OS. Named
    // explicitly for the well-known Win+<x> shortcuts (number-pin, screenshot,
    // clipboard history, emoji picker, share, dictation, dot/emoji on some
    // keyboards) even though the SUPER/META check above already refuses the
    // whole namespace on its own.
    for s in [
        "Super+K", "Super+Shift+S", "Alt+Super+P", "Cmd+Q", "Super+1", "Super+9",
        "Super+G", "Super+V", "Super+H", "Super+Period",
    ] {
        assert_eq!(reason(s), Some(HK_SYSTEM), "not refused: {s}");
    }
    // Shift alone is how a capital letter is typed: never a modifier here.
    for s in ["Shift+A", "Shift+1", "Shift+F5"] {
        assert_eq!(reason(s), Some(HK_INVALID), "not refused: {s}");
    }
    // …and the line stops there: adding a second modifier makes a combination
    // no program owns, which is exactly where users pick their own shortcuts.
    for s in [
        "Ctrl+Shift+C", "Ctrl+Alt+V", "Ctrl+Shift+S", "Ctrl+Alt+Q", "Alt+Q",
        "Ctrl+Shift+1", "Alt+F9", "Ctrl+F1", "Ctrl+Shift+F4", "Ctrl+Alt+ArrowUp",
    ] {
        assert_eq!(reason(s), None, "wrongly refused: {s}");
    }
}

// The shipped default must actually register, or every fresh install would
// silently lose its launcher hotkey the moment rearm_hotkeys() runs at startup
// (an unparseable/blocked default gets cleared back to "", exactly like a bad
// value a user typed in by hand -- see rearm_hotkeys' app_ok branch).
#[test]
fn the_default_hotkey_is_not_refused() {
    assert_eq!(accel_error(&Settings::default().hotkey).err(), None);
}

// The blocklist is authoritative wherever a binding can enter the store.
#[test]
fn a_blocked_combination_never_reaches_the_store() {
    let prompts = sample_prompts();
    let others: Vec<&Prompt> = prompts.iter().collect();
    assert_eq!(sanitize_hotkey("Ctrl+C", "", "", "", "", &others), Err(HK_SYSTEM.into()));
    assert_eq!(sanitize_hotkey("Shift+A", "", "", "", "", &others), Err(HK_INVALID.into()));
    assert_eq!(sanitize_hotkey("Super+J", "", "", "", "", &others), Err(HK_SYSTEM.into()));
    // A blocked accelerator is never registered, whatever put it in the store.
    let mut blocked = sample_prompts();
    blocked[0].hotkey = "Ctrl+V".into();
    blocked[1].hotkey = "Alt+Tab".into();
    assert!(hotkey_plan("Ctrl+S", None, None, None, &blocked, true).is_empty());
    // …and an import drops it instead of keeping a binding that does nothing.
    assert_eq!(drop_colliding_hotkeys("", &[], &mut blocked), 2);
    assert!(blocked.iter().all(|p| p.hotkey.is_empty()));
}

#[test]
fn a_hotkey_carries_its_own_copy_or_paste_choice() {
    assert_eq!(sanitize_hotkey_mode("copy"), "copy");
    assert_eq!(sanitize_hotkey_mode(" paste "), "paste");
    // Anything else means "follow the global default".
    for raw in ["", "   ", "yes", "true", "PASTE", "copy;drop"] {
        assert_eq!(sanitize_hotkey_mode(raw), "", "not folded to the default: {raw:?}");
    }
}

#[test]
fn hotkey_plan_resolves_conflicts_by_precedence() {
    let mut prompts = sample_prompts();
    prompts[1].hotkey = "CommandOrControl+Shift+2".into();
    // The app hotkey wins over a prompt that wants the same combination.
    let plan = hotkey_plan("ctrl+shift+1", None, None, None, &prompts, true);
    assert_eq!(plan.len(), 2);
    assert_eq!(plan[0].1, HotkeyOwner::App);
    assert_eq!(plan[1].1, HotkeyOwner::Prompt("p2".into()));

    // Between two prompts the first one in list order keeps it.
    prompts[1].hotkey = "ctrl+shift+1".into();
    let plan = hotkey_plan("", None, None, None, &prompts, true);
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].1, HotkeyOwner::Prompt("p1".into()));

    // Feature toggled off: nothing but the app hotkey is registered, and the
    // stored accelerators stay untouched.
    let plan = hotkey_plan("Alt+Q", None, None, None, &prompts, false);
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].1, HotkeyOwner::App);
    assert_eq!(prompts[0].hotkey, "CommandOrControl+Shift+1");
}

// A third owner, distinct from both the app hotkey and any prompt --
// every combination lands on the right one, and it follows the exact same
// precedence/collision rules (app first, then voice, then prompts).
#[test]
fn hotkey_plan_gives_voice_its_own_owner() {
    let mut prompts = sample_prompts();
    prompts[0].hotkey = "ctrl+shift+7".into();
    let plan = hotkey_plan("ctrl+shift+1", Some("ctrl+shift+9"), None, None, &prompts, true);
    assert_eq!(plan[0].1, HotkeyOwner::App);
    assert!(plan.iter().any(|(_, o)| *o == HotkeyOwner::Voice));
    assert!(plan.iter().any(|(_, o)| *o == HotkeyOwner::Prompt("p1".into())));

    // The app hotkey wins a collision with the voice hotkey, same as with a prompt.
    let plan = hotkey_plan("ctrl+shift+1", Some("ctrl+shift+1"), None, None, &prompts, true);
    assert!(!plan.iter().any(|(_, o)| *o == HotkeyOwner::Voice));

    // Switched off (the caller passes None): nothing voice-owned is registered.
    let plan = hotkey_plan("ctrl+shift+1", None, None, None, &prompts, true);
    assert!(!plan.iter().any(|(_, o)| *o == HotkeyOwner::Voice));

    // A combination the blocklist refuses is dropped just like anywhere else.
    let plan = hotkey_plan("", Some("Ctrl+C"), None, None, &prompts, true);
    assert!(!plan.iter().any(|(_, o)| *o == HotkeyOwner::Voice));
}

#[test]
fn a_deleted_prompt_gives_its_accelerator_back() {
    let mut prompts = sample_prompts();
    prompts[1].hotkey = "Ctrl+Shift+2".into();
    assert_eq!(hotkey_plan("", None, None, None, &prompts, true).len(), 2);
    prompts.retain(|p| p.id != "p1");
    let plan = hotkey_plan("", None, None, None, &prompts, true);
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].1, HotkeyOwner::Prompt("p2".into()));
}

#[test]
fn an_import_drops_colliding_hotkeys_instead_of_failing() {
    let mut prompts = sample_prompts();
    prompts[1].hotkey = "ctrl+shift+1".into(); // same combination as p1
    assert_eq!(drop_colliding_hotkeys("", &[], &mut prompts), 1);
    assert_eq!(prompts[0].hotkey, "CommandOrControl+Shift+1");
    assert!(prompts[1].hotkey.is_empty());

    // The app hotkey outranks every prompt, and junk is dropped too.
    let mut prompts = sample_prompts();
    prompts[1].hotkey = "not an accelerator".into();
    assert_eq!(drop_colliding_hotkeys("Ctrl+Shift+1", &[], &mut prompts), 2);
    assert!(prompts.iter().all(|p| p.hotkey.is_empty()));

    // Nothing to drop leaves every binding in place.
    let mut prompts = sample_prompts();
    prompts[1].hotkey = "Alt+F9".into();
    assert_eq!(drop_colliding_hotkeys("Ctrl+Alt+Q", &[], &mut prompts), 0);
    assert_eq!(prompts[1].hotkey, "Alt+F9");
}

#[test]
fn the_editor_rejects_a_taken_or_unusable_accelerator() {
    let prompts = sample_prompts();
    let others: Vec<&Prompt> = prompts.iter().collect();
    assert_eq!(sanitize_hotkey("", "Ctrl+Q", "", "", "", &others).unwrap(), "");
    assert_eq!(sanitize_hotkey("  Alt+F9  ", "Ctrl+Q", "", "", "", &others).unwrap(), "Alt+F9");
    assert_eq!(sanitize_hotkey("Ctrl+Shift+P", "ctrl+shift+p", "", "", "", &others), Err("hotkey-taken".into()));
    assert_eq!(sanitize_hotkey("ctrl+shift+1", "", "", "", "", &others), Err("hotkey-taken".into()));
    assert_eq!(sanitize_hotkey("F9", "", "", "", "", &others), Err("hotkey-invalid".into()));
    // Editing the prompt that already owns the combination is not a conflict.
    let others: Vec<&Prompt> = prompts.iter().filter(|p| p.id != "p1").collect();
    assert_eq!(sanitize_hotkey("ctrl+shift+1", "", "", "", "", &others).unwrap(), "ctrl+shift+1");
}

// A prompt hotkey identical to the currently-configured rec-stop/pause
// combination is refused up front -- it used to be accepted, then silently
// stop firing the moment a recording started (hotkey_plan gives those two
// priority over every prompt).
#[test]
fn the_editor_rejects_a_prompt_hotkey_matching_the_active_recording_hotkeys() {
    let prompts = sample_prompts();
    let others: Vec<&Prompt> = prompts.iter().collect();
    assert_eq!(
        sanitize_hotkey("Ctrl+Alt+S", "", "", "ctrl+alt+s", "ctrl+alt+o", &others),
        Err(HK_TAKEN.into())
    );
    assert_eq!(
        sanitize_hotkey("Ctrl+Alt+O", "", "", "ctrl+alt+s", "ctrl+alt+o", &others),
        Err(HK_TAKEN.into())
    );
    // Feature/toggle off (caller passes "") -- the same combination is free again.
    assert_eq!(sanitize_hotkey("Ctrl+Alt+S", "", "", "", "", &others).unwrap(), "Ctrl+Alt+S");
}

#[test]
fn check_hotkey_ignores_recording_hotkeys_that_are_switched_off() {
    let mut settings = Settings {
        rec_stop_hotkey: "Ctrl+Alt+S".to_string(),
        rec_pause_hotkey: "Ctrl+Alt+O".to_string(),
        ..Default::default()
    };
    let stop = parse_accel("Ctrl+Alt+S").unwrap();
    let pause = parse_accel("Ctrl+Alt+O").unwrap();
    assert!(hotkey_in_use(&settings, &[], None, stop));
    assert!(hotkey_in_use(&settings, &[], None, pause));
    settings.ui_flags.insert("recPauseHotkey".to_string(), false);
    assert!(hotkey_in_use(&settings, &[], None, stop));
    assert!(!hotkey_in_use(&settings, &[], None, pause));
    settings.ui_flags.insert("screenRecord".to_string(), false);
    assert!(!hotkey_in_use(&settings, &[], None, stop));
}

// A prompt stored before per-prompt hotkeys existed has no hotkey field at all: it must load with an
// empty binding instead of failing the whole row.
#[test]
fn a_prompt_without_a_hotkey_field_still_loads() {
    let json = r#"{"id":"old","name":"N","text":"T","color":"","tags":["Work"]}"#;
    let p: Prompt = serde_json::from_str(json).unwrap();
    assert!(p.hotkey.is_empty());
    assert!(parse_accel(&p.hotkey).is_none());
}

// Settings JSON from an older version: unknown/removed fields (copy_history,
// a future field) are ignored and every missing field defaults: never a total
// parse failure that would wipe the user's settings.
#[test]
fn load_prune_drops_a_removed_value_and_keeps_a_live_one() {
    let mut s = Settings::default();
    s.ui_values.insert("syncFailMax".into(), 9.0);
    s.ui_values.insert("errorLogKeepKb".into(), 512.0);
    assert!(s.has_removed_ui_values());
    s.migrate();
    assert!(!s.ui_values.contains_key("syncFailMax"));
    assert_eq!(s.ui_values.get("errorLogKeepKb"), Some(&512.0));
    assert!(!s.has_removed_ui_values());
}

#[test]
fn a_failed_repack_makes_the_inbox_scrub_a_failure() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
             INSERT INTO clip_inbox(ts, text) VALUES(1, 'secret');",
    )
    .unwrap();
    // A VACUUM cannot run inside a transaction, so the repack step fails here.
    conn.execute_batch("BEGIN").unwrap();
    assert_eq!(scrub_clip_inbox(&conn), None);
    conn.execute_batch("ROLLBACK").unwrap();
    assert_eq!(scrub_clip_inbox(&conn), Some(1));
    assert_eq!(scrub_clip_inbox(&conn), Some(0), "nothing left: no repack needed, still a success");
}

#[test]
fn a_failing_row_leaves_the_copy_history_untouched() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             INSERT INTO copy_log VALUES(1, 'keep');
             CREATE TRIGGER no_bad BEFORE INSERT ON copy_log WHEN NEW.id = 'bad' BEGIN SELECT RAISE(ABORT, 'no'); END;",
    )
    .unwrap();
    let rows = [
        CopyLogRow { ts: 2, id: "a".into() },
        CopyLogRow { ts: 3, id: "bad".into() },
        CopyLogRow { ts: 4, id: "b".into() },
    ];
    assert!(db_restore_copy_log(&conn, &rows).is_err());
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM copy_log", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1, "the rows before the failing one must not stay");
    assert!(db_restore_copy_log(&conn, &rows[..1]).is_ok());
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM copy_log", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 2);
}

#[test]
fn the_clip_inbox_cap_defaults_and_clamps() {
    let mut s = Settings::default();
    assert_eq!(clip_inbox_cap(&s), 50);
    s.ui_values.insert("clipInboxMax".into(), 5.0);
    assert_eq!(clip_inbox_cap(&s), 10);
    s.ui_values.insert("clipInboxMax".into(), 999.0);
    assert_eq!(clip_inbox_cap(&s), 200);
}

#[test]
fn the_clip_cap_counts_rows_and_an_undo_sorts_below_newer_copies() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);",
    )
    .unwrap();
    for i in 0..10u64 {
        db_add_clip(&conn, i, &format!("t{i}"), 50).unwrap();
    }
    // Dismissing single entries leaves holes in the ids; the cap must not eat real rows.
    for id in [2, 4, 6, 8] {
        conn.execute("DELETE FROM clip_inbox WHERE id = ?1", params![id]).unwrap();
    }
    db_add_clip(&conn, 10, "t10", 7).unwrap();
    assert_eq!(db_count_clips(&conn), 7);
    // Clear, a new copy arrives, then the undo brings the old rows back.
    let old = db_list_clips(&conn);
    conn.execute("DELETE FROM clip_inbox", []).unwrap();
    db_add_clip(&conn, 20, "fresh", 50).unwrap();
    let items: Vec<ClipRestoreItem> =
        old.iter().rev().map(|c| ClipRestoreItem { id: Some(c.id), ts: c.ts, text: c.text.clone() }).collect();
    db_restore_clips(&conn, &items, 50).unwrap();
    let texts: Vec<String> = db_list_clips(&conn).into_iter().map(|c| c.text).collect();
    assert_eq!(texts.first().map(String::as_str), Some("fresh"), "the new copy stays the newest");
    assert_eq!(texts.len(), 8);
    // A cap smaller than the restored set drops the oldest, never the new copy.
    db_restore_clips(&conn, &[], 3).unwrap();
    assert_eq!(db_list_clips(&conn).first().map(|c| c.text.as_str()), Some("fresh"));
    assert_eq!(db_count_clips(&conn), 3);
}

#[test]
fn a_failing_item_leaves_the_clip_inbox_untouched() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
             CREATE TRIGGER no_bad BEFORE INSERT ON clip_inbox WHEN NEW.ts = 99 BEGIN SELECT RAISE(ABORT, 'no'); END;",
    )
    .unwrap();
    let items = [
        ClipRestoreItem { id: None, ts: 1, text: "a".into() },
        ClipRestoreItem { id: None, ts: 99, text: "bad".into() },
        ClipRestoreItem { id: None, ts: 3, text: "b".into() },
    ];
    assert!(db_restore_clips(&conn, &items, 50).is_err());
    assert!(db_list_clips(&conn).is_empty(), "no partial restore");
    assert_eq!(db_count_clips(&conn), 0);
    assert!(db_restore_clips(&conn, &[ClipRestoreItem { id: None, ts: 1, text: "a".into() }, ClipRestoreItem { id: None, ts: 3, text: "b".into() }], 50).is_ok());
    assert_eq!(db_list_clips(&conn).len(), 2);
    assert_eq!(db_count_clips(&conn), 2, "the count matches the list without unsealing it");
}

#[test]
fn clearing_the_history_keeps_the_counters_when_the_delete_fails() {
    let mut s = Settings::default();
    s.usage.insert("a".into(), 4);
    s.last_used.insert("a".into(), 77);
    assert!(!clear_history_core(|| false, &mut s));
    assert_eq!(s.usage.get("a"), Some(&4));
    assert_eq!(s.last_used.get("a"), Some(&77));
    assert!(clear_history_core(|| true, &mut s));
    assert!(s.usage.is_empty() && s.last_used.is_empty());
}

#[test]
fn a_pair_of_writes_lands_together_or_not_at_all() {
    use std::cell::Cell;
    let undone = Cell::new(false);
    let second_ran = Cell::new(false);
    // The first write fails: the second never starts, nothing to undo.
    assert!(!save_pair_or_undo_first(|| false, || { second_ran.set(true); true }, || undone.set(true)));
    assert!(!second_ran.get() && !undone.get());
    // The second fails: the first is taken back and the call reports failure.
    assert!(!save_pair_or_undo_first(|| true, || false, || undone.set(true)));
    assert!(undone.get());
    // Both land: no undo.
    undone.set(false);
    assert!(save_pair_or_undo_first(|| true, || true, || undone.set(true)));
    assert!(!undone.get());
}

#[test]
fn search_caps_merge_to_largest() {
    let run = |old: &[(&str, f64)], new: Option<f64>| {
        let mut s = Settings::default();
        for (k, v) in old {
            s.ui_values.insert((*k).into(), *v);
        }
        if let Some(v) = new {
            s.ui_values.insert("searchListMax".into(), v);
        }
        let dirty = s.has_removed_ui_values();
        s.migrate();
        for (k, _) in old {
            assert!(!s.ui_values.contains_key(*k), "{k} survived migrate");
        }
        assert_eq!(dirty, !old.is_empty());
        s.ui_values.get("searchListMax").copied()
    };
    // The largest stored value wins; a stored 0 is ignored.
    assert_eq!(run(&[("searchRecentMax", 12.0), ("gsRecentMax", 0.0)], None), Some(12.0));
    assert_eq!(run(&[("globalSearchLimit", 5.0), ("trayMenuMax", 15.0), ("launcherMaxResults", 8.0)], None), Some(15.0));
    // A cap that was never stored counts with the default it had: one lowered cap
    // must not shrink the lists the others kept at their defaults.
    assert_eq!(run(&[("gsRecentMax", 3.0)], None), Some(8.0));
    assert_eq!(run(&[("searchRecentMax", 1.0), ("globalSearchLimit", 4.0)], None), Some(8.0));
    assert_eq!(run(&[("searchRecentMax", 12.0), ("gsRecentMax", 0.0)], None), Some(12.0));
    // Only a zero stored: nothing to carry over.
    assert_eq!(run(&[("gsRecentMax", 0.0)], None), None);
    // Out-of-range old values land inside the new range.
    assert_eq!(run(&[("searchRecentMax", 1.0)], None), Some(8.0));
    assert_eq!(run(&[("searchRecentMax", 1.0), ("globalSearchLimit", 1.0), ("gsRecentMax", 1.0), ("launcherMaxResults", 1.0), ("trayMenuMax", 1.0)], None), Some(3.0));
    assert_eq!(run(&[("launcherMaxResults", 99.0)], None), Some(20.0));
    // Nothing stored: no key. A value already under the new key is kept.
    assert_eq!(run(&[], None), None);
    assert_eq!(run(&[("searchRecentMax", 12.0)], Some(6.0)), Some(6.0));
}

#[test]
fn toggle_pairs_merge_to_zero() {
    let run = |flags: &[(&str, bool)], values: &[(&str, f64)]| {
        let mut s = Settings::default();
        for (k, v) in flags {
            s.ui_flags.insert((*k).into(), *v);
        }
        for (k, v) in values {
            s.ui_values.insert((*k).into(), *v);
        }
        let dirty = s.has_removed_ui_values();
        s.migrate();
        assert_eq!(dirty, !flags.is_empty());
        for (k, _) in flags {
            assert!(!s.ui_flags.contains_key(*k), "{k} survived migrate");
        }
        s
    };
    // A switch stored off becomes a stored 0, whatever duration sat beside it.
    let s = run(&[("copyBubble", false)], &[]);
    assert_eq!(s.ui_values.get("bubbleMs"), Some(&0.0));
    let s = run(&[("undoToast", false)], &[("undoToastMs", 5000.0)]);
    assert_eq!(s.ui_values.get("undoToastMs"), Some(&0.0));
    let s = run(&[("tooltipDelay", false)], &[]);
    assert_eq!(s.ui_values.get("tooltipDelayMs"), Some(&0.0));
    // A switch stored on keeps the stored duration.
    let s = run(&[("editorDraft", true)], &[("editorDraftSecs", 9.0)]);
    assert_eq!(s.ui_values.get("editorDraftSecs"), Some(&9.0));
    // A switch stored on with no duration leaves no key (the default applies).
    let s = run(&[("copyBubble", true)], &[]);
    assert!(!s.ui_values.contains_key("bubbleMs"));
    // Nothing stored: nothing written.
    let s = run(&[], &[]);
    assert!(s.ui_values.is_empty());
}

pub(super) fn migrated_with_folder_flags(flags: &[(&str, bool)]) -> Settings {
    let mut s = Settings::default();
    for (k, v) in flags {
        s.ui_flags.insert((*k).into(), *v);
    }
    s.migrate();
    s
}

#[test]
fn both_folder_switches_off_turn_folders_off() {
    let s = migrated_with_folder_flags(&[("tileFolders", false), ("pillFolders", false)]);
    assert_eq!(s.ui_flags.get("folders"), Some(&false));
    assert!(!s.ui_flags.contains_key("tileFolders") && !s.ui_flags.contains_key("pillFolders"));
}

#[test]
fn one_folder_switch_off_keeps_folders_on() {
    let s = migrated_with_folder_flags(&[("tileFolders", false)]);
    assert_eq!(s.ui_flags.get("folders"), Some(&true));
    let s = migrated_with_folder_flags(&[("tileFolders", true), ("pillFolders", false)]);
    assert_eq!(s.ui_flags.get("folders"), Some(&true));
    assert!(!s.ui_flags.contains_key("tileFolders") && !s.ui_flags.contains_key("pillFolders"));
}

#[test]
fn no_stored_folder_switch_writes_no_folders_key() {
    let s = migrated_with_folder_flags(&[]);
    assert!(!s.ui_flags.contains_key("folders"));
}

#[test]
fn the_folder_switch_merge_runs_once_and_keeps_a_stored_choice() {
    let mut s = migrated_with_folder_flags(&[("tileFolders", false), ("pillFolders", false)]);
    let before = s.ui_flags.clone();
    s.migrate();
    assert_eq!(s.ui_flags, before);
    // An already stored "folders" wins over leftover old keys.
    let s = migrated_with_folder_flags(&[("folders", true), ("tileFolders", false), ("pillFolders", false)]);
    assert_eq!(s.ui_flags.get("folders"), Some(&true));
}

#[test]
fn tile_flags_merge_to_style() {
    let run = |flags: &[(&str, bool)], texts: &[(&str, &str)]| {
        let mut s = Settings::default();
        for (k, v) in flags {
            s.ui_flags.insert((*k).into(), *v);
        }
        for (k, v) in texts {
            s.ui_texts.insert((*k).into(), (*v).into());
        }
        s.migrate();
        for (k, _) in flags {
            assert!(!s.ui_flags.contains_key(*k), "{k} survived migrate");
        }
        s.ui_texts.get("tileStyle").cloned()
    };
    // One style on: that style.
    assert_eq!(run(&[("uppercaseTiles", true)], &[]).as_deref(), Some("uppercase"));
    assert_eq!(run(&[("tileGradient", true)], &[]).as_deref(), Some("gradient"));
    // Several on: the first in the merge order wins.
    assert_eq!(run(&[("italicTiles", true), ("boldTileNames", true)], &[]).as_deref(), Some("bold"));
    assert_eq!(run(&[("compactTiles", true), ("tileTextShadow", true)], &[]).as_deref(), Some("shadow"));
    // Nothing on, or stored off: no key, so the default "plain" applies.
    assert_eq!(run(&[], &[]), None);
    assert_eq!(run(&[("boldTileNames", false)], &[]), None);
    // A stored choice is never overwritten.
    assert_eq!(run(&[("boldTileNames", true)], &[("tileStyle", "mono")]).as_deref(), Some("mono"));
}

#[test]
fn the_attachment_preview_slider_becomes_three_steps() {
    let run = |kb: f64, stored: Option<&str>| {
        let mut s = Settings::default();
        s.ui_values.insert("attachPreviewKb".into(), kb);
        if let Some(v) = stored {
            s.ui_texts.insert("attachPreviewSize".into(), v.into());
        }
        s.migrate();
        assert!(!s.ui_values.contains_key("attachPreviewKb"), "the slider value survived migrate");
        let once = s.ui_texts.get("attachPreviewSize").cloned();
        s.migrate();
        assert_eq!(s.ui_texts.get("attachPreviewSize").cloned(), once, "a second migrate changes nothing");
        once
    };
    assert_eq!(run(2.0, None).as_deref(), Some("short"));
    assert_eq!(run(4.0, None).as_deref(), Some("short"));
    assert_eq!(run(8.0, None), None, "the default stays medium, no key");
    assert_eq!(run(19.0, None), None);
    assert_eq!(run(64.0, None).as_deref(), Some("long"));
    // A choice already made wins over the old slider.
    assert_eq!(run(64.0, Some("medium")).as_deref(), Some("medium"));
}

#[test]
fn old_snip_sizes_carry_into_the_remembered_sizes() {
    let run = |values: &[(&str, f64)], texts: &[(&str, &str)]| {
        let mut s = Settings::default();
        for (k, v) in values {
            s.ui_values.insert((*k).into(), *v);
        }
        for (k, v) in texts {
            s.ui_texts.insert((*k).into(), (*v).into());
        }
        s.migrate();
        for (k, _) in values {
            assert!(!s.ui_values.contains_key(*k), "{k} survived migrate");
        }
        s.ui_texts
            .get("snipToolSizes")
            .map(|j| serde_json::from_str::<serde_json::Value>(j).unwrap())
    };
    let j = run(
        &[("snipPenPx", 10.0), ("snipTextPx", 40.0), ("snipTextOutlinePx", 5.0), ("snipPixelBlock", 20.0), ("snipSpotlightDim", 70.0), ("snipStepPx", 9.0)],
        &[],
    )
    .unwrap();
    assert_eq!(j["pen"]["width"], 10.0);
    assert_eq!(j["text"]["fontPx"], 40.0);
    assert_eq!(j["text"]["outlinePx"], 5.0);
    assert_eq!(j["pixelate"]["blockPx"], 20.0);
    assert_eq!(j["spotlight"]["dim"], 0.7);
    assert_eq!(j["step"]["width"], 9.0);
    assert!(j.get("marker").is_none());
    // An out-of-range old value is not carried.
    let j = run(&[("snipPenPx", 500.0), ("snipRectPx", 7.0)], &[]).unwrap();
    assert!(j.get("pen").is_none());
    assert_eq!(j["rect"]["width"], 7.0);
    // Nothing stored: nothing written. Remembered sizes already stored win.
    assert!(run(&[], &[]).is_none());
    let j = run(&[("snipPenPx", 10.0)], &[("snipToolSizes", "{\"pen\":{\"width\":3}}")]).unwrap();
    assert_eq!(j["pen"]["width"], 3.0);
    // Only the old arrow-head scale: no sizes to carry.
    assert!(run(&[("snipArrowHeadScale", 4.0)], &[]).is_none());
}

#[test]
fn stored_update_interval_is_kept_in_range() {
    let run = |h: f64| {
        let mut s = Settings::default();
        s.ui_values.insert("updateCheckH".into(), h);
        s.migrate();
        s.ui_values.get("updateCheckH").copied()
    };
    // 0 never means "off" here (that is the Auto update switch); it is the shortest wait.
    assert_eq!(run(0.0), Some(1.0));
    assert_eq!(run(0.5), Some(1.0));
    assert_eq!(run(-4.0), Some(1.0));
    assert_eq!(run(500.0), Some(168.0));
    assert_eq!(run(f64::NAN), Some(24.0));
    assert_eq!(run(12.0), Some(12.0));
    // Nothing stored: nothing written.
    let mut s = Settings::default();
    s.migrate();
    assert!(!s.ui_values.contains_key("updateCheckH"));
}

#[test]
fn hover_settings_merge() {
    let run = |flags: &[(&str, bool)], values: &[(&str, f64)]| {
        let mut s = Settings::default();
        for (k, v) in flags {
            s.ui_flags.insert((*k).into(), *v);
        }
        for (k, v) in values {
            s.ui_values.insert((*k).into(), *v);
        }
        s.migrate();
        for (k, _) in flags {
            assert!(!s.ui_flags.contains_key(*k), "{k} survived migrate");
        }
        s
    };
    let dim = |s: &Settings| s.ui_values.get("hoverDim").copied();
    let lift = |s: &Settings| s.ui_values.get("hoverLift").copied();
    // Dim: the strongest dim wins, rounded to a slider step.
    let s = run(&[("tilesHoverDim", true)], &[("tilesHoverOpacity", 45.0)]);
    assert_eq!(dim(&s), Some(55.0));
    assert!(!s.ui_values.contains_key("tilesHoverOpacity"));
    let s = run(&[("toolbarHoverDim", true)], &[]);
    assert_eq!(dim(&s), Some(50.0));
    let s = run(&[("mediaHoverDim", true)], &[]);
    assert_eq!(dim(&s), Some(20.0));
    let s = run(
        &[("tilesHoverDim", true), ("mediaHoverDim", true), ("toolbarHoverDim", true)],
        &[("tilesHoverOpacity", 70.0), ("mediaHoverOpacity", 30.0)],
    );
    assert_eq!(dim(&s), Some(70.0));
    // A switch stored off dims nothing, and nothing stored writes no key.
    let s = run(&[("tilesHoverDim", false), ("toolbarHoverDim", false)], &[("tilesHoverOpacity", 20.0)]);
    assert_eq!(dim(&s), None);
    assert_eq!(dim(&run(&[], &[])), None);
    // A stored strength is never overwritten.
    let s = run(&[("tilesHoverDim", true)], &[("hoverDim", 30.0)]);
    assert_eq!(dim(&s), Some(30.0));
    // Lift: an install that stored none of the keys takes the new default (no key).
    assert_eq!(lift(&run(&[], &[])), None);
    // Stored on, or a stored distance: the lift it showed stays.
    assert_eq!(lift(&run(&[("tileHoverLift", true)], &[("hoverLift", 3.0)])), Some(3.0));
    assert_eq!(lift(&run(&[("tileHover", true)], &[])), Some(1.0));
    assert_eq!(lift(&run(&[], &[("hoverLift", 4.0)])), Some(4.0));
    // Either switch stored off means no lift.
    assert_eq!(lift(&run(&[("tileHoverLift", false)], &[("hoverLift", 3.0)])), None);
    assert_eq!(lift(&run(&[("tileHover", false)], &[])), None);
    // A lift of 0 (off) with no old switch left stays 0 through repeated launches.
    let mut s = run(&[], &[("hoverLift", 0.0)]);
    s.migrate();
    s.migrate();
    assert_eq!(lift(&s), Some(0.0));
}

#[test]
fn removed_key_lists_have_no_duplicates() {
    for list in [removed_ui_value_keys(), removed_ui_flag_keys(), removed_ui_text_keys()] {
        let mut seen = std::collections::HashSet::new();
        for k in list {
            assert!(seen.insert(*k), "{k} is listed twice");
        }
    }
}

#[test]
fn keys_of_removed_controls_are_dropped() {
    // The history window, the inbox badge, the view groups and the placeholder
    // window switch are gone; their stored keys must not linger.
    let mut s = Settings::default();
    for k in ["showJournal", "varPopupWindow", "viewGroups", "pausedIcons"] {
        s.ui_flags.insert(k.into(), false);
    }
    for k in ["journalWidth", "viewDropHoverMs", "viewGroupMaxRows", "clipSeenId"] {
        s.ui_values.insert(k.into(), 7.0);
    }
    assert!(s.has_removed_ui_values());
    s.migrate();
    assert!(s.ui_flags.is_empty(), "left: {:?}", s.ui_flags);
    for k in ["journalWidth", "viewDropHoverMs", "viewGroupMaxRows", "clipSeenId"] {
        assert!(!s.ui_values.contains_key(k), "{k} survived migrate");
    }
}

#[test]
fn fixed_size_and_timing_values_are_dropped_on_load() {
    // These expert rows became constants; a stored number must not linger or
    // come back if a row of the same name is ever added again.
    let keys = [
        "tooltipWidth", "scrollbarWidth", "toastWidth", "toastMs", "modalRadius", "modalScale",
        "loupeCells", "pickSwatchPx", "tagChipPx", "tagPopCols", "searchDebounceMs",
    ];
    let mut s = Settings::default();
    for k in keys {
        s.ui_values.insert(k.into(), 7.0);
    }
    s.ui_values.insert("tooltipDelayMs".into(), 0.0);
    assert!(s.has_removed_ui_values());
    s.migrate();
    for k in keys {
        assert!(!s.ui_values.contains_key(k), "{k} survived migrate");
    }
    assert_eq!(s.ui_values.get("tooltipDelayMs"), Some(&0.0), "a live value next to them stays");
    assert!(!s.has_removed_ui_values());
}

#[test]
fn the_library_colour_filter_switch_is_dropped_on_load() {
    let mut s = Settings::default();
    s.ui_flags.insert("colorFilter".into(), false);
    s.ui_flags.insert("libraryTypeFilter".into(), false);
    assert!(s.has_removed_ui_values());
    s.migrate();
    assert!(!s.ui_flags.contains_key("colorFilter"), "colorFilter survived migrate");
    assert_eq!(s.ui_flags.get("libraryTypeFilter"), Some(&false), "the neighbouring switch stays");
    assert!(!s.has_removed_ui_values());
}

#[test]
fn visual_effect_switches_and_their_blur_value_are_dropped_on_load() {
    // The five effects are gone, and tileBlur only ever belonged to the first one.
    let flags = ["blurTilesUntilHover", "grayscaleMedia", "hideTileNames", "accentScrollbar", "smoothScroll"];
    let mut s = Settings::default();
    for k in flags {
        s.ui_flags.insert(k.into(), true);
    }
    s.ui_flags.insert("blurMediaUntilHover".into(), true);
    s.ui_values.insert("tileBlur".into(), 9.0);
    s.ui_values.insert("frostedBlur".into(), 9.0);
    assert!(s.has_removed_ui_values());
    s.migrate();
    for k in flags {
        assert!(!s.ui_flags.contains_key(k), "{k} survived migrate");
    }
    assert!(!s.ui_values.contains_key("tileBlur"), "tileBlur survived migrate");
    assert_eq!(s.ui_flags.get("blurMediaUntilHover"), Some(&true), "the kept blur switch stays");
    assert_eq!(s.ui_values.get("frostedBlur"), Some(&9.0), "the kept blur strength stays");
    assert!(!s.has_removed_ui_values());
}

#[test]
fn fine_tuning_values_and_safety_net_switches_are_dropped_on_load() {
    // These rows became constants and the sub-switches ship on for good; a stored
    // value must not linger, and a second pass must change nothing.
    let values = [
        "gridPad", "gridGap", "overlayDim", "animSpeed", "dragThreshold", "disarmMs", "composerMaxH",
        "modalMaxVh", "composerScale", "primaryScale", "snipPreviewVh", "ocrUpscaleUnderPx",
        "ocrIndexMaxChars", "recDiskWarnMb", "recAudioKbps", "keyOverlayHoldMs", "videoClipTtlMin",
        "syncPollSecs", "hlFontPx", "historyPreviewLen", "rotateTipLen", "trayLabelMax", "favStarSize",
        "viewBorder", "tileHotkeyBadgeOpacity", "mediaRadius", "mediaBlur", "snipEditUndoMax",
        "videoEditUndoMax", "savedSearchMax",
    ];
    let flags = [
        "floatPark", "etaShow", "chainVarCheck", "ocrAutoPickLang", "voiceAutoPickLang", "ocrLangAutoUpdate",
        "voiceLangAutoUpdate", "helpTips", "helpHighlight", "helpSketches", "historyDiff", "veOvRemember",
    ];
    let texts = ["snipScaleFilter", "fuzzyMaxTypos"];
    let mut s = Settings::default();
    for k in values {
         s.ui_values.insert(k.into(), 7.0);
    }
    for k in flags {
         s.ui_flags.insert(k.into(), false);
    }
    for k in texts {
         s.ui_texts.insert(k.into(), "x".into());
    }
    // Neighbours that stay: a value, a switch and a dropdown of the same screens.
    s.ui_values.insert("libraryWidth".into(), 700.0);
    s.ui_flags.insert("copyFlash".into(), false);
    s.ui_texts.insert("tileStyle".into(), "bold".into());
    assert!(s.has_removed_ui_values());
    s.migrate();
    for k in values {
         assert!(!s.ui_values.contains_key(k), "{k} survived migrate");
    }
    for k in flags {
         assert!(!s.ui_flags.contains_key(k), "{k} survived migrate");
    }
    for k in texts {
         assert!(!s.ui_texts.contains_key(k), "{k} survived migrate");
    }
    assert_eq!(s.ui_values.get("libraryWidth"), Some(&700.0));
    assert_eq!(s.ui_flags.get("copyFlash"), Some(&false));
    assert_eq!(s.ui_texts.get("tileStyle").map(String::as_str), Some("bold"));
    assert!(!s.has_removed_ui_values());
    let once = serde_json::to_string(&s).unwrap();
    s.migrate();
    assert_eq!(serde_json::to_string(&s).unwrap(), once, "a second migrate changes nothing");
}

#[test]
fn removed_flags_are_dropped() {
    // Every listed key, whatever the stored value (true or false), is dropped
    // once; a live switch next to it is untouched.
    for stored in [true, false] {
        let mut s = Settings::default();
        for k in removed_ui_flag_keys() {
            s.ui_flags.insert((*k).into(), stored);
        }
        s.ui_flags.insert("barToggles".into(), false);
        assert!(s.has_removed_ui_values());
        s.migrate();
        for k in removed_ui_flag_keys() {
            assert!(!s.ui_flags.contains_key(*k), "{k} survived migrate");
        }
        assert_eq!(s.ui_flags.get("barToggles"), Some(&false));
        assert!(!s.has_removed_ui_values());
    }
}

#[test]
fn fav_view_merges_into_star() {
    let run = |view: Option<bool>, star: Option<bool>| {
        let mut s = Settings::default();
        if let Some(v) = view {
            s.ui_flags.insert("favView".into(), v);
        }
        if let Some(v) = star {
            s.ui_flags.insert("favViewButton".into(), v);
        }
        let dirty = s.has_removed_ui_values();
        s.migrate();
        assert!(!s.ui_flags.contains_key("favView"));
        assert_eq!(dirty, view.is_some());
        s.ui_flags.get("favViewButton").copied()
    };
    // Visible before (view on, star never hidden): stays on.
    assert_eq!(run(Some(true), None), Some(true));
    assert_eq!(run(Some(true), Some(true)), Some(true));
    // View on but star hidden, or view off: falls to the new default (off, no key).
    assert_eq!(run(Some(true), Some(false)), None);
    assert_eq!(run(Some(false), None), None);
    assert_eq!(run(Some(false), Some(true)), None);
    // No old key: a stored star value is the one source and is left alone.
    assert_eq!(run(None, Some(true)), Some(true));
    assert_eq!(run(None, None), None);
}

#[test]
fn removed_fine_tuning_values_keep_their_former_defaults() {
    assert_eq!(ERROR_LOG_MAX, 100);
    assert_eq!(OCR_LANG_CACHE_H, 24.0);
    assert_eq!(VOICE_LANG_CACHE_H, 24.0);
    assert_eq!(REC_BROKEN_GRACE_MIN, 5.0);
    assert_eq!(REC_SWEEP_EVERY_MIN, 60.0);
    assert_eq!(CLIP_SECRET_MIN_LEN, 8);
    assert_eq!(OCR_UPSCALE_UNDER_PX, 300);
    assert_eq!(OCR_INDEX_MAX_CHARS, 20000);
    assert_eq!(crate::rec_cmds::REC_DISK_WARN_MB, 500.0);
    assert_eq!(crate::rec_cmds::REC_AUDIO_KBPS, 192);
    assert_eq!(crate::keylog::HOLD_MS, 400);
    assert_eq!(TRAY_LABEL_MAX, 40);
    assert_eq!(SYNC_POLL_SECS, 5);
    assert_eq!(CLIP_EXPORT_TTL_SECS, 3600);
}

#[test]
fn old_settings_json_migrates() {
    let json = r#"{
            "theme":"dark","language":"de","minimize_to_tray":true,
            "copy_history":["a","b"],"ui_flags":{"floating":false},
            "tile_size":18,"removed_future_field":42
        }"#;
    let s: Settings = serde_json::from_str(json).expect("old settings must still parse");
    assert_eq!(s.theme, "dark");
    assert_eq!(s.language, "de");
    assert!(s.minimize_to_tray);
    assert_eq!(s.ui_flags.get("floating"), Some(&false));
    assert_eq!(s.tile_size, 18);
    assert!(s.copy_log.is_empty()); // new field defaults
    assert!(s.usage.is_empty());
    assert!(s.auto_update); // missing field -> default_on
}

// Prompt JSON from an older version: removed field (favorite) ignored, missing
// new fields default; the prompt still loads.
#[test]
fn old_prompt_json_migrates() {
    let json = r#"{"id":"x1","name":"Old","text":"hi","favorite":true,"show_image":true}"#;
    let p: Prompt = serde_json::from_str(json).expect("old prompt must still parse");
    assert_eq!(p.id, "x1");
    assert_eq!(p.name, "Old");
    assert!(p.show_image);
    assert_eq!(p.color, ""); // missing -> default
    assert_eq!(p.caption_size, 0);
}

#[test]
fn txt_roundtrip_keeps_every_field() {
    let out = to_txt(&sample_prompts());
    let data = parse_txt(&out);
    assert_eq!(data.prompts.len(), 2);
    let p1 = &data.prompts[0];
    assert_eq!(p1.name, "Mail");
    assert_eq!(p1.text, "Hello\nWorld");
    assert_eq!(p1.color, "#ef4444");
    assert_eq!(p1.font, "mono");
    assert_eq!(p1.font_size, 24);
    assert_eq!(p1.image, "data:image/png;base64,QUJD");
    assert!(p1.show_image && p1.copy_image);
    let p2 = &data.prompts[1];
    assert_eq!(p2.font, "");
    assert_eq!(p2.font_size, 1);
    assert_eq!(p2.file_path, "C:\\tmp\\report.pdf");
    assert_eq!(p2.pdf_page, 3);
    assert_eq!(p1.pdf_page, 0, "the omitted @pdfpage line must default to 0");
    assert_eq!(p2.icon_path, "C:\\tmp\\clip.mp4");
    assert_eq!(p2.caption, "Mein Untertitel");
    assert_eq!(p2.caption_size, 18);
    assert!(p2.show_image && !p2.copy_image);
    assert_eq!(p1.tags, vec!["Work".to_string(), "E-Mail".to_string()]);
    assert!(p2.tags.is_empty());
}

#[test]
fn csv_roundtrip_keeps_every_field() {
    let out = to_csv(&sample_prompts());
    let rows = parse_csv(&out);
    // header + 2 prompts (no settings/views blocks in a prompts-only export)
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1][0], "Mail");
    assert_eq!(rows[1][4], "mono");
    assert_eq!(rows[1][5], "24");
    assert_eq!(rows[1][8], "1");
    assert_eq!(rows[1][9], "1");
    assert_eq!(rows[1][10], "data:image/png;base64,QUJD");
    assert_eq!(rows[2][6], "C:\\tmp\\report.pdf");
    assert_eq!(rows[2][7], "C:\\tmp\\clip.mp4");
    assert_eq!(rows[2][11], "Mein Untertitel");
    assert_eq!(rows[2][12], "18");
    assert_eq!(rows[1][13], "Work,E-Mail");
    assert_eq!(rows[2][13], "");
    assert_eq!(rows[1][15], "0");
    assert_eq!(rows[2][15], "3");
}

// A choice variable is plain prompt text, so neither exporter may touch
// its `|` or its commas: a mangled token would silently lose the options.
#[test]
fn choice_variable_survives_txt_and_csv_export() {
    const BODY: &str = "Write a {?tone|formal,casual,witty} note about {?Topic}.";
    let mut prompts = sample_prompts();
    prompts[0].text = BODY.into();
    prompts[0].image = String::new(); // a text prompt, not the media sample
    prompts[0].show_image = false;
    prompts[0].copy_image = false;
    assert_eq!(parse_txt(&to_txt(&prompts)).prompts[0].text, BODY);
    assert_eq!(parse_csv(&to_csv(&prompts))[1][1], BODY);
    assert_eq!(parse_csv_data(&to_csv(&prompts)).prompts[0].text, BODY);
}

#[test]
fn json_roundtrip_keeps_every_field_including_favorite() {
    let mut prompts = sample_prompts();
    prompts[0].favorite = true; // the one field CSV/TXT cannot carry
    let out = to_json(&prompts);
    let data = parse_json_data(&out).expect("valid JSON");
    assert_eq!(data.prompts.len(), 2);
    let p1 = &data.prompts[0];
    assert_eq!(p1.name, "Mail");
    assert_eq!(p1.text, "Hello\nWorld");
    assert_eq!(p1.color, "#ef4444");
    assert_eq!(p1.font, "mono");
    assert_eq!(p1.font_size, 24);
    assert_eq!(p1.image, "data:image/png;base64,QUJD");
    assert!(p1.show_image && p1.copy_image);
    assert_eq!(p1.tags, vec!["Work".to_string(), "E-Mail".to_string()]);
    assert_eq!(p1.notes, "Ask Anna first.\nDeadline: Friday");
    assert!(p1.favorite, "JSON export must keep favorite, unlike CSV/TXT");
    let p2 = &data.prompts[1];
    assert_eq!(p2.file_path, "C:\\tmp\\report.pdf");
    assert_eq!(p2.pdf_page, 3);
    assert_eq!(p2.icon_path, "C:\\tmp\\clip.mp4");
    assert_eq!(p2.caption, "Mein Untertitel");
    assert_eq!(p2.caption_size, 18);
    assert!(!p2.favorite);
}

// image_text is a search index, never exported: ExportPrompt
// simply has no such field, proven here the same way slim_prompt's own
// safety tests prove it for the state payload and the picture bytes.
#[test]
fn json_export_never_carries_image_text() {
    let mut prompts = sample_prompts();
    prompts[0].image_text = "invoice total due friday".into();
    let out = to_json(&prompts);
    assert!(!out.contains("invoice"), "OCR-recognized text leaked into the export: {out}");
    assert!(!out.contains("image_text"), "the export still has an image_text field: {out}");
}

#[test]
fn json_roundtrip_keeps_rotating_entries_as_a_real_array() {
    let mut prompts = sample_prompts();
    prompts[0].variants = vec!["Second entry".to_string(), "Third, entry\nwith a newline".to_string()];
    prompts[0].rotate_mode = "random".to_string();
    let data = parse_json_data(&to_json(&prompts)).expect("valid JSON");
    let p = &data.prompts[0];
    assert_eq!(p.text, prompts[0].text, "text must not carry a variant-join marker in JSON");
    assert_eq!(p.variants, prompts[0].variants);
    assert_eq!(p.rotate_mode, "random");
}

#[test]
fn json_export_never_carries_machine_local_or_session_state() {
    let out = to_json(&sample_prompts());
    assert!(!out.contains("hotkey"), "hotkey bindings are machine-local, never exported");
    assert!(!out.contains("rotate_auto"), "rotate_auto is live per-session state, never exported");
    assert!(!out.contains("trashed_at"));
    assert!(!out.contains("\"id\""));
}

#[test]
fn parse_json_data_rejects_invalid_json_with_a_friendly_error() {
    match parse_json_data("not json") {
        Err(msg) => assert_eq!(msg, "the file is not valid JSON"),
        Ok(_) => panic!("\"not json\" must not parse as valid JSON"),
    }
}

#[test]
fn parse_json_data_skips_entries_with_neither_name_nor_text() {
    let data = parse_json_data(r#"[{"name":"","text":""},{"name":"Ok","text":"x"}]"#)
        .expect("valid JSON");
    assert_eq!(data.prompts.len(), 1);
    assert_eq!(data.prompts[0].name, "Ok");
}

#[test]
fn csv_export_feeds_import_parser_losslessly() {
    let out = to_csv(&sample_prompts());
    let data = parse_csv_data(&out);
    assert_eq!(data.prompts.len(), 2);
    let p1 = &data.prompts[0];
    assert!(p1.show_image && p1.copy_image);
    assert_eq!(p1.image, "data:image/png;base64,QUJD");
    assert_eq!(p1.text, "Hello\nWorld");
    assert_eq!(p1.tags, vec!["Work".to_string(), "E-Mail".to_string()]);
    let p2 = &data.prompts[1];
    assert_eq!(p2.file_path, "C:\\tmp\\report.pdf");
    assert_eq!(p2.pdf_page, 3);
    assert_eq!(p1.pdf_page, 0);
    assert_eq!(p2.icon_path, "C:\\tmp\\clip.mp4");
    assert_eq!(p2.caption, "Mein Untertitel");
    assert_eq!(p2.caption_size, 18);
    assert!(p2.tags.is_empty());
}

// An archived entry is just a prompt carrying a reserved tag -- neither
// export nor backup ever filters by tag, so both already carry it through
// unchanged. This proves that stays true rather than assuming it.
#[test]
fn an_archived_prompt_is_included_in_export_and_backup() {
    let mut prompts = sample_prompts();
    prompts[0].tags.push("Archive".to_string());
    // Export: CSV and JSON both keep the tag on round trip.
    let data = parse_csv_data(&to_csv(&prompts));
    assert!(data.prompts[0].tags.contains(&"Archive".to_string()));
    let json = to_json(&prompts);
    assert!(json.contains("Archive"), "the archived prompt's tag did not make it into the JSON export");
    // Backup: the same content survives the real encrypt/decrypt round trip
    // backup_password_roundtrip_and_wrong_password_fails already proves is
    // byte-exact, here specifically carrying the archive tag through it.
    let blob = backup_encrypt(json.as_bytes(), None).expect("encrypt");
    let restored = backup_decrypt(&blob, None).expect("decrypt").expect("psb");
    assert_eq!(restored, json.as_bytes());
}

#[test]
fn is_archived_matches_only_the_reserved_tag_case_insensitively() {
    let mut p = sample_prompts()[0].clone();
    p.tags = vec!["Archive".to_string()];
    assert!(is_archived(&p, Some("archive")));
    assert!(!is_archived(&p, Some("something-else")));
    assert!(!is_archived(&p, None));
}

// An entry that happens to contain the join marker itself must not be
// split into extra entries on round trip.
#[test]
fn join_variants_survives_an_entry_containing_the_separator_verbatim() {
    let text = "a\n<!-- next -->\nb".to_string(); // looks like 2 entries, is 1
    let variants = vec!["c".to_string()];
    let joined = join_variants(&text, &variants);
    let (out_text, out_variants) = split_variants(&joined);
    assert_eq!(out_text, text);
    assert_eq!(out_variants, variants);
}

// Rotating-prompt entries round-trip through every export/storage format
// without loss. Each format gets its OWN test since each chose its own way to
// embed the entries and could silently regress independently of the others.
#[test]
fn csv_roundtrip_keeps_rotating_entries() {
    let mut prompts = sample_prompts();
    prompts[0].variants = vec!["Second entry".to_string(), "Third, entry\nwith a newline".to_string()];
    prompts[0].rotate_mode = "random".to_string();
    let data = parse_csv_data(&to_csv(&prompts));
    let p = &data.prompts[0];
    assert_eq!(p.text, prompts[0].text);
    assert_eq!(p.variants, prompts[0].variants);
    assert_eq!(p.rotate_mode, "random");
}

#[test]
fn txt_roundtrip_keeps_rotating_entries_and_never_confuses_them_with_the_block_separator() {
    let mut prompts = sample_prompts();
    // Deliberately contains a bare "---" line inside an entry: the whole point
    // of VARIANT_SEP being a distinct marker is that this must NOT be mistaken
    // for the "\n\n---\n\n" boundary between two DIFFERENT prompts.
    prompts[0].variants = vec![
        "Second entry".to_string(),
        "---\nlooks like a separator, isn't one".to_string(),
    ];
    prompts[0].rotate_mode = "random".to_string();
    let data = parse_txt(&to_txt(&prompts));
    assert_eq!(data.prompts.len(), prompts.len(), "a variant containing \"---\" must never split into an extra prompt");
    let p = &data.prompts[0];
    assert_eq!(p.text, prompts[0].text);
    assert_eq!(p.variants, prompts[0].variants);
    assert_eq!(p.rotate_mode, "random");
}

#[test]
fn markdown_sync_roundtrip_keeps_rotating_entries() {
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["Second entry".to_string(), "Third entry".to_string()];
    let md = prompt_to_md(&p, &SyncPlacement::default());
    assert!(md.contains("variants: 2"), "the header count is informational but should still be right");
    let restored = apply_md_to_prompt(&p, &md)
        .expect("a file this function itself just wrote must always be recognised as ours");
    assert_eq!(restored.text, p.text);
    assert_eq!(restored.variants, p.variants);
}

#[test]
fn markdown_sync_survives_a_hand_edit_that_removes_every_entry() {
    // The sync folder is the user's to hand-edit: deleting
    // every "<!-- next -->" marker must degrade to a plain prompt, never crash,
    // and never leave rotate_mode stranded on an empty list.
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["Second entry".to_string()];
    p.rotate_mode = "random".to_string();
    let md = prompt_to_md(&p, &SyncPlacement::default()).replace(&format!("{VARIANT_SEP}Second entry"), "");
    let restored = apply_md_to_prompt(&p, &md).unwrap();
    assert!(restored.variants.is_empty());
    assert_eq!(restored.rotate_mode, "", "empty variants must never leave a stranded rotate_mode");
}

#[test]
fn backup_json_roundtrips_rotating_fields() {
    let mut p = sample_prompts().remove(0);
    p.variants = vec!["Second".to_string(), "Third".to_string()];
    p.rotate_mode = "random".to_string();
    let backup = Backup { prompts: vec![p.clone()], settings: Settings::default() };
    let json = serde_json::to_string(&backup).unwrap();
    let restored: Backup = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.prompts[0].variants, p.variants);
    assert_eq!(restored.prompts[0].rotate_mode, p.rotate_mode);
}

#[test]
fn version_history_snapshots_and_restores_rotating_entries() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');",
    )
    .unwrap();
    let variants = vec!["b".to_string(), "c".to_string()];
    db_add_version(&conn, "p1", 100, "Name", "Entry one", &variants, 20).unwrap();
    let listed = db_list_versions(&conn, "p1");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].variants, variants);
    let (pid, name, text, restored_variants) = db_get_version(&conn, listed[0].id).unwrap();
    assert_eq!(pid, "p1");
    assert_eq!(name, "Name");
    assert_eq!(text, "Entry one");
    assert_eq!(restored_variants, variants);
}

#[test]
fn restore_version_as_new_keeps_the_source_and_starts_fresh_history() {
    let mut source = sample_prompts().remove(0);
    source.color = "#112233".to_string();
    source.image = "data:image/png;base64,AAA".to_string();
    source.favorite = true;
    source.tags = vec!["work".to_string()];
    let original_text = source.text.clone();
    let variants = vec!["Second".to_string()];
    let created = build_restored_prompt(&source, "Old name", "Old text".to_string(), variants.clone(), "(from 2026-01-01)");
    assert_eq!(source.text, original_text, "building the copy must never touch the source prompt");
    assert_eq!(created.name, "Old name (from 2026-01-01)");
    assert_eq!(created.text, "Old text");
    assert_eq!(created.variants, variants);
    assert_eq!(created.color, source.color, "the colour marker comes from the current prompt, not the version");
    assert_eq!(created.image, source.image, "the image comes from the current prompt, not the version");
    assert_eq!(created.tags, source.tags, "tags come from the current prompt too, the version never stored them");
    assert_ne!(created.id, source.id);
    assert!(!created.favorite, "a restored-as-new copy starts unstarred like any other new entry");
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');",
    )
    .unwrap();
    assert!(db_list_versions(&conn, &created.id).is_empty(), "a restored-as-new entry starts with no history of its own");
}

#[test]
fn old_export_with_settings_imports_prompts_only() {
    // A legacy file that still carries @settings/@views must import its prompts
    // and silently ignore the settings blocks (no panic, no bogus prompt).
    let txt = "@settings\nlanguage=de\ntheme=midnight\n\n---\n\n@views\nv|View\n\n---\n\n### Old\nSome text";
    let data = parse_txt(txt);
    assert_eq!(data.prompts.len(), 1);
    assert_eq!(data.prompts[0].name, "Old");
}

#[test]
fn old_exports_without_style_still_parse() {
    let txt = "### Old\nSome text\n@color #123456";
    let data = parse_txt(txt);
    assert_eq!(data.prompts.len(), 1);
    assert_eq!(data.prompts[0].text, "Some text");
    assert_eq!(data.prompts[0].color, "#123456");
    assert_eq!(data.prompts[0].font, "");
    assert_eq!(data.prompts[0].font_size, 0);
    assert_eq!(data.prompts[0].file_path, "");
}

#[test]
fn version_compare_pads_components() {
    assert!(version_newer("1.9.0", "1.8.9"));
    assert!(version_newer("2.0", "1.9.9"));
    assert!(!version_newer("1.9", "1.9.0")); // equal once padded
    assert!(!version_newer("1.9.0", "1.9"));
    assert!(!version_newer("1.8.0", "1.9.0"));
    assert!(version_newer("1.10.0", "1.9.0")); // numeric, not lexical
}
