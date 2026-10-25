use super::*;

// The checks below compare multi-line source text; a checkout with Windows line endings
// must read the same as one with Unix endings.
macro_rules! source {
    ($path:literal) => {
        include_str!($path).replace("\r\n", "\n")
    };
}

#[test]
fn prune_history_keeps_entries_stored_without_a_timestamp() {
    let now = now_secs();
    let mut settings = Settings {
        copy_log: vec![
            CopyEntry { id: "undated".into(), ts: 0 },
            CopyEntry { id: "fresh".into(), ts: now },
            CopyEntry { id: "old".into(), ts: now - 3 * 86_400 },
        ],
        ..Settings::default()
    };
    settings.ui_values.insert("historyDays".into(), 1.0);
    prune_history(&mut settings);
    let ids: Vec<&str> = settings.copy_log.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, vec!["undated", "fresh"]);
}

#[cfg(windows)]
#[test]
fn only_a_confidential_copy_opts_out_of_clipboard_history() {
    assert!(clipwatch::private_formats(false).is_empty());
    let secret = clipwatch::private_formats(true);
    for name in ["CanIncludeInClipboardHistory", "CanUploadToCloudClipboard", "ExcludeClipboardContentFromMonitorProcessing"] {
        assert!(secret.contains(&name), "{name} missing");
    }
}

#[test]
fn the_clipboard_inbox_takes_up_to_the_character_limit_and_no_more() {
    use clipwatch::{should_store, CLIP_INBOX_MAX_CHARS};
    assert_eq!(CLIP_INBOX_MAX_CHARS, 200_000);
    // Spaces between letters keep the password heuristic out of this test.
    let exact = format!("{}aa", "a ".repeat(CLIP_INBOX_MAX_CHARS / 2 - 1));
    assert_eq!(exact.trim().chars().count(), CLIP_INBOX_MAX_CHARS);
    assert!(should_store(&exact, 3, false), "200 000 characters are kept");
    let over = format!("{exact}b");
    assert_eq!(over.trim().chars().count(), CLIP_INBOX_MAX_CHARS + 1);
    assert!(!should_store(&over, 3, false), "200 001 characters are skipped");
    // Multi-byte characters count once: 200 000 of them are 600 000 bytes, still in.
    assert!(should_store(&"€".repeat(CLIP_INBOX_MAX_CHARS), 3, false));
    assert!(!should_store(&"€".repeat(CLIP_INBOX_MAX_CHARS + 1), 3, false));
    // The byte shortcut: far past four bytes a character is out without counting.
    assert!(!should_store(&"a".repeat(CLIP_INBOX_MAX_CHARS * 4 + 1), 3, false));
    assert!(!should_store("ab", 3, false), "the minimum length still applies");
    assert!(should_store("abc", 3, false));
}

#[test]
fn a_clipboard_text_block_past_the_character_limit_in_bytes_is_never_read() {
    use clipwatch::{text_block_within_limit, CLIP_INBOX_MAX_CHARS, CLIP_INBOX_MAX_UNICODE_BYTES};
    assert_eq!(CLIP_INBOX_MAX_UNICODE_BYTES, CLIP_INBOX_MAX_CHARS * 4 + 2);
    assert!(text_block_within_limit(0));
    assert!(text_block_within_limit((CLIP_INBOX_MAX_CHARS + 1) * 2), "a limit-size text in UTF-16 plus its NUL fits");
    assert!(text_block_within_limit(CLIP_INBOX_MAX_UNICODE_BYTES));
    assert!(!text_block_within_limit(CLIP_INBOX_MAX_UNICODE_BYTES + 1));
    assert!(!text_block_within_limit(300 * 1024 * 1024), "a 300 MB block is refused unread");
}

#[cfg(windows)]
#[test]
fn a_locked_vault_delivers_no_confidential_text() {
    assert_eq!(clipwatch::deliverable_text(true, false, "secret"), "");
    assert_eq!(clipwatch::deliverable_text(true, true, "secret"), "secret");
    assert_eq!(clipwatch::deliverable_text(false, false, "plain"), "plain");
}

// Structural fix, no red unit test: a refused clipboard promise or listener cannot be provoked
// without owning the system clipboard.
#[test]
fn a_refused_clipboard_promise_ends_the_session() {
    let src = source!("../clipwatch.rs");
    assert!(src.contains("promised = SetClipboardData(CF_UNICODETEXT, 0) != 0;"));
    assert!(src.contains("end_session_with_plain_write(is_first, true);"));
    assert!(src.contains("if AddClipboardFormatListener(hwnd) == 0 {"));
}

#[cfg(windows)]
#[test]
fn a_busy_clipboard_is_retried_a_bounded_number_of_times() {
    assert_eq!(clipwatch::open_retry_delay(0), Some(15));
    assert_eq!(clipwatch::open_retry_delay(9), Some(15));
    assert_eq!(clipwatch::open_retry_delay(10), None);
}

// The retry loop and the render timing need a clipboard held by another app, so the wiring is
// pinned in the source.
#[test]
fn a_rearm_retries_and_the_render_answer_leaves_the_judging_to_a_posted_message() {
    let src = source!("../clipwatch.rs");
    let open = &src[src.find("fn open_and_promise").unwrap()..];
    let open = &open[..open.find("\n}\n").unwrap()];
    assert!(open.contains("open_retry_delay(attempt)"), "a failed re-arm open must retry first");
    assert!(open.find("open_retry_delay").unwrap() < open.find("end_session_with_plain_write").unwrap());
    let render = &src[src.find("fn handle_render_format").unwrap()..];
    let render = &render[..render.find("\n}\n").unwrap()];
    assert!(render.contains("WM_APP_JUDGE"), "the judgement runs from a posted message");
    assert!(!render.contains("lock(&state)"), "the render answer takes no Db lock");
    assert!(!render.contains("emit_rotation_changed"));
}

#[test]
fn a_force_end_with_a_busy_clipboard_keeps_the_text_for_the_next_render() {
    let src = source!("../clipwatch.rs");
    let end = &src[src.find("fn handle_app_end").unwrap()..];
    let end = &end[..end.find("\n}\n").unwrap()];
    // The session is not taken before the open: it stays, marked as ending, until delivered.
    assert!(!end.contains(".take()"));
    assert!(end.contains("session.ending = true;"));
    assert!(end.contains("open_retry_delay(attempt)"));
    let render = &src[src.find("fn handle_render_format").unwrap()..];
    let render = &render[..render.find("\n}\n").unwrap()];
    assert!(render.contains("session.ending"), "a render delivers the text of an ending session");
}

#[test]
fn a_failed_message_window_is_logged() {
    let src = source!("../clipwatch.rs");
    let at = src.find("if hwnd == 0 {\n            append_crash_log(").expect("log before the early return");
    assert!(src[at..at + 260].contains("could not create the message window"));
}
