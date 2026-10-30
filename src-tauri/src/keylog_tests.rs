use super::*;
use std::fs;
// Only used by the Pane/Custom test below; imported here so a non-test build has no
// unused-import warning.
use windows::Win32::UI::Accessibility::{UIA_CustomControlTypeId, UIA_PaneControlTypeId};

#[test]
fn resume_reinstalls_the_hook_only_while_the_flag_is_on() {
    assert!(hook_install_wanted(true, false));
    assert!(!hook_install_wanted(false, false));
    assert!(!hook_install_wanted(true, true));
    assert!(!hook_install_wanted(false, true));
}

#[test]
fn a_start_is_refused_only_while_the_live_flag_is_off() {
    assert_eq!(claim_start(FLAG_UNKNOWN), Some(FLAG_ON));
    assert_eq!(claim_start(FLAG_ON), Some(FLAG_ON));
    assert_eq!(claim_start(FLAG_OFF), None);
}

#[test]
fn recorder_stores_identifiers_with_timestamps_only() {
    let mut rec = Recorder::default();
    rec.record(0x41, 0, true, 0, false);
    rec.record(0x41, 0, false, 120, false);
    assert_eq!(
        rec.sidecar.events,
        vec![KeyEvent { vk: 0x41, down: true, t_ms: 0 }, KeyEvent { vk: 0x41, down: false, t_ms: 120 }]
    );
    assert!(rec.sidecar.gaps.is_empty());
}

#[test]
fn a_blocked_down_never_grows_open_downs() {
    let mut rec = Recorder::default();
    for scan in 0..(OPEN_DOWNS_CAP as u32 * 2) {
        // Distinct blocked downs (an injected flood) must not grow open_downs: a blocked down
        // removes instead of inserting.
        rec.record(0x41, scan, true, scan as u64, true);
    }
    assert_eq!(rec.open_downs.len(), 0, "a blocked down must never be stored in open_downs");
}

#[test]
fn open_downs_past_its_cap_is_cleared_rather_than_growing_further() {
    let mut rec = Recorder::default();
    for scan in 0..(OPEN_DOWNS_CAP as u32) {
        // Accepted downs for distinct keys with no up are the only way open_downs grows; it stays
        // at the cap.
        rec.record(0x41, scan, true, scan as u64, false);
    }
    assert_eq!(rec.open_downs.len(), OPEN_DOWNS_CAP);
    // One more accepted down past the cap clears the map.
    rec.record(0x41, OPEN_DOWNS_CAP as u32, true, OPEN_DOWNS_CAP as u64, false);
    assert_eq!(rec.open_downs.len(), 0, "open_downs must be cleared once it grows past the defensive cap");
}

#[test]
fn events_stop_growing_past_the_cap_and_leave_a_gap_instead() {
    let mut rec = Recorder::default();
    for i in 0..(EVENTS_CAP / 2) {
        let t = (i * 2) as u64;
        rec.record(0x41, 0, true, t, false);
        rec.record(0x41, 0, false, t + 1, false);
    }
    assert_eq!(rec.sidecar.events.len(), EVENTS_CAP, "must reach the cap exactly, not exceed it");
    // A key past the cap adds no event and extends a gap instead.
    rec.record(0x43, 0, true, EVENTS_CAP as u64, false);
    assert_eq!(rec.sidecar.events.len(), EVENTS_CAP, "an event past the cap must never be stored");
    assert_eq!(rec.sidecar.gaps.len(), 1, "the overlay must show a gap where events stopped, not silent truncation");
    rec.record(0x43, 0, false, EVENTS_CAP as u64 + 10, false);
    assert_eq!(rec.sidecar.events.len(), EVENTS_CAP);
    assert_eq!(rec.sidecar.gaps.len(), 1, "further capped keys extend the same gap rather than opening new ones");
}

#[test]
fn a_password_field_produces_a_gap_instead_of_events() {
    let mut rec = Recorder::default();
    rec.record(0x41, 0, true, 0, false);
    rec.record(0x42, 0, true, 100, true); // focus moves into a password field
    rec.record(0x42, 0, false, 150, true);
    rec.record(0x43, 0, true, 400, false); // focus leaves it again
    assert_eq!(rec.sidecar.events.len(), 2);
    assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 100, end_ms: 150 }]);
}

#[test]
fn back_to_back_password_periods_extend_one_gap_rather_than_opening_a_new_one() {
    let mut rec = Recorder::default();
    rec.record(0x41, 0, true, 0, true);
    rec.record(0x41, 0, false, 10, true);
    rec.record(0x42, 0, true, 20, true);
    assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 0, end_ms: 20 }]);
}

#[test]
fn a_recording_entirely_in_a_password_field_writes_no_events_at_all() {
    let mut rec = Recorder::default();
    rec.record(0x41, 0, true, 0, true);
    rec.record(0x41, 0, false, 50, true);
    assert!(rec.sidecar.events.is_empty());
    assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 0, end_ms: 50 }]);
}

#[test]
fn the_switch_off_default_never_starts_a_capture() {
    let flags = std::collections::HashMap::new();
    assert!(!enabled(&flags)); // absent: off is the shipped default
    maybe_start(enabled(&flags), 400, mono_ms(), shortcuts_only(&flags));
    assert!(slot().lock().unwrap().is_none());
}

#[test]
fn the_sidecar_shape_carries_no_text_field_at_all() {
    // KeyEvent has three fields (vk, down, t_ms) and Gap two, with nowhere for a character,
    // string or window title to land. Checking the serialized key set proves the shape.
    let sidecar = KeySidecar {
        events: vec![KeyEvent { vk: 0x41, down: true, t_ms: 0 }],
        gaps: vec![Gap { start_ms: 10, end_ms: 20 }],
        hold_ms: 400,
    };
    let json = serde_json::to_value(&sidecar).unwrap();
    let event = &json["events"][0];
    let mut keys: Vec<&str> = event.as_object().unwrap().keys().map(|s| s.as_str()).collect();
    keys.sort();
    assert_eq!(keys, vec!["down", "t_ms", "vk"]);
    let gap = &json["gaps"][0];
    let mut gap_keys: Vec<&str> = gap.as_object().unwrap().keys().map(|s| s.as_str()).collect();
    gap_keys.sort();
    assert_eq!(gap_keys, vec!["end_ms", "start_ms"]);
}

#[test]
fn written_sidecar_is_sealed_on_disk_and_parses_back() {
    let dir = std::env::temp_dir().join(format!("ps-keys-sealed-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let video = dir.join("rec-sealed.mp4");
    let sc = KeySidecar { hold_ms: 777, ..Default::default() };
    write_sidecar(&video, &sc);
    let raw = fs::read_to_string(crate::sidecar_path(&video, crate::KEYS_SIDECAR_EXT)).unwrap();
    assert!(!raw.contains("hold_ms") && !raw.starts_with('{'), "the sidecar must not sit on disk as plain JSON");
    assert_eq!(crate::parse_key_sidecar(&raw), Some(sc));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_old_plain_sidecar_still_parses() {
    let parsed = crate::parse_key_sidecar(r#"{"events":[],"gaps":[],"hold_ms":500}"#).unwrap();
    assert_eq!(parsed.hold_ms, 500);
}

#[test]
fn past_cutoff_only_fires_once_a_cutoff_is_actually_set() {
    assert!(!past_cutoff(1_000, None)); // no switch in progress: nothing is cut off
    assert!(!past_cutoff(999, Some(1_000))); // strictly before the switch instant
    assert!(past_cutoff(1_000, Some(1_000))); // AT the switch instant: already cut off
    assert!(past_cutoff(1_500, Some(1_000))); // after the switch instant
}

// An injected key gaps exactly like a password field: the consumer records verdict's
// answer before calling record().
#[test]
fn an_injected_key_is_gapped_the_same_way_a_password_field_is() {
    let mut rec = Recorder::default();
    rec.record(0x41, 0, true, 0, false);
    let injected_or_password = true; // stands for verdict()'s Some(true)
    rec.record(0x56, 0, true, 50, injected_or_password); // a password-manager auto-type
    rec.record(0x56, 0, false, 55, injected_or_password);
    rec.record(0x42, 0, true, 200, false);
    assert_eq!(rec.sidecar.events.len(), 2);
    assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 50, end_ms: 55 }]);
}

// Down/up taint (Recorder::open_downs): an up is stored only alongside a down this
// Recorder accepted.

#[test]
fn an_up_is_dropped_when_its_matching_down_was_gapped_even_if_the_up_itself_looks_clean() {
    // A password key's down is gapped, focus later moves to a normal field, and the up (clean
    // on its own) is still dropped.
    let mut rec = Recorder::default();
    rec.record(0x41, 0x1E, true, 100, true); // down, gapped (password)
    rec.record(0x41, 0x1E, false, 900, false); // up, its own verdict now clean
    assert!(rec.sidecar.events.is_empty(), "a tainted down must take its up down with it");
    assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 100, end_ms: 900 }]);
}

#[test]
fn an_up_with_no_matching_down_in_this_capture_is_dropped_too() {
    // A key held from before this capture has no accepted down, so its up proves nothing.
    let mut rec = Recorder::default();
    rec.record(0x41, 0x1E, false, 100, false); // up only, own verdict clean
    assert!(rec.sidecar.events.is_empty());
    assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 100, end_ms: 100 }]);
}

#[test]
fn a_clean_down_and_its_matching_up_are_both_stored_normally() {
    let mut rec = Recorder::default();
    rec.record(0x41, 0x1E, true, 0, false);
    rec.record(0x41, 0x1E, false, 120, false);
    assert_eq!(rec.sidecar.events.len(), 2);
    assert!(rec.sidecar.gaps.is_empty());
}

#[test]
fn different_scan_codes_under_the_same_vk_are_tainted_independently() {
    // Left and right Ctrl share a VK; gapping one must not taint the other's pair.
    let mut rec = Recorder::default();
    rec.record(0x11, 0x1D, true, 0, true); // left ctrl down, gapped
    rec.record(0x11, 0x9D, true, 10, false); // right ctrl down, clean
    rec.record(0x11, 0x9D, false, 20, false); // right ctrl up, clean, matches its own down
    assert_eq!(rec.sidecar.events.len(), 2, "the clean scan code's pair must record despite the other one being gapped");
}

// The verdict algorithm (Ledger + verdict): a delayed verdict with a settle check, a guard
// window scanned for dirty checks, and a blind window around every focus change. All pure.

#[test]
fn verdict_is_pending_until_a_check_has_started_late_enough_to_speak_for_the_key() {
    // A key at t=0 needs a check that started at t>=300 (SETTLE_MS); an earlier one is not
    // enough.
    let mut ledger = Ledger::default();
    ledger.push_check(0, FieldClass::NonText);
    assert_eq!(verdict(&ledger, 0, false), None, "a same-instant check does not qualify");
    ledger.push_check(299, FieldClass::NonText);
    assert_eq!(verdict(&ledger, 0, false), None, "still one ms short of the settle deadline");
    ledger.push_check(300, FieldClass::NonText);
    assert_eq!(verdict(&ledger, 0, false), Some(Verdict::Allow), "a check starting exactly at the deadline qualifies");
}

#[test]
fn verdict_records_normal_typing_once_a_stable_field_has_a_qualifying_check() {
    let mut ledger = Ledger::default();
    ledger.push_check(0, FieldClass::NonText); // the capture-start confirmation
    ledger.push_check(400, FieldClass::NonText); // a later check, no focus change in between
    assert_eq!(verdict(&ledger, 50, false), Some(Verdict::Allow));
}

#[test]
fn verdict_gaps_a_key_typed_right_before_the_ui_automation_event_arrives() {
    // Tab into a password field and type before the UI Automation event arrives: the blind
    // window reaches GUARD_MS before the arrival, so the key still gaps.
    let mut ledger = Ledger::default();
    ledger.push_check(0, FieldClass::NonText);
    let key_t = 250;
    let arrival = 260; // the Tab's own event arrives just after the key
    ledger.push_arrival(arrival);
    ledger.push_check(arrival, FieldClass::Password); // the field turned out to be a password field
    ledger.push_check(700, FieldClass::Password); // any later check while still in the field
    assert_eq!(verdict(&ledger, key_t, false), Some(Verdict::Gap));
}

#[test]
fn verdict_gaps_every_char_of_a_lagging_consumers_tab_in_pin_tab_out() {
    // Tab in, type a PIN, Tab out. Even when a lagging consumer finds "not password" checks on
    // both sides of a PIN character, the blind windows around the two arrivals gap it.
    let mut ledger = Ledger::default();
    ledger.push_check(0, FieldClass::NonText);
    let tab_in = 10;
    ledger.push_arrival(tab_in);
    ledger.push_check(tab_in, FieldClass::Password);
    let pin_chars = [50u64, 70, 90, 110];
    let tab_out = 200;
    ledger.push_arrival(tab_out);
    ledger.push_check(tab_out, FieldClass::NonText);
    ledger.push_check(900, FieldClass::NonText); // the lagging consumer's own late recheck
    for t in pin_chars {
        assert!(verdict(&ledger, t, false) == Some(Verdict::Gap), "PIN char at {t} must gap");
    }
}

#[test]
fn window_is_dirty_treats_an_unknown_check_as_dirty_too() {
    // An Unknown check between a key and its qualifying check dirties the window like a
    // Password check.
    let mut ledger = Ledger::default();
    ledger.push_check(0, FieldClass::Text); // stable before any of this
    ledger.push_check(100, FieldClass::Unknown); // classification failed partway through
    ledger.push_check(400, FieldClass::Text); // the qualifying check for the key below
    assert_eq!(
        verdict(&ledger, 50, false),
        Some(Verdict::Gap),
        "an Unknown check inside the window must gap the key, not be ignored by window_is_dirty"
    );
}

#[test]
fn verdict_at_stop_gaps_a_key_with_no_qualifying_check_instead_of_waiting_forever() {
    let mut ledger = Ledger::default();
    ledger.push_check(0, FieldClass::NonText); // far too early to qualify for a key at t=1000
    assert_eq!(verdict(&ledger, 1_000, false), None, "still pending under the normal algorithm");
    assert!(verdict_at_stop(&ledger, 1_000, false, KeyId { vk: 0x41, scan: 0, extended: false, ctrl: false, alt: false, win: false }, false), "stopping must resolve doubt as a gap, never as recorded");
}

#[test]
fn verdict_at_stop_still_honors_a_qualifying_check_that_already_completed() {
    let mut ledger = Ledger::default();
    ledger.push_check(0, FieldClass::NonText);
    ledger.push_check(400, FieldClass::NonText);
    assert!(!verdict_at_stop(&ledger, 50, false, KeyId { vk: 0x41, scan: 0, extended: false, ctrl: false, alt: false, win: false }, false), "a real, already-completed answer is not thrown away just because the capture is stopping");
}

// A modifier the hook tracked as held must be confirmed against the live keyboard before
// it keeps a key.

#[test]
fn a_tracked_modifier_with_no_live_confirmation_is_not_held() {
    // Alt tracked down but released on an elevated window or the secure desktop:
    // GetAsyncKeyState says up and wins.
    assert!(!confirm_modifier(true, false), "a stale tracked modifier with no live confirmation must resolve to not held");
}

#[test]
fn a_tracked_and_live_modifier_is_held() {
    assert!(confirm_modifier(true, true), "a modifier both tracked and live must still count as held");
}

#[test]
fn an_untracked_modifier_is_never_held_even_if_live_state_is_stale() {
    // Both sides must agree: an untracked modifier is never held, whatever the live read says.
    assert!(!confirm_modifier(false, true));
}

#[test]
fn an_injected_modifier_event_is_never_tracked() {
    assert!(!should_track_modifier_event(true), "a synthetic modifier press must never update the tracked state");
    assert!(should_track_modifier_event(false), "a real modifier press must still be tracked");
}

// keyOverlayShortcutsOnly: an absolute allow-list, tested directly against
// is_kept_by_shortcuts_only.

#[test]
fn shortcuts_only_drops_a_plain_letter() {
    assert!(!is_kept_by_shortcuts_only(0x41, 0, false, false, false, false), "a plain letter key must be dropped under shortcuts-only");
}

#[test]
fn shortcuts_only_keeps_ctrl_c() {
    assert!(is_kept_by_shortcuts_only(0x43, 0, false, true, false, false), "Ctrl+C is a shortcut, not typed text, so it must record");
}

#[test]
fn shortcuts_only_drops_altgr() {
    // Ctrl+Alt without Win is AltGr (Ctrl+Alt+Q types "@" on a German keyboard): typing, not a
    // shortcut.
    assert!(!is_kept_by_shortcuts_only(0x51, 0, false, true, true, false), "AltGr must still be dropped, not treated as a shortcut");
}

// Alt+Numpad and hex numpad character entry is dropped, never kept as an Alt+key shortcut.

#[test]
fn shortcuts_only_drops_alt_numpad_digit_with_numlock_on() {
    // NumLock on: Numpad 6 arrives as VK_NUMPAD6 (0x66).
    assert!(!is_kept_by_shortcuts_only(0x66, 0x4D, false, false, true, false), "an Alt+Numpad digit composes a character, it is not a shortcut");
}

#[test]
fn shortcuts_only_drops_alt_numpad_plus_with_numlock_on() {
    assert!(!is_kept_by_shortcuts_only(0x6B, 0x4E, false, false, true, false), "Numpad + starts EnableHexNumpad entry and must be dropped too");
}

#[test]
fn shortcuts_only_drops_alt_numpad_digit_with_numlock_off_by_its_scan_code() {
    // NumLock off: the same key arrives as VK_LEFT (0x25) with scan code 0x4B, not extended.
    // The scan code identifies it.
    assert!(!is_kept_by_shortcuts_only(VK_LEFT, 0x4B, false, false, true, false), "a numpad-origin nav VK with NumLock off must still be dropped under Alt");
}

#[test]
fn shortcuts_only_drops_alt_numpad_digit_key_up_too() {
    // Down and up carry the same vk/scan/flags here, so one call proves both sides agree.
    assert!(!is_kept_by_shortcuts_only(0x66, 0x4D, false, false, true, false), "the down is dropped");
    assert!(!is_kept_by_shortcuts_only(0x66, 0x4D, false, false, true, false), "and so is the matching up, same inputs either way");
}

#[test]
fn shortcuts_only_keeps_the_real_nav_cluster_key_regardless_of_the_numpad_check() {
    // The real arrow cluster sends the same VKs but with LLKHF_EXTENDED set, so Alt+Left stays
    // a kept nav key.
    assert!(is_kept_by_shortcuts_only(VK_LEFT, 0x4B, true, false, true, false), "the real, extended arrow key must still be kept under Alt");
}

#[test]
fn shortcuts_only_keeps_alt_with_an_ordinary_letter() {
    assert!(is_kept_by_shortcuts_only(0x41, 0x1E, false, false, true, false), "Alt+A is an ordinary shortcut, not a numpad scan or VK");
}

#[test]
fn shortcuts_only_does_not_treat_ctrl_alt_numpad_as_numpad_entry() {
    // The numpad check applies to Alt held alone; Ctrl+Alt is AltGr territory and other
    // combinations are ordinary shortcuts.
    assert!(is_kept_by_shortcuts_only(0x66, 0x4D, false, true, true, true), "Ctrl+Alt+Win+Numpad6 is an ordinary shortcut combo, not character entry");
}

// Only a structurally non-text control type may resolve to NonText. A type on neither list
// (a bare Pane/Custom element such as PuTTY or an RDP/VNC session), a failed read or no
// element is Unknown, which gaps (see classify_control).

#[test]
fn every_allow_listed_control_type_is_non_text() {
    for ct in [
        UIA_ButtonControlTypeId,
        UIA_CheckBoxControlTypeId,
        UIA_RadioButtonControlTypeId,
        UIA_MenuItemControlTypeId,
        UIA_TabItemControlTypeId,
        UIA_ListItemControlTypeId,
        UIA_TreeItemControlTypeId,
        UIA_SliderControlTypeId,
        UIA_ScrollBarControlTypeId,
        UIA_HyperlinkControlTypeId,
        UIA_ImageControlTypeId,
        UIA_ToolBarControlTypeId,
        UIA_ThumbControlTypeId,
    ] {
        assert!(is_non_text_control_type(ct));
    }
}

#[test]
fn a_putty_like_pane_or_custom_control_gaps_instead_of_recording() {
    // PuTTY, RDP/VNC windows and Qt/Java/game windows without an accessibility provider
    // surface as a bare Pane or Custom element, which is not allow-listed.
    assert!(!is_non_text_control_type(UIA_PaneControlTypeId));
    assert!(!is_non_text_control_type(UIA_CustomControlTypeId));
    // Such an element never proved a positive Text identification either, so it resolves to
    // Unknown and gaps like a password field. An SSH or RDP password typed there is not
    // recorded.
    let mut ledger = Ledger::default();
    ledger.push_check(0, FieldClass::Unknown);
    ledger.push_check(400, FieldClass::Unknown);
    let v = verdict(&ledger, 50, false).expect("a qualifying check exists");
    assert_eq!(v, Verdict::Gap);
    assert!(
        resolve_verdict(v, false, KeyId { vk: 0x41, scan: 0x1E, extended: false, ctrl: false, alt: false, win: false }),
        "an unclassifiable Pane/Custom window's typed letters must be gapped, never recorded"
    );
}

// A check's reads must not be trusted once the focused identity has moved on since the
// check started.

#[test]
fn a_changed_hwnd_is_a_changed_identity_even_with_no_element_on_either_side() {
    let a = (HWND(std::ptr::dangling_mut::<std::ffi::c_void>()), None);
    let b = (HWND(2 as *mut std::ffi::c_void), None);
    assert!(focus_identity_changed(&None, &a, &b), "a different HWND alone is enough, no UIA object needed to see it");
}

#[test]
fn the_same_hwnd_with_no_element_on_either_side_is_not_a_changed_identity() {
    let a = (HWND(std::ptr::dangling_mut::<std::ffi::c_void>()), None);
    let b = (HWND(std::ptr::dangling_mut::<std::ffi::c_void>()), None);
    assert!(!focus_identity_changed(&None, &a, &b));
}

#[test]
fn is_password_field_fails_closed_with_no_element_to_query() {
    // A missing element (UIA unavailable or the fetch failed) resolves to the fail-closed
    // Password answer.
    assert!(is_password_field(HWND(std::ptr::dangling_mut::<std::ffi::c_void>()), None));
}

#[test]
fn classify_control_is_unknown_with_no_element_to_query() {
    // No element to query is Unknown (which gaps), not Text.
    assert_eq!(classify_control(None), FieldClass::Unknown, "no element to classify is doubt, never a positive Text finding");
}

#[test]
fn classify_identity_is_password_when_the_shared_element_is_missing() {
    let identity = (HWND(std::ptr::dangling_mut::<std::ffi::c_void>()), None);
    assert_eq!(classify_identity(&identity), FieldClass::Password);
}

#[test]
fn classify_checked_fails_closed_and_reports_no_change_with_no_uia_object() {
    // With no UI Automation object, focus_identity touches no live element, so classify_checked
    // takes the ordinary fail-closed path: Password, not reported as invalidated.
    assert_eq!(classify_checked(&None), (FieldClass::Password, false));
}

#[test]
fn a_late_detected_focus_change_after_a_stall_gaps_the_key_typed_during_it() {
    // A stale check reporting the old field as non-text, arriving well after the key, would
    // satisfy verdict's Allow branch on its own.
    let mut ledger = Ledger::default();
    ledger.push_check(1600, FieldClass::NonText);
    assert_eq!(verdict(&ledger, 1200, false), Some(Verdict::Allow));

    let mut pending: VecDeque<RawKey> = VecDeque::new();
    pending.push_back(RawKey {
        vk: 0x41,
        scan: 0,
        extended: false,
        down: true,
        t_ms: 1200,
        injected: false,
        ctrl: false,
        alt: false,
        win: false,
        forced_gap: false,
    });

    // The poll last confirmed focus at 1000, stalled in a slow UI Automation call and detected
    // the change at 2000. The key above landed inside that span, which mark_forced_gaps and
    // push_arrival cover.
    mark_forced_gaps(&mut pending, 1000, 2000);
    ledger.push_arrival(1000);
    ledger.push_arrival(2000);

    let mut rec = Recorder::default();
    let clock = Mutex::new(ActiveClock::new(0));
    let cutoff: Mutex<Option<u64>> = Mutex::new(None);
    let ledger = Mutex::new(ledger);
    resolve_pending(&mut pending, &mut rec, &clock, &cutoff, &ledger, false);

    assert!(pending.is_empty());
    assert!(rec.sidecar.events.is_empty());
    assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 1200, end_ms: 1200 }]);
}

#[test]
fn a_poll_that_overran_its_deadline_counts_as_a_stall_even_with_no_detected_change() {
    assert!(!poll_overran(1049, 1000)); // 49ms late, still within FOCUS_POLL_MS
    assert!(!poll_overran(1050, 1000)); // exactly FOCUS_POLL_MS late, not yet over
    assert!(poll_overran(1051, 1000)); // over by more than one full poll interval
}

#[test]
fn key_in_widened_span_covers_guard_ms_on_both_sides_of_the_span() {
    assert!(!key_in_widened_span(699, 1000, 2000)); // just outside, even widened
    assert!(key_in_widened_span(700, 1000, 2000)); // span start - GUARD_MS
    assert!(key_in_widened_span(1500, 1000, 2000)); // inside the span itself
    assert!(key_in_widened_span(2300, 1000, 2000)); // span end + GUARD_MS
    assert!(!key_in_widened_span(2301, 1000, 2000)); // just outside, even widened
}

#[test]
fn mark_forced_gaps_only_touches_keys_inside_the_widened_span() {
    let mut pending: VecDeque<RawKey> = VecDeque::new();
    let make = |t_ms: u64| RawKey {
        vk: 0x41,
        scan: 0,
        extended: false,
        down: true,
        t_ms,
        injected: false,
        ctrl: false,
        alt: false,
        win: false,
        forced_gap: false,
    };
    pending.push_back(make(500)); // before the widened span
    pending.push_back(make(1500)); // inside
    pending.push_back(make(2500)); // after the widened span
    mark_forced_gaps(&mut pending, 1000, 2000);
    let flags: Vec<bool> = pending.iter().map(|k| k.forced_gap).collect();
    assert_eq!(flags, vec![false, true, false]);
}

fn make_raw_key(t_ms: u64) -> RawKey {
    RawKey {
        vk: 0x41,
        scan: 0,
        extended: false,
        down: true,
        t_ms,
        injected: false,
        ctrl: false,
        alt: false,
        win: false,
        forced_gap: false,
    }
}

#[test]
fn idle_to_busy_reset_anchors_the_deadline_to_the_keys_own_hook_timestamp() {
    let (last_focus_at, next_poll_due) = idle_to_busy_reset(1200);
    assert_eq!(last_focus_at, 1200);
    assert_eq!(next_poll_due, 1250);
}

#[test]
fn first_key_after_an_idle_gap_consumed_promptly_is_kept() {
    // An idle gap alone, with the key consumed right after capture, must not force-gap it:
    // next_poll_due lands at raw.t_ms + 50.
    let (_, next_poll_due) = idle_to_busy_reset(1200);
    let now = 1200;
    assert!(!poll_overran(now, next_poll_due));

    let mut ledger = Ledger::default();
    ledger.push_check(1600, FieldClass::NonText);
    assert_eq!(verdict(&ledger, 1200, false), Some(Verdict::Allow));

    let mut pending: VecDeque<RawKey> = VecDeque::new();
    pending.push_back(make_raw_key(1200));

    let mut rec = Recorder::default();
    let clock = Mutex::new(ActiveClock::new(0));
    let cutoff: Mutex<Option<u64>> = Mutex::new(None);
    let ledger = Mutex::new(ledger);
    resolve_pending(&mut pending, &mut rec, &clock, &cutoff, &ledger, false);

    assert!(pending.is_empty());
    assert!(rec.sidecar.gaps.is_empty(), "an idle gap with a prompt receive must not turn into a recorded gap");
}

#[test]
fn first_key_after_an_idle_gap_with_a_focus_change_on_the_next_poll_is_gapped() {
    // The idle-to-busy reset only rearms the deadline and leaves last_focus alone, so a later
    // poll that detects a changed identity still gaps the key through mark_forced_gaps.
    let (last_focus_at, _) = idle_to_busy_reset(1200);

    let mut ledger = Ledger::default();
    ledger.push_check(1600, FieldClass::NonText);

    let mut pending: VecDeque<RawKey> = VecDeque::new();
    pending.push_back(make_raw_key(1200));
    mark_forced_gaps(&mut pending, last_focus_at, 1300);
    ledger.push_arrival(last_focus_at);
    ledger.push_arrival(1300);

    let mut rec = Recorder::default();
    let clock = Mutex::new(ActiveClock::new(0));
    let cutoff: Mutex<Option<u64>> = Mutex::new(None);
    let ledger = Mutex::new(ledger);
    resolve_pending(&mut pending, &mut rec, &clock, &cutoff, &ledger, false);

    assert!(pending.is_empty());
    assert!(rec.sidecar.events.is_empty());
    assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 1200, end_ms: 1200 }]);
}

#[test]
fn a_genuine_stall_with_a_key_already_pending_still_forces_the_gap() {
    // A poll armed because a key was already pending still treats its own overrun as a stall;
    // idle_to_busy_reset must not weaken that.
    assert!(poll_overran(2000, 1000));
    let mut pending: VecDeque<RawKey> = VecDeque::new();
    pending.push_back(make_raw_key(1200));
    mark_forced_gaps(&mut pending, 1000, 2000);
    assert!(pending[0].forced_gap);
}

#[test]
fn a_delayed_receive_with_no_detected_change_still_forces_the_gap() {
    // Focus goes A, password/text, A inside the gap between this key's hook timestamp (1200)
    // and its receipt (1400), with no UIA event. An endpoint comparison would read
    // "unchanged"; anchoring the deadline to raw.t_ms makes poll_overran catch it (1250 is
    // more than one poll interval behind).
    let (last_focus_at, next_poll_due) = idle_to_busy_reset(1200);
    let now = 1400;
    assert!(poll_overran(now, next_poll_due), "a consumption delay this long must read as an overrun on its own");

    let mut pending: VecDeque<RawKey> = VecDeque::new();
    pending.push_back(make_raw_key(1200));
    mark_forced_gaps(&mut pending, last_focus_at, now);

    let mut ledger = Ledger::default();
    ledger.push_check(1600, FieldClass::NonText);
    ledger.push_arrival(last_focus_at);
    ledger.push_arrival(now);

    let mut rec = Recorder::default();
    let clock = Mutex::new(ActiveClock::new(0));
    let cutoff: Mutex<Option<u64>> = Mutex::new(None);
    let ledger = Mutex::new(ledger);
    resolve_pending(&mut pending, &mut rec, &clock, &cutoff, &ledger, false);

    assert!(pending.is_empty());
    assert!(rec.sidecar.events.is_empty());
    assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 1200, end_ms: 1200 }]);
}

#[test]
fn a_key_captured_while_busy_before_recv_with_a_stale_observation_is_gapped() {
    // The thread was busy (not in recv) while pending stayed empty, so neither the poll nor a
    // settle check ran, and focus could flip and return unseen. The key is captured at 1250,
    // recv is entered at 1300. Since raw_t_ms precedes recv_entered_at and the silence since
    // last_observed_at (1000) exceeds the stall allowance, classify_idle_transition
    // force-gaps back to 1000.
    let outcome = classify_idle_transition(1250, 1300, 1000, 1310);
    let (span_from, span_to) = match outcome {
        IdleTransition::ForceGap { span_from, span_to } => (span_from, span_to),
        IdleTransition::Rearm { .. } => panic!("expected ForceGap for a stale, busy-before-recv key"),
    };
    assert_eq!((span_from, span_to), (1000, 1310));

    let mut ledger = Ledger::default();
    ledger.push_check(1600, FieldClass::NonText);
    ledger.push_arrival(span_from);
    ledger.push_arrival(span_to);

    let mut pending: VecDeque<RawKey> = VecDeque::new();
    let mut raw = make_raw_key(1250);
    raw.forced_gap = true;
    pending.push_back(raw);

    let mut rec = Recorder::default();
    let clock = Mutex::new(ActiveClock::new(0));
    let cutoff: Mutex<Option<u64>> = Mutex::new(None);
    let ledger = Mutex::new(ledger);
    resolve_pending(&mut pending, &mut rec, &clock, &cutoff, &ledger, false);

    assert!(pending.is_empty());
    assert!(rec.sidecar.events.is_empty());
    assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 1250, end_ms: 1250 }]);
}

#[test]
fn idle_then_prompt_key_still_rearms_and_is_kept() {
    // The genuine idle case: the thread was blocked in recv when the key arrived (raw_t_ms >=
    // recv_entered_at), so the ordinary Rearm branch applies.
    let recv_entered_at = 1150;
    let last_observed_at = 1150; // set when this thread entered that same recv call
    let raw_t_ms = 1200; // arrived while genuinely waiting
    let now = 1200;
    let outcome = classify_idle_transition(raw_t_ms, recv_entered_at, last_observed_at, now);
    let (last_focus_at, next_poll_due) = match outcome {
        IdleTransition::Rearm { last_focus_at, next_poll_due } => (last_focus_at, next_poll_due),
        IdleTransition::ForceGap { .. } => panic!("expected Rearm for a genuinely idle receive"),
    };
    assert_eq!((last_focus_at, next_poll_due), (1200, 1250));

    let mut ledger = Ledger::default();
    ledger.push_check(1600, FieldClass::NonText);
    assert_eq!(verdict(&ledger, 1200, false), Some(Verdict::Allow));

    let mut pending: VecDeque<RawKey> = VecDeque::new();
    pending.push_back(make_raw_key(1200));

    let mut rec = Recorder::default();
    let clock = Mutex::new(ActiveClock::new(0));
    let cutoff: Mutex<Option<u64>> = Mutex::new(None);
    let ledger = Mutex::new(ledger);
    resolve_pending(&mut pending, &mut rec, &clock, &cutoff, &ledger, false);

    assert!(pending.is_empty());
    assert!(rec.sidecar.gaps.is_empty(), "a genuinely idle receive must not turn into a recorded gap");
}

#[test]
fn a_brief_busy_stretch_right_after_an_observation_still_rearms() {
    // Busy before recv alone does not force-gap: a short stretch after a real observation
    // barely hides anything. Only silence past the stall allowance escalates to ForceGap.
    let outcome = classify_idle_transition(1210, 1230, 1200, 1235);
    match outcome {
        IdleTransition::Rearm { last_focus_at, next_poll_due } => {
            assert_eq!((last_focus_at, next_poll_due), (1210, 1260));
        }
        IdleTransition::ForceGap { .. } => panic!("expected Rearm for a fresh observation"),
    }
}

#[test]
fn an_unresolved_classification_is_unknown_and_gaps() {
    // A failed or unknown classification resolves to Unknown and gaps unconditionally, proven
    // through the production verdict() pipeline.
    let mut ledger = Ledger::default();
    ledger.push_check(0, FieldClass::Unknown);
    ledger.push_check(400, FieldClass::Unknown);
    let v = verdict(&ledger, 50, false).expect("a qualifying check exists");
    assert_eq!(v, Verdict::Gap);
    assert!(resolve_verdict(v, false, KeyId { vk: 0x41, scan: 0, extended: false, ctrl: false, alt: false, win: false }), "an unresolved classification must gap, never record as if it were confirmed text");
}

#[test]
fn a_nontext_field_keeps_plain_letter_keys() {
    let mut ledger = Ledger::default();
    ledger.push_check(0, FieldClass::NonText);
    ledger.push_check(400, FieldClass::NonText);
    let v = verdict(&ledger, 50, false).expect("a qualifying check exists");
    assert_eq!(v, Verdict::Allow);
    assert!(!resolve_verdict(v, false, KeyId { vk: 0x41, scan: 0, extended: false, ctrl: false, alt: false, win: false }), "a NonText field's plain typing must keep recording as before");
}

#[test]
fn a_password_field_drops_ctrl_c_too() {
    let mut ledger = Ledger::default();
    ledger.push_check(0, FieldClass::Password);
    ledger.push_check(400, FieldClass::Password);
    let v = verdict(&ledger, 50, false).expect("a qualifying check exists");
    assert_eq!(v, Verdict::Gap);
    assert!(resolve_verdict(v, false, KeyId { vk: 0x43, scan: 0, extended: false, ctrl: true, alt: false, win: false }), "shortcuts are blocked in password fields too, not just typed characters");
}

// keyOverlayShortcutsOnly applied through resolve_verdict: an absolute filter on top of
// the verdict, whatever field class produced the Allow, and never relaxing a password gap.

#[test]
fn shortcuts_only_drops_a_letter_under_every_allow_producing_field_class() {
    for class in [FieldClass::NonText, FieldClass::Text] {
        let mut ledger = Ledger::default();
        ledger.push_check(0, class);
        ledger.push_check(400, class);
        let v = verdict(&ledger, 50, false).expect("a qualifying check exists");
        assert_eq!(v, Verdict::Allow);
        assert!(
            resolve_verdict(v, true, KeyId { vk: 0x41, scan: 0x1E, extended: false, ctrl: false, alt: false, win: false }),
            "a plain letter must be dropped under shortcuts-only regardless of field class"
        );
    }
}

#[test]
fn shortcuts_only_keeps_ctrl_c_enter_and_f5_through_resolve_verdict() {
    const VK_F5: u32 = 0x74;
    assert!(!resolve_verdict(Verdict::Allow, true, KeyId { vk: 0x43, scan: 0, extended: false, ctrl: true, alt: false, win: false }), "Ctrl+C stays");
    assert!(!resolve_verdict(Verdict::Allow, true, KeyId { vk: VK_RETURN, scan: 0, extended: false, ctrl: false, alt: false, win: false }), "Enter stays");
    assert!(!resolve_verdict(Verdict::Allow, true, KeyId { vk: VK_F5, scan: 0, extended: false, ctrl: false, alt: false, win: false }), "F5 stays");
}

#[test]
fn shortcuts_only_never_relaxes_a_password_gap() {
    assert!(
        resolve_verdict(Verdict::Gap, true, KeyId { vk: 0x43, scan: 0, extended: false, ctrl: true, alt: false, win: false }),
        "a password verdict stays gapped no matter what the shortcuts-only filter would otherwise allow"
    );
}

#[test]
fn shortcuts_only_off_never_changes_the_base_verdict() {
    assert!(!resolve_verdict(Verdict::Allow, false, KeyId { vk: 0x41, scan: 0, extended: false, ctrl: false, alt: false, win: false }), "the setting off must not change existing behavior at all");
}

#[test]
fn shortcuts_only_flag_round_trips_through_json_like_settings_storage() {
    let mut flags = std::collections::HashMap::new();
    flags.insert("keyOverlayShortcutsOnly".to_string(), true);
    let json = serde_json::to_string(&flags).unwrap();
    let restored: std::collections::HashMap<String, bool> = serde_json::from_str(&json).unwrap();
    assert!(shortcuts_only(&restored));
    assert!(!shortcuts_only(&std::collections::HashMap::new()), "absent, default off");
}

#[test]
fn a_tab_key_down_counts_as_a_synthetic_arrival_that_gaps_a_key_with_no_uia_event_at_all() {
    // Tab into a password field, type, Tab out within 300ms, with no UI Automation event for
    // either move. The Tab-out key-down counts as an arrival at the hook's timestamp, so its
    // blind window still gaps the key.
    assert!(is_focus_moving_key(VK_TAB));
    assert!(is_focus_moving_key(VK_RETURN));
    assert!(is_focus_moving_key(VK_ESCAPE));
    assert!(!is_focus_moving_key(0x41)); // an ordinary letter key never counts
    let mut ledger = Ledger::default();
    ledger.push_check(0, FieldClass::NonText); // the field looked clean before any of this
    let key_t = 100;
    let tab_out_t = 150; // well within 300ms of the key
    ledger.push_arrival(tab_out_t); // what the hook-time synthetic arrival does
    ledger.push_check(700, FieldClass::NonText); // the consumer's own later recheck, also clean
    assert_eq!(verdict(&ledger, key_t, false), Some(Verdict::Gap), "the synthetic arrival's blind window must still gap the key");
}

#[test]
fn handler_registration_failure_gaps_every_key_regardless_of_checks() {
    let mut ledger = Ledger::default();
    ledger.push_check(0, FieldClass::NonText);
    ledger.push_check(400, FieldClass::NonText);
    assert_eq!(verdict(&ledger, 50, false), Some(Verdict::Allow), "before the failure, a clean qualifying check still records");
    ledger.gap_everything();
    assert_eq!(verdict(&ledger, 50, false), Some(Verdict::Gap), "once the handler failed to register, nothing gets recorded again");
    assert_eq!(verdict(&ledger, 1_000_000, false), Some(Verdict::Gap), "including any key far later, however clean its own checks look");
}

#[test]
fn pruning_drops_only_entries_no_pending_or_future_key_could_ever_need_again() {
    let mut ledger = Ledger::default();
    ledger.push_check(0, FieldClass::NonText); // ancient, long since resolved
    ledger.push_arrival(50); // ancient focus change, long since resolved
    ledger.push_check(50, FieldClass::Password);
    // One key already resolved by a qualifying check, one still waiting.
    let resolved_key = 5_000;
    ledger.push_check(resolved_key + SETTLE_MS, FieldClass::NonText);
    let pending_key = 5_200;
    let before_resolved = verdict(&ledger, resolved_key, false);
    let before_pending = verdict(&ledger, pending_key, false);
    let checks_before = ledger.checks.len();
    let arrivals_before = ledger.arrivals.len();

    // Prune to the oldest pending key minus GUARD_MS, as the consumer loop does.
    ledger.prune(pending_key.saturating_sub(GUARD_MS));

    assert!(ledger.checks.len() < checks_before, "the ancient entries must actually be gone");
    assert!(ledger.arrivals.len() < arrivals_before, "the ancient arrival must actually be gone");
    assert_eq!(verdict(&ledger, resolved_key, false), before_resolved, "pruning must never change an already-settled verdict");
    assert_eq!(verdict(&ledger, pending_key, false), before_pending, "pruning must never change a still-pending verdict either");
}

#[test]
fn join_bounded_returns_the_result_once_the_thread_actually_finishes() {
    let handle = thread::spawn(|| {
        thread::sleep(Duration::from_millis(20));
        42
    });
    assert_eq!(join_bounded(handle, Duration::from_millis(2_000)), Some(42));
}

#[test]
fn join_bounded_gives_up_and_detaches_a_thread_that_outlives_the_bound() {
    // A thread running well past the bound stands in for a consumer stuck in a hung UIA call;
    // join_bounded must return None quickly.
    let handle = thread::spawn(|| {
        thread::sleep(Duration::from_millis(500));
        "done"
    });
    let started = Instant::now();
    assert_eq!(join_bounded(handle, Duration::from_millis(50)), None);
    assert!(started.elapsed() < Duration::from_millis(500), "must give up at the bound, not wait for the thread");
}

#[test]
fn settle_and_poll_deadlines_are_never_starved_by_a_burst_of_ok_arrivals() {
    // Key-repeat bursts keep hitting the Ok(raw) arm, so a settle check or focus poll must not
    // depend on the Timeout arm: both are pure functions of absolute time.
    let front_t_ms = 0u64;
    let mut next_poll_due = FOCUS_POLL_MS;
    // Three arrivals 10ms apart, well under FOCUS_POLL_MS.
    for now in [10u64, 20, 30] {
        assert!(!settle_check_due(Some(front_t_ms), now), "not due yet at t={now}");
        assert!(!focus_poll_due(false, now, next_poll_due), "not due yet at t={now}");
    }
    // The settle deadline is reached even though every tick arrived through Ok.
    let settle_due_at = front_t_ms + SETTLE_MS;
    assert!(settle_check_due(Some(front_t_ms), settle_due_at), "must become due exactly at its own deadline");
    // Likewise the poll deadline, independently.
    assert!(focus_poll_due(false, next_poll_due, next_poll_due));
    next_poll_due += FOCUS_POLL_MS;
    assert!(!focus_poll_due(false, next_poll_due - 1, next_poll_due), "not due again until the next tick");
}

#[test]
fn recv_wait_targets_whichever_deadline_is_sooner_and_never_goes_negative() {
    // The settle deadline is sooner (front=100, settle at 400, poll at 1000).
    assert_eq!(recv_wait_ms(0, 100, 1_000), 400);
    // The poll deadline is sooner.
    assert_eq!(recv_wait_ms(0, 10_000, 50), 50);
    // Both deadlines already passed: an immediate wake, no underflow.
    assert_eq!(recv_wait_ms(10_000, 0, 0), 0);
}

#[test]
fn exceeding_the_ledger_hard_cap_fails_closed_and_drops_history() {
    let mut ledger = Ledger::default();
    // A clean check within the settle window, so any later gap comes from the cap, not a dirty
    // window.
    ledger.push_check(0, FieldClass::NonText);
    assert_eq!(verdict(&ledger, 0, false), None, "not yet, no qualifying check exists"); // sanity: still pure, unaffected by the flood below
    for i in 0..LEDGER_HARD_CAP {
        // Arrivals far enough apart that none lands in another's blind window.
        ledger.push_arrival((i as u64) * 10_000);
    }
    assert!(ledger.fail_closed, "the cap must have tripped fail_closed");
    assert!(ledger.checks.is_empty() && ledger.arrivals.is_empty(), "history must actually be dropped, not just capped");
    assert_eq!(verdict(&ledger, 0, false), Some(Verdict::Gap), "every verdict from here on must gap, regardless of any earlier clean check");
    assert_eq!(verdict(&ledger, 50_000_000, false), Some(Verdict::Gap), "including a key far outside every arrival's own window");
}

// ActiveClock: pause drops events, and offsets use the active-recording clock, not wall
// time.

#[test]
fn active_clock_with_no_pause_is_plain_elapsed_wall_time() {
    let clock = ActiveClock::new(1_000);
    assert!(!clock.is_paused_at(1_500));
    assert_eq!(clock.active_ms(1_500), 500);
}

#[test]
fn active_clock_rebased_to_the_first_frame_counts_keys_from_the_video_start() {
    // Capture came up at 1_000, the first frame (video time zero) arrived 300 ms later.
    let mut clock = ActiveClock::new(1_000);
    assert_eq!(clock.active_ms(1_350), 350, "before the rebase a key reads late by the first-frame latency");
    clock.rebase(1_300);
    assert_eq!(clock.active_ms(1_350), 50);
    assert_eq!(clock.active_ms(1_200), 0, "a key before the first frame clamps to the start");
}

#[test]
fn active_clock_marks_a_key_struck_during_a_closed_pause_as_paused() {
    let mut clock = ActiveClock::new(0);
    clock.pause(100);
    clock.resume(300);
    assert!(!clock.is_paused_at(50));
    assert!(clock.is_paused_at(150));
    assert!(clock.is_paused_at(299));
    assert!(!clock.is_paused_at(300));
    assert!(!clock.is_paused_at(400));
}

#[test]
fn active_clock_marks_a_key_struck_during_a_still_open_pause_as_paused() {
    let mut clock = ActiveClock::new(0);
    clock.pause(100);
    // Never resumed: everything after the pause started reads as paused.
    assert!(clock.is_paused_at(100));
    assert!(clock.is_paused_at(10_000));
}

#[test]
fn active_clock_subtracts_paused_spans_from_the_elapsed_offset() {
    let mut clock = ActiveClock::new(0);
    clock.pause(1_000);
    clock.resume(3_000); // 2000ms paused
    // 4000ms after start with 2000ms paused is 2000ms of active time, the same basis as the
    // video track's clock.
    assert_eq!(clock.active_ms(4_000), 2_000);
}

#[test]
fn active_clock_ignores_a_pause_that_starts_after_the_queried_instant() {
    let mut clock = ActiveClock::new(0);
    clock.pause(5_000);
    clock.resume(6_000);
    assert_eq!(clock.active_ms(1_000), 1_000, "a later pause must not affect an earlier timestamp's offset");
}

#[test]
fn active_clock_a_pause_called_twice_in_a_row_does_not_open_a_second_span() {
    let mut clock = ActiveClock::new(0);
    clock.pause(100);
    clock.pause(150); // e.g. a stale double-click of the pause button
    clock.resume(300);
    assert_eq!(clock.pauses.len(), 1);
    assert_eq!(clock.active_ms(400), 200);
}

#[test]
fn a_stopping_capture_leaves_the_sender_of_a_newer_capture_alone() {
    let (old_tx, _old_rx) = sync_channel::<RawKey>(1);
    let (new_tx, _new_rx) = sync_channel::<RawKey>(1);
    // The old capture (id 9001) was replaced by a newer one (id 9002) before its stop ran.
    *raw_tx_slot().lock().unwrap() = Some((9001, old_tx));
    *raw_tx_slot().lock().unwrap() = Some((9002, new_tx));
    clear_raw_tx_if_owner(9001);
    assert!(raw_tx_slot().lock().unwrap().as_ref().is_some_and(|(id, _)| *id == 9002), "the newer capture keeps its sender");
    clear_raw_tx_if_owner(9002);
    assert!(raw_tx_slot().lock().unwrap().is_none(), "the owner clears its own sender");
}
