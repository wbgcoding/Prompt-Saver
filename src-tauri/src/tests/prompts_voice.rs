use super::*;

// ---- The prompt list the window gets, and what a save may do to an image ----

pub(super) const TEST_PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==";

pub(super) fn prompt_with_image() -> Prompt {
    serde_json::from_str(&format!(
        r#"{{"id":"p1","name":"Screenshot","text":"body","image":"{TEST_PNG}","show_image":true}}"#
    ))
    .expect("prompt fixture")
}

// The whole point of the slim list: the picture stays in the process.
#[test]
fn slim_prompt_carries_no_image_bytes() {
    let json = serde_json::to_string(&SlimPrompt::from(&prompt_with_image())).unwrap();
    assert!(!json.contains("iVBORw0KGgo"), "image bytes leaked into the state payload: {json}");
    assert!(!json.contains("\"image\":"), "the state payload still has an image field: {json}");
    assert!(json.contains("\"has_image\":true"));
}

// image_text is a search index, never shown: proven the same way the
// picture bytes above are: the frontend's own state payload must not
// carry it at all, not even the field name.
#[test]
fn slim_prompt_never_carries_image_text() {
    let mut p = prompt_with_image();
    p.image_text = "invoice total due friday".into();
    let json = serde_json::to_string(&SlimPrompt::from(&p)).unwrap();
    assert!(!json.contains("invoice"), "OCR-recognized text leaked into the state payload: {json}");
    assert!(!json.contains("image_text"), "the state payload still has an image_text field: {json}");
}

#[test]
fn slim_prompt_image_rev_is_stable_and_changes_with_the_image() {
    let p = prompt_with_image();
    let rev = SlimPrompt::from(&p).image_rev;
    assert_ne!(rev, 0);
    assert_eq!(SlimPrompt::from(&p).image_rev, rev, "same image must hash the same");
    let mut other = p.clone();
    other.image = "data:image/png;base64,QQ==".into();
    assert_ne!(SlimPrompt::from(&other).image_rev, rev);
}

// ---- Takeover offer must not be hidden by the onboarding examples ----

pub(super) fn prompt_named(id: &str) -> Prompt {
    serde_json::from_str(&format!(r#"{{"id":"{id}","name":"n","text":"t"}}"#))
        .expect("prompt fixture")
}

#[test]
fn takeover_treats_seeded_examples_only_as_empty() {
    let examples = vec!["e1".to_string(), "e2".to_string()];
    let prompts = vec![prompt_named("e1"), prompt_named("e2")];
    assert!(store_has_no_real_prompts(&prompts, &examples), "example-only store must count as empty");
}

#[test]
fn takeover_sees_a_real_prompt_even_alongside_examples() {
    let examples = vec!["e1".to_string()];
    let prompts = vec![prompt_named("e1"), prompt_named("own")];
    assert!(!store_has_no_real_prompts(&prompts, &examples), "a non-example prompt must block the offer");
}

#[test]
fn takeover_empty_store_counts_as_empty() {
    assert!(store_has_no_real_prompts(&[], &[]));
}

// ---- Takeover offer: info, and the three answer paths ----

#[test]
fn takeover_offer_info_reports_the_real_size_and_prompt_count() {
    let root = std::env::temp_dir().join(format!("{}-test-takeover-offer-info", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mut conn = full_schema_db(&root.join("data.db"), "info");
    db_write_prompts(&mut conn, &numbered_prompts(4), &mut WrittenRows::new()).unwrap();
    drop(conn);
    std::fs::write(root.join("settings.json"), b"{}").unwrap();

    let out = takeover_offer_info(&root);
    assert_eq!(out.prompts, Some(4), "the count must come from the real database, not a guess");
    let expected_size = fs::metadata(root.join("data.db")).unwrap().len() + fs::metadata(root.join("settings.json")).unwrap().len();
    assert_eq!(out.size, expected_size, "size must be the actual bytes on disk, not an estimate");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn takeover_offer_info_names_an_unreadable_database_as_unknown_rather_than_failing() {
    let root = std::env::temp_dir().join(format!("{}-test-takeover-offer-info-bad-db", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("data.db"), b"not a real database").unwrap();

    let out = takeover_offer_info(&root);
    assert_eq!(out.prompts, None, "a database that cannot be opened must report an unknown count, not crash or lie");

    let _ = std::fs::remove_dir_all(&root);
}

// "uebernehmen" (adopt): the copy lands, the source is untouched, and; since
// the same source is asked about: a remembered decision stops a second offer.
#[test]
fn takeover_decision_adopt_is_remembered_and_stops_a_future_offer() {
    let root = std::env::temp_dir().join(format!("{}-test-takeover-decision-adopt", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    let src = root.join("src");
    let dst = root.join("dst");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();
    std::fs::write(src.join("data.db"), b"source content").unwrap();

    let mut landed = false;
    copy_store_files(&src, &dst, &mut landed).unwrap();
    assert_eq!(fs::read(src.join("data.db")).unwrap(), b"source content", "adopting must never touch the original");
    assert_eq!(fs::read(dst.join("data.db")).unwrap(), b"source content", "the copy must land before anything is settled");

    let mut ui_texts: HashMap<String, String> = HashMap::new();
    assert!(!takeover_already_decided(&ui_texts, &src), "an unanswered location must still be offered");
    takeover_record_decision(&mut ui_texts, &src, true, true);
    assert!(takeover_already_decided(&ui_texts, &src), "an adopted, remembered decision must stop the next offer");

    let _ = std::fs::remove_dir_all(&root);
}

// "getrennt lassen" (keep separate): nothing is copied, but the choice is
// still remembered for that exact source so it does not nag every launch.
#[test]
fn takeover_decision_keep_separate_is_remembered_without_copying_anything() {
    let root = std::env::temp_dir().join(format!("{}-test-takeover-decision-skip", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    let src = root.join("src");
    let dst = root.join("dst");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();
    std::fs::write(src.join("data.db"), b"source content").unwrap();

    let mut ui_texts: HashMap<String, String> = HashMap::new();
    takeover_record_decision(&mut ui_texts, &src, false, true);

    assert!(takeover_already_decided(&ui_texts, &src), "a remembered 'keep separate' must stop the next offer too");
    assert_eq!(
        ui_texts.get(&takeover_decision_key(&src)).map(String::as_str),
        Some("skip"),
        "the recorded answer must say skip, not be indistinguishable from adopt"
    );
    assert!(!dst.join("data.db").exists(), "keeping separate must never write anything into the new location");
    assert_eq!(fs::read(src.join("data.db")).unwrap(), b"source content", "the source must stay exactly as it was");

    let _ = std::fs::remove_dir_all(&root);
}

// "keine Antwort" (no answer): nothing is recorded, so the exact same folder
// is offered again next launch, and neither side was ever touched.
#[test]
fn takeover_decision_no_answer_records_nothing_and_asks_again() {
    let root = std::env::temp_dir().join(format!("{}-test-takeover-decision-none", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("data.db"), b"source content").unwrap();

    let mut ui_texts: HashMap<String, String> = HashMap::new();
    // Mirrors what takeover_apply does when the dialog closes with the
    // "do not ask again" box left unticked: remember == false either way.
    takeover_record_decision(&mut ui_texts, &src, false, false);

    assert!(ui_texts.is_empty(), "an unremembered answer must leave no trace at all");
    assert!(!takeover_already_decided(&ui_texts, &src), "the same folder must still be offered on the next launch");
    assert_eq!(fs::read(src.join("data.db")).unwrap(), b"source content", "the source must be untouched");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn slim_prompt_without_an_image_reports_no_revision() {
    let mut p = prompt_with_image();
    p.image.clear();
    let slim = SlimPrompt::from(&p);
    assert!(!slim.has_image);
    assert_eq!(slim.image_rev, 0);
}

// A rename must not cost the user their picture: the editor sends no image
// field at all unless it changed, and that has to mean "keep what is stored".
#[test]
fn update_without_an_image_field_keeps_the_stored_image() {
    let mut p = prompt_with_image();
    p.name = "Renamed".into(); // the only thing this save touches
    p.image = merge_image(&p.image, None);
    assert_eq!(p.image, TEST_PNG, "a save that sent no image erased the stored one");
}

#[test]
fn only_an_explicit_empty_image_clears_it() {
    let mut p = prompt_with_image();
    p.image = merge_image(&p.image, Some(String::new()));
    assert!(p.image.is_empty(), "the remove-image click must still clear the picture");
}

#[test]
fn a_new_image_replaces_the_stored_one() {
    let mut p = prompt_with_image();
    p.image = merge_image(&p.image, Some("data:image/png;base64,QQ==".into()));
    assert_eq!(p.image, "data:image/png;base64,QQ==");
}

// Pure text-logic tests for ocr_join_lines: no Windows API involved, so
// these run on any platform via plain `cargo test --lib`.
#[test]
fn ocr_join_lines_joins_a_wrapped_sentence() {
    let lines = vec!["This line wraps".to_string(), "into the next one.".to_string()];
    assert_eq!(ocr_join_lines(&lines), "This line wraps into the next one.");
}

#[test]
fn ocr_join_lines_keeps_complete_sentences_separate() {
    let lines = vec!["First sentence.".to_string(), "Second sentence.".to_string()];
    assert_eq!(ocr_join_lines(&lines), "First sentence.\nSecond sentence.");
}

#[test]
fn ocr_join_lines_blank_line_separates_paragraphs() {
    let lines = vec![
        "Paragraph one.".to_string(),
        "".to_string(),
        "Paragraph two.".to_string(),
    ];
    assert_eq!(ocr_join_lines(&lines), "Paragraph one.\n\nParagraph two.");
}

#[test]
fn ocr_join_lines_empty_input_yields_empty_string() {
    let lines: Vec<String> = vec![];
    assert_eq!(ocr_join_lines(&lines), "");
}

// An image whose lines all come back empty must assemble to a genuinely
// empty string (a whitespace-only join, e.g. "\n\n", used to slip through
// and open the result dialog with nothing visible in it).
#[test]
fn ocr_assemble_text_all_empty_lines_yields_empty_string() {
    let lines = vec!["".to_string(), "".to_string()];
    assert_eq!(ocr_assemble_text(&lines, false), "");
    assert_eq!(ocr_assemble_text(&lines, true), "");
}

#[test]
fn ocr_assemble_text_trims_surrounding_whitespace_from_real_text() {
    let lines = vec!["  Hello world  ".to_string()];
    assert_eq!(ocr_assemble_text(&lines, false), "Hello world");
}

// A hyphen is not sentence-ending punctuation, so a hyphen-wrapped word is
// space-joined the same as any other mid-sentence wrap: there is no special
// dehyphenation: one rule, no script-specific cases.
#[test]
fn ocr_join_lines_hyphenated_wrap_gets_a_plain_space_join() {
    let lines = vec!["This is a hyphen-".to_string(), "ated example.".to_string()];
    assert_eq!(ocr_join_lines(&lines), "This is a hyphen- ated example.");
}

// Right-to-left scripts are not special-cased: the same edge-only rule applies,
// and it never assumes an LTR character order, so a joined Arabic line is
// untouched beyond the same whitespace handling as any other script.
#[test]
fn ocr_join_lines_right_to_left_script_is_not_reordered() {
    let lines = vec!["مرحبا بالعالم".to_string()];
    assert_eq!(ocr_join_lines(&lines), "مرحبا بالعالم");
}

pub(super) const OCR_PREFIX: &str = "Language.OCR~~~";
pub(super) const SPEECH_PREFIX: &str = "Language.Speech~~~";

// Capability-name allowlist: the gate everything in
// ocr_install_language relies on. Valid, realistic tags pass; anything
// malformed or injection-flavored is rejected outright.
#[test]
fn ocr_capability_name_accepts_realistic_tags() {
    for name in [
        "Language.OCR~~~de-DE~0.0.1.0",
        "Language.OCR~~~en-US~0.0.1.0",
        "Language.OCR~~~ja-JP~0.0.1.0",
        "Language.OCR~~~zh-Hans-CN~0.0.1.0",
    ] {
        assert!(ocr_capability_name_is_well_formed(OCR_PREFIX, name), "expected {name} to be accepted");
    }
}

// The whole point of this fix: a real speech capability name (the
// exact shape voice_install_language actually gets called with) used to be
// rejected outright because the check hardcoded the OCR prefix regardless
// of kind, so no speech language pack could ever be installed.
#[test]
fn ocr_capability_name_accepts_speech_names_with_the_speech_prefix() {
    assert!(ocr_capability_name_is_well_formed(SPEECH_PREFIX, "Language.Speech~~~de-DE~0.0.1.0"));
}

// No cross-wiring between the two: an OCR-shaped name must not pass under
// the speech prefix and vice versa.
#[test]
fn ocr_capability_name_rejects_the_other_kinds_prefix() {
    assert!(!ocr_capability_name_is_well_formed(SPEECH_PREFIX, "Language.OCR~~~de-DE~0.0.1.0"));
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, "Language.Speech~~~de-DE~0.0.1.0"));
}

#[test]
fn ocr_capability_name_rejects_the_spec_example_payload() {
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, "Language.OCR~~~de-DE~0.0.1.0; rm -rf"));
}

#[test]
fn ocr_capability_name_rejects_command_injection_payloads() {
    for name in [
        "Language.OCR~~~de-DE~0.0.1.0;whoami",
        "Language.OCR~~~de-DE~0.0.1.0`whoami`",
        "Language.OCR~~~de-DE~0.0.1.0|whoami",
        "Language.OCR~~~de-DE~0.0.1.0\nwhoami",
    ] {
        assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, name), "expected {name} to be rejected");
    }
}

#[test]
fn ocr_capability_name_rejects_empty_string() {
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, ""));
}

#[test]
fn ocr_capability_name_rejects_missing_prefix() {
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, "de-DE~0.0.1.0"));
}

#[test]
fn ocr_capability_name_rejects_wrong_trailing_version() {
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, "Language.OCR~~~de-DE~1.0.0.0"));
}

#[test]
fn ocr_capability_name_rejects_a_tag_over_12_chars() {
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, "Language.OCR~~~thirteenchars~0.0.1.0"));
}

#[test]
fn ocr_capability_name_rejects_tag_chars_outside_the_allowed_class() {
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, "Language.OCR~~~de_DE~0.0.1.0"));
    assert!(!ocr_capability_name_is_well_formed(OCR_PREFIX, "Language.OCR~~~de DE~0.0.1.0"));
}

// Same allowlist string, as a tag extractor: pulls the tag back out of an
// already-valid name.
#[test]
fn ocr_tag_from_capability_name_extracts_the_tag() {
    assert_eq!(ocr_tag_from_capability_name(OCR_PREFIX, "Language.OCR~~~de-DE~0.0.1.0"), Some("de-DE".to_string()));
    assert_eq!(ocr_tag_from_capability_name(OCR_PREFIX, "not-a-capability-name"), None);
}

// The same extraction under the speech prefix: this used to always
// return None for a real speech name (empty tag -> every speech install's
// result/script file collided under the same "-voice-install-.json" path).
#[test]
fn ocr_tag_from_capability_name_extracts_the_tag_for_speech() {
    assert_eq!(ocr_tag_from_capability_name(SPEECH_PREFIX, "Language.Speech~~~de-DE~0.0.1.0"), Some("de-DE".to_string()));
}

// PowerShell `Get-WindowsCapability | ConvertTo-Json` output parsing --
// fixed sample strings standing in for the three shapes PowerShell actually
// produces.
#[test]
fn ocr_parse_capabilities_json_handles_a_single_object() {
    let json = r#"{
            "Name": "Language.OCR~~~de-DE~0.0.1.0",
            "State": "Installed",
            "Version": "0.0.1.0",
            "DownloadSize": 12345678,
            "InstallSize": 23456789
        }"#;
    let caps = ocr_parse_capabilities_json(OCR_PREFIX, json).unwrap();
    assert_eq!(caps.len(), 1);
    assert_eq!(caps[0].name, "Language.OCR~~~de-DE~0.0.1.0");
    assert_eq!(caps[0].tag, "de-DE");
    assert_eq!(caps[0].state, "Installed");
    assert_eq!(caps[0].version, "0.0.1.0");
    assert_eq!(caps[0].download_size, Some(12345678));
    assert_eq!(caps[0].install_size, Some(23456789));
}

#[test]
fn ocr_parse_capabilities_json_handles_an_array() {
    let json = r#"[
            {
                "Name": "Language.OCR~~~de-DE~0.0.1.0",
                "State": "NotPresent",
                "Version": "0.0.1.0",
                "DownloadSize": 12345678,
                "InstallSize": 23456789
            },
            {
                "Name": "Language.OCR~~~en-US~0.0.1.0",
                "State": "Installed",
                "Version": "0.0.1.0",
                "DownloadSize": 9999999,
                "InstallSize": 8888888
            }
        ]"#;
    let caps = ocr_parse_capabilities_json(OCR_PREFIX, json).unwrap();
    assert_eq!(caps.len(), 2);
    assert_eq!(caps[0].tag, "de-DE");
    assert_eq!(caps[0].state, "NotPresent");
    assert_eq!(caps[1].tag, "en-US");
    assert_eq!(caps[1].state, "Installed");
}

// An already-installed capability commonly has no DownloadSize/InstallSize
// at all: that must become None, never Some(0) or a panic.
#[test]
fn ocr_parse_capabilities_json_missing_size_becomes_none() {
    let json = r#"{
            "Name": "Language.OCR~~~fr-FR~0.0.1.0",
            "State": "Installed",
            "Version": "0.0.1.0",
            "DownloadSize": null,
            "InstallSize": null
        }"#;
    let caps = ocr_parse_capabilities_json(OCR_PREFIX, json).unwrap();
    assert_eq!(caps.len(), 1);
    assert_eq!(caps[0].download_size, None);
    assert_eq!(caps[0].install_size, None);
}

// Real capture from a VM: `State` comes back as a bare integer, `Version` as
// JSON null, and DownloadSize/InstallSize as a literal 0 rather than absent
//: none of which the original hand-authored fixtures above (a wrong guess
// at the real shape, not tested against actual PowerShell output) exercised.
// A second-opinion review + a real VM capture both caught this; the
// automated suite passed the whole time regardless. State 4 = Installed
// (confirmed against a capability independently proven installed by using
// it for real OCR), 0 = NotPresent.
#[test]
fn ocr_parse_capabilities_json_handles_the_real_numeric_shape() {
    let json = r#"[
            { "Name": "Language.OCR~~~de-DE~0.0.1.0", "State": 4, "Version": null, "DownloadSize": 0, "InstallSize": 0 },
            { "Name": "Language.OCR~~~fr-FR~0.0.1.0", "State": 0, "Version": null, "DownloadSize": 0, "InstallSize": 0 },
            { "Name": "Language.OCR~~~ar-SA~0.0.1.0", "State": 4, "Version": null, "DownloadSize": 0, "InstallSize": 1053812 }
        ]"#;
    let caps = ocr_parse_capabilities_json(OCR_PREFIX, json).unwrap();
    assert_eq!(caps.len(), 3);
    assert_eq!(caps[0].state, "Installed");
    assert_eq!(caps[0].version, "");
    assert_eq!(caps[0].download_size, None);
    assert_eq!(caps[0].install_size, None); // literal 0 is "not reported", not a real zero-byte install
    assert_eq!(caps[1].state, "NotPresent");
    assert_eq!(caps[2].state, "Installed");
    assert_eq!(caps[2].install_size, Some(1053812)); // a real nonzero size still comes through
}

// An unrecognized numeric state code must not be mistaken for Installed --
// understating readiness is the safe direction to be wrong in.
#[test]
fn ocr_parse_capabilities_json_unknown_numeric_state_is_not_present() {
    let json = r#"{ "Name": "Language.OCR~~~de-DE~0.0.1.0", "State": 2, "Version": null, "DownloadSize": 0, "InstallSize": 0 }"#;
    let caps = ocr_parse_capabilities_json(OCR_PREFIX, json).unwrap();
    assert_eq!(caps[0].state, "NotPresent");
}

// Parsed under the speech prefix, a real speech capability entry gets
// its tag too: this used to come out empty for every single row (the
// shared parser hardcoded the OCR prefix), which is why "manage languages"
// showed a blank/garbage label for every speech language even before an
// install was ever attempted.
#[test]
fn ocr_parse_capabilities_json_extracts_the_tag_for_speech_entries() {
    let json = r#"{ "Name": "Language.Speech~~~de-DE~0.0.1.0", "State": "Installed", "Version": "0.0.1.0", "DownloadSize": null, "InstallSize": null }"#;
    let caps = ocr_parse_capabilities_json(SPEECH_PREFIX, json).unwrap();
    assert_eq!(caps[0].tag, "de-DE");
}

// "Updatable?" decision: table-driven per the spec's five cases.
#[test]
fn ocr_lang_needs_update_table() {
    let installed = |v: &str| Some(OcrCapSnapshot { version: v.to_string(), installed: true });
    let not_installed = |v: &str| Some(OcrCapSnapshot { version: v.to_string(), installed: false });

    // same version + Installed -> not updatable
    assert!(!ocr_lang_needs_update(installed("0.0.1.0").as_ref(), "Installed", "0.0.1.0"));
    // different version + Installed -> updatable
    assert!(ocr_lang_needs_update(installed("0.0.1.0").as_ref(), "Installed", "0.0.2.0"));
    // previously Installed, now NotPresent -> updatable
    assert!(ocr_lang_needs_update(installed("0.0.1.0").as_ref(), "NotPresent", "0.0.1.0"));
    // never-installed NotPresent (a remembered-but-never-installed snapshot) -> not updatable
    assert!(!ocr_lang_needs_update(not_installed("0.0.1.0").as_ref(), "NotPresent", "0.0.1.0"));
    // first-time-seen language, no previous snapshot at all -> not updatable
    assert!(!ocr_lang_needs_update(None, "NotPresent", "0.0.1.0"));
}

#[test]
fn ocr_validate_path_rejects_a_disallowed_extension() {
    let mut p = std::env::temp_dir();
    p.push("ocr_validate_path_test.txt");
    fs::write(&p, b"not an image").unwrap();
    let result = ocr_validate_path(p.to_str().unwrap());
    let _ = fs::remove_file(&p);
    assert!(result.is_err());
}

#[test]
fn ocr_validate_path_rejects_a_missing_file() {
    let p = std::env::temp_dir().join("ocr_validate_path_does_not_exist.png");
    assert!(ocr_validate_path(p.to_str().unwrap()).is_err());
}

#[test]
fn ocr_validate_path_accepts_a_real_image_file() {
    let mut p = std::env::temp_dir();
    p.push("ocr_validate_path_test.png");
    fs::write(&p, b"\x89PNG\r\n\x1a\nnot a real png body, just a valid extension/size check").unwrap();
    let result = ocr_validate_path(p.to_str().unwrap());
    let _ = fs::remove_file(&p);
    assert!(result.is_ok());
}

#[test]
fn winrt_file_path_flattens_slashes_and_verbatim_prefixes() {
    assert_eq!(winrt_file_path("C:/bfb/media/a.jpg"), r"C:\bfb\media\a.jpg");
    assert_eq!(winrt_file_path(r"C:\x\a.jpg"), r"C:\x\a.jpg");
    assert_eq!(winrt_file_path(r"\\?\C:\x\a.jpg"), r"C:\x\a.jpg");
    assert_eq!(winrt_file_path(r"\\?\UNC\srv\share\a.jpg"), r"\\srv\share\a.jpg");
    assert_eq!(winrt_file_path(r"\\srv/share/a.jpg"), r"\\srv\share\a.jpg");
}

#[test]
fn ocr_resize_target_leaves_a_normally_sized_image_alone() {
    assert_eq!(ocr_resize_target(800, 600, 4200, 300), None);
}

#[test]
fn ocr_resize_target_downscales_an_oversized_image_proportionally() {
    let (nw, nh) = ocr_resize_target(8400, 4200, 4200, 300).unwrap();
    assert_eq!((nw, nh), (4200, 2100));
}

#[test]
fn ocr_resize_target_upscales_a_small_crop_2x() {
    assert_eq!(ocr_resize_target(120, 80, 4200, 300), Some((240, 160)));
}

// ---- Voice dictation ----

#[cfg(windows)]
#[test]
fn voice_pick_lang_auto_without_any_installed_pack_fails() {
    let err = voice::pick_lang("auto", true, &[], None).unwrap_err();
    assert_eq!(err, "voice-no-language");
}

#[cfg(windows)]
#[test]
fn voice_pick_lang_auto_prefers_the_installed_system_language() {
    let installed = vec!["de-DE".to_string(), "en-US".to_string()];
    assert_eq!(voice::pick_lang("auto", true, &installed, Some("en-US")).unwrap(), "en-US");
    // System language has no pack installed -> falls back to whatever IS there.
    assert_eq!(voice::pick_lang("auto", true, &installed, Some("fr-FR")).unwrap(), "de-DE");
}

#[cfg(windows)]
#[test]
fn voice_pick_lang_unknown_tag_falls_back_when_auto_pick_is_on() {
    let installed = vec!["de-DE".to_string()];
    assert_eq!(voice::pick_lang("fr-FR", true, &installed, None).unwrap(), "de-DE");
}

#[cfg(windows)]
#[test]
fn voice_pick_lang_unknown_tag_fails_when_auto_pick_is_off() {
    let installed = vec!["de-DE".to_string()];
    let err = voice::pick_lang("fr-FR", false, &installed, None).unwrap_err();
    assert_eq!(err, "voice-no-language");
}

#[cfg(windows)]
#[test]
fn voice_pick_lang_an_installed_explicit_tag_is_used_as_is() {
    let installed = vec!["de-DE".to_string(), "en-US".to_string()];
    assert_eq!(voice::pick_lang("en-US", false, &installed, None).unwrap(), "en-US");
}

#[cfg(windows)]
#[test]
fn voice_hresult_table_maps_every_measured_code() {
    assert_eq!(voice::error_key_for_hresult(0x800455BCu32 as i32), "voice-no-language");
    assert_eq!(voice::error_key_for_hresult(0x80070005u32 as i32), "voice-denied");
    assert_eq!(voice::error_key_for_hresult(0x80045509u32 as i32), "voice-privacy-off");
    assert_eq!(voice::error_key_for_hresult(0x80004005u32 as i32), "voice-failed"); // any other HRESULT
}

#[cfg(windows)]
#[test]
fn voice_stop_reason_table() {
    // Windows' own AutoStopSilenceTimeout: always "silence", regardless of
    // what (if anything) we had requested.
    assert_eq!(voice::stop_reason(7, None), "silence");
    assert_eq!(voice::stop_reason(7, Some("max")), "silence");
    // Success (0): whatever we asked for, defaulting to "manual".
    assert_eq!(voice::stop_reason(0, None), "manual");
    assert_eq!(voice::stop_reason(0, Some("max")), "max");
    assert_eq!(voice::stop_reason(0, Some("shutdown")), "shutdown");
    // Anything else is a genuine failure.
    assert_eq!(voice::stop_reason(1, None), "error");
    assert_eq!(voice::stop_reason(-1, None), "error");
}

#[cfg(windows)]
#[test]
fn voice_should_stop_silence_window_and_zero_means_off() {
    use std::time::Duration;
    // Idle past the silence window, well under the hard cap -> "silence".
    assert_eq!(voice::should_stop(Duration::from_secs(6), Duration::from_secs(10), 5, 300), Some("silence"));
    // Not idle long enough yet.
    assert_eq!(voice::should_stop(Duration::from_secs(2), Duration::from_secs(10), 5, 300), None);
    // auto_stop_sec == 0 -> the idle check never fires, no matter how idle.
    assert_eq!(voice::should_stop(Duration::from_secs(999), Duration::from_secs(10), 0, 300), None);
}

#[cfg(windows)]
#[test]
fn voice_should_stop_hard_cap_always_wins() {
    use std::time::Duration;
    // Elapsed has reached max_sec even though nothing was idle at all ->
    // "max", not "silence" and not None.
    assert_eq!(voice::should_stop(Duration::from_secs(0), Duration::from_secs(300), 5, 300), Some("max"));
    // The cap wins even when voiceAutoStop (auto_stop_sec) is off.
    assert_eq!(voice::should_stop(Duration::from_secs(0), Duration::from_secs(300), 0, 300), Some("max"));
}

#[cfg(windows)]
#[test]
fn voice_clamp_durations_enforces_the_real_1_to_60_min_range() {
    // The UI slider only goes to 60 min/60 sec, but the backend is the
    // real limit: a value above the slider's own max must still be cut down.
    assert_eq!(voice::clamp_durations(3600, 60), (3600, 60)); // exactly at the new ceiling
    assert_eq!(voice::clamp_durations(7200, 999), (3600, 60)); // above it -> clamped
    assert_eq!(voice::clamp_durations(0, 0), (60, 0)); // below the floor -> min 60s/1min
}

#[test]
fn voice_should_send_stop_skips_a_session_that_already_ended_itself() {
    assert!(!should_send_stop(None, 5, true));
    assert!(!should_send_stop(Some(5), 5, true));
}

// ---- Kind-generalized language-pack elevation ----

// start_request keeps one pending nonce per kind: the tests that start requests take turns.
#[cfg(windows)]
static ELEVATION_REQUEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(windows)]
#[test]
fn ocr_elevated_capabilities_result_survives_a_truncated_read() {
    let _serial = ELEVATION_REQUEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let nonce = ocr::start_request(ocr::CapKind::Ocr, ocr::CAPS_TAG);
    let path = ocr::caps_result_path(ocr::CapKind::Ocr, &nonce);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&path, b"{\"Name\":\"Language.OC").unwrap(); // deliberately truncated JSON
    let result = ocr::elevated_capabilities_result(ocr::CapKind::Ocr);
    assert!(matches!(result, Ok(None)));
    assert!(path.exists(), "a truncated/unparseable read must leave the file for the next poll, not discard it");
    let _ = std::fs::remove_file(&path);
}

#[cfg(windows)]
#[test]
fn ocr_elevated_capabilities_result_deletes_on_a_complete_error_payload() {
    let _serial = ELEVATION_REQUEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let nonce = ocr::start_request(ocr::CapKind::Speech, ocr::CAPS_TAG);
    let path = ocr::caps_result_path(ocr::CapKind::Speech, &nonce);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&path, br#"{"error":"boom"}"#).unwrap();
    match ocr::elevated_capabilities_result(ocr::CapKind::Speech) {
        Err(e) => assert_eq!(e, "boom"),
        Ok(_) => panic!("expected an error"),
    }
    assert!(!path.exists(), "a terminal (error) result must be cleared, not left for a duplicate re-read");
}

#[cfg(windows)]
#[test]
fn ocr_and_speech_elevated_result_paths_never_collide() {
    let _serial = ELEVATION_REQUEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let a = ocr::start_request(ocr::CapKind::Ocr, ocr::CAPS_TAG);
    let b = ocr::start_request(ocr::CapKind::Speech, ocr::CAPS_TAG);
    assert_ne!(
        ocr::caps_result_path(ocr::CapKind::Ocr, &a),
        ocr::caps_result_path(ocr::CapKind::Speech, &b)
    );
}

#[cfg(windows)]
#[test]
fn an_elevated_result_is_only_believed_while_a_request_is_in_flight() {
    let _serial = ELEVATION_REQUEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // The temp folder is writable by anything running as this user. With the
    // file name derived from the language tag (or the process id) another
    // process could drop a finished-looking result in ahead of time and the
    // app would report a language pack installed that was never installed.
    // Two nonces in a row must differ, and a file under a name nobody asked
    // for must be ignored outright.
    let first = ocr::start_request(ocr::CapKind::Ocr, ocr::CAPS_TAG);
    let second = ocr::start_request(ocr::CapKind::Ocr, ocr::CAPS_TAG);
    assert_ne!(first, second, "every elevated request needs its own name");
    assert!(first.len() >= 16, "too short to be unguessable: {first}");

    let planted = ocr::caps_result_path(ocr::CapKind::Ocr, "planted-by-someone-else");
    if let Some(dir) = planted.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&planted, br#"{"error":"forged"}"#).unwrap();
    // `second` is the pending nonce and no file exists under it, so this is
    // "still running": the planted file must not be read at all.
    assert!(matches!(ocr::elevated_capabilities_result(ocr::CapKind::Ocr), Ok(None)));
    assert!(planted.exists(), "a file the app never asked for must be left untouched");
    let _ = std::fs::remove_file(&planted);
}

// DISM writing its unconditional startup log to %WINDIR%\Logs\DISM
// (unwritable without admin rights) was the actual cause of "jetzt
// prüfen" failing on a standard account: redirected into our own
// always-writable temp folder instead, single-quoted like every other
// embedded path in this file.
#[cfg(windows)]
#[test]
fn dism_log_args_redirects_into_our_own_writable_temp_dir() {
    let ocr = ocr::dism_log_args(ocr::CapKind::Ocr);
    let voice = ocr::dism_log_args(ocr::CapKind::Speech);
    assert!(ocr.contains("-LogPath '") && ocr.contains("-ScratchDirectory '"), "{ocr}");
    assert!(ocr.contains("-ocr-dism.log"), "{ocr}");
    assert!(voice.contains("-voice-dism.log"), "{voice}");
    assert_ne!(ocr, voice, "each kind must log to its own file, not share one");
}

// 177 part 3: whatever the real failure was (log path, a flat "needs
// elevation", anything else), the frontend always gets the SAME stable
// code: with the original text still attached, not silently dropped.
#[cfg(windows)]
#[test]
fn caps_elevation_error_keeps_the_stable_code_and_the_raw_text() {
    let wrapped = ocr::caps_elevation_error("Der Zugriff auf den Pfad wurde verweigert.");
    assert!(wrapped.starts_with("ocr-caps-need-elevation|"));
    assert!(wrapped.ends_with("Der Zugriff auf den Pfad wurde verweigert."));
}

// A cancelled or declined elevation dialog is always worth a retry --
// Windows lets that same credential prompt take a DIFFERENT
// administrator's password next time, so this account's own rights are
// not the last word. Only a Group Policy that denies the prompt outright
// (no dialog ever shown, to anyone) is genuinely final.
#[cfg(windows)]
#[test]
fn classify_elevation_refusal_treats_a_decline_as_retryable() {
    assert_eq!(ocr::classify_elevation_refusal(false), ocr::ElevationRefusal::Retryable);
}

#[cfg(windows)]
#[test]
fn classify_elevation_refusal_treats_a_policy_denial_as_final() {
    assert_eq!(ocr::classify_elevation_refusal(true), ocr::ElevationRefusal::PolicyBlocked);
}

#[cfg(windows)]
#[test]
fn strip_ps_banner_keeps_only_the_first_line() {
    assert_eq!(ocr::strip_ps_banner("  Access is denied.  "), "Access is denied.");
    assert_eq!(
        ocr::strip_ps_banner("Get-WindowsCapability : Access is denied.\nAt line:1 char:1\n+ ..."),
        "Get-WindowsCapability : Access is denied."
    );
    assert_eq!(ocr::strip_ps_banner(""), "");
}

#[test]
fn voice_should_send_stop_targets_the_right_running_session() {
    assert!(should_send_stop(None, 5, false));
    assert!(should_send_stop(Some(5), 5, false));
    assert!(!should_send_stop(Some(3), 5, false)); // a stale/foreign id never stops the live session
}

#[test]
fn voice_text_event_serialises_with_its_token() {
    let json = serde_json::to_string(&VoiceTextEvent { id: 1, token: 42, text: "hi".into() }).unwrap();
    assert!(json.contains("\"token\":42"));
}

#[test]
fn data_url_within_limit_rejects_one_byte_over_and_accepts_exactly_at() {
    let at_limit = "x".repeat(10);
    let one_over = "x".repeat(11);
    assert!(data_url_within_limit(&at_limit, 10));
    assert!(!data_url_within_limit(&one_over, 10));
}

// ---- Image search text ----

#[test]
fn cap_ocr_text_cuts_by_character_not_by_byte() {
    // "café" is 4 chars but 5 bytes in UTF-8: a byte-based cut would
    // split the "é" and produce invalid UTF-8 (or panic).
    assert_eq!(cap_ocr_text("café", 4), "café");
    assert_eq!(cap_ocr_text("café", 3), "caf");
    assert_eq!(cap_ocr_text("hi", 10), "hi");
}

#[test]
fn resolved_image_text_only_recognizes_a_non_empty_picture_with_the_switch_on() {
    let calls = std::cell::Cell::new(0);
    let recognize = |img: &str| { calls.set(calls.get() + 1); format!("seen:{img}") };
    // Switch off: never even calls the recognizer, whatever the picture.
    assert_eq!(resolved_image_text("data:image/png;base64,AAA", false, recognize), "");
    assert_eq!(calls.get(), 0);
    // Picture removed: empties the field and never calls the recognizer either.
    assert_eq!(resolved_image_text("", true, recognize), "");
    assert_eq!(calls.get(), 0);
    // Switch on, a real picture: calls the recognizer exactly once and keeps
    // whatever it returns.
    assert_eq!(resolved_image_text("data:image/png;base64,AAA", true, recognize), "seen:data:image/png;base64,AAA");
    assert_eq!(calls.get(), 1);
}

pub(super) fn image_text_prompt(id: &str, image_text: &str) -> Prompt {
    let mut p = test_prompt(id);
    p.image_text = image_text.into();
    p
}

#[test]
fn clear_image_text_empties_every_index_and_reports_whether_it_changed() {
    let mut prompts = vec![image_text_prompt("a", "invoice total"), image_text_prompt("b", ""), image_text_prompt("c", "receipt")];
    assert!(clear_image_text(&mut prompts));
    assert!(prompts.iter().all(|p| p.image_text.is_empty()));
    assert!(!clear_image_text(&mut prompts), "nothing left to clear the second time");
}

#[test]
fn image_text_matches_is_empty_with_the_switch_off_even_for_a_real_hit() {
    let prompts = vec![image_text_prompt("a", "invoice total due friday")];
    assert!(image_text_matches(&prompts, false, "invoice").is_empty());
}

#[test]
fn image_text_matches_finds_a_case_insensitive_substring_and_skips_empty_queries() {
    let prompts = vec![
        image_text_prompt("a", "Invoice Total Due Friday"),
        image_text_prompt("b", "just a landscape photo, no text"),
        image_text_prompt("c", ""),
    ];
    assert_eq!(image_text_matches(&prompts, true, "invoice"), vec!["a".to_string()]);
    assert!(image_text_matches(&prompts, true, "   ").is_empty(), "a blank query must match nothing, not everything");
    assert!(image_text_matches(&prompts, true, "zzz-no-such-word").is_empty());
}

// import_prompts refuses a file that changed since preview_import hashed
// it (the real gate is a one-line `!=` compare in import_prompts itself,
// which needs a live AppHandle to exercise directly: this proves the
// hash it compares against actually distinguishes an edited file from
// the original and is stable for an unchanged one).
#[test]
fn import_file_hash_changes_with_the_file_content() {
    let original = import_file_hash(b"name,text\r\nA,B\r\n");
    let edited = import_file_hash(b"name,text\r\nA,C\r\n");
    assert_ne!(original, edited);
    assert_eq!(original, import_file_hash(b"name,text\r\nA,B\r\n"));
}

#[test]
fn a_theme_that_no_longer_ships_falls_back_to_system() {
    let mut s = Settings::default();
    // Themes dropped in 3.1.0: an installation set to one of them must still
    // start with a full set of colour tokens.
    for gone in ["gradient", "sunset", "forest", "midnight", "retro", "lavender", "candy", "coffee"] {
        assert_eq!(theme_base_of(&s, gone), "system", "{gone} should fall back");
    }
    assert_eq!(theme_base_of(&s, "ocean"), "ocean");
    assert_eq!(theme_base_of(&s, "system"), "system");
    // A custom theme built on a dropped base falls back the same way.
    s.custom_themes.push(CustomTheme {
        id: "t1".into(),
        name: "Mine".into(),
        base: "retro".into(),
        vars: HashMap::new(),
    });
    assert_eq!(theme_base_of(&s, "custom:t1"), "system");
}

#[test]
fn stored_ai_theme_migrates_to_neon_not_system() {
    // "ai" was renamed to "neon": a saved preference must land on the
    // renamed theme, not fall through known_theme() to "system".
    let mut s = Settings { theme: "ai".into(), ..Default::default() };
    s.custom_themes.push(CustomTheme {
        id: "t1".into(),
        name: "Mine".into(),
        base: "ai".into(),
        vars: HashMap::new(),
    });
    s.migrate();
    assert_eq!(s.theme, "neon");
    assert_eq!(s.custom_themes[0].base, "neon");
    assert_eq!(theme_base_of(&s, &s.theme), "neon");
}

#[test]
fn stored_black_and_white_theme_migrates_to_light() {
    // The Black & White theme is gone: it was a light one, so a stored preference and a
    // custom theme built on it land on Light, and the custom colours stay.
    let mut s = Settings { theme: "mono".into(), ..Default::default() };
    s.custom_themes.push(CustomTheme {
        id: "t1".into(),
        name: "Mine".into(),
        base: "mono".into(),
        vars: HashMap::from([("bg".to_string(), "#123456".to_string())]),
    });
    s.migrate();
    assert_eq!(s.theme, "light");
    assert_eq!(s.custom_themes[0].base, "light");
    assert_eq!(s.custom_themes[0].vars.get("bg").map(String::as_str), Some("#123456"));
    assert!(!BUILTIN_THEMES.contains(&"mono"));

    let mut custom = Settings { theme: "custom:t1".into(), ..Default::default() };
    custom.custom_themes.push(CustomTheme {
        id: "t1".into(),
        name: "Mine".into(),
        base: "mono".into(),
        vars: HashMap::new(),
    });
    custom.migrate();
    assert_eq!(theme_base_of(&custom, &custom.theme), "light");
}

// An existing install (real prompts, or the example-seeding marker set --
// even if those examples were later dismissed) must never see the first-run
// tour; only a genuinely blank store should.
#[test]
fn tour_is_marked_seen_for_an_existing_store() {
    assert!(tour_should_be_marked_seen(true, false), "prompts already exist");
    assert!(tour_should_be_marked_seen(false, true), "examples were seeded (even if later dismissed)");
    assert!(tour_should_be_marked_seen(true, true), "both signal a non-fresh store");
}

#[test]
fn tour_is_not_marked_seen_for_a_blank_store() {
    assert!(!tour_should_be_marked_seen(false, false));
}

// ---- Colour-picker history ----

// PICK_HISTORY is one process-wide list: the tests that fill and clear it take turns.
static PICK_HISTORY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn record_pick_keeps_five_newest_and_skips_a_repeat() {
    let _serial = PICK_HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    PICK_HISTORY.lock().unwrap_or_else(|e| e.into_inner()).clear();
    for i in 1..=6u8 {
        record_pick([i, 0, 0], PICK_HISTORY_DEFAULT);
    }
    record_pick([6, 0, 0], PICK_HISTORY_DEFAULT); // the same colour twice in a row is not a new entry
    let hex = pick_history_hex();
    assert_eq!(hex.len(), PICK_HISTORY_DEFAULT);
    // Newest first, and the oldest ([1,0,0]) has fallen off the end.
    assert_eq!(hex[0], "#060000");
    assert_eq!(hex[PICK_HISTORY_DEFAULT - 1], "#020000");
    // Picking an earlier colour again puts it back on top as a fresh entry.
    record_pick([2, 0, 0], PICK_HISTORY_DEFAULT);
    assert_eq!(pick_history_hex()[0], "#020000");
}

#[test]
fn pick_history_persists_across_a_reload_and_respects_a_lowered_cap() {
    let _serial = PICK_HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    PICK_HISTORY.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let mut s = Settings::default();
    record_pick([1, 0, 0], pick_history_max(&s));
    record_pick([2, 0, 0], pick_history_max(&s));
    s.ui_texts.insert("pickHistory".to_string(), pick_history_hex().join(","));
    // Simulate a restart: clear the in-memory list, reload from the mirror.
    PICK_HISTORY.lock().unwrap_or_else(|e| e.into_inner()).clear();
    load_pick_history(&s);
    assert_eq!(pick_history_hex(), vec!["#020000", "#010000"]);
    // Lowering the cap below the stored count trims on the next load.
    s.ui_values.insert("pickHistoryN".to_string(), 1.0);
    load_pick_history(&s);
    assert_eq!(pick_history_hex(), vec!["#020000"]);
    // 0 turns the feature off: the mirror is kept, but nothing loads back in.
    s.ui_values.insert("pickHistoryN".to_string(), 0.0);
    load_pick_history(&s);
    assert!(pick_history_hex().is_empty());
}

// The sync folder is a directory the USER picks, so it can hold their own
// documents. Only files carrying our frontmatter may ever be swept.
#[test]
fn sync_sweep_spares_documents_the_app_did_not_write() {
    let dir = std::env::temp_dir().join(format!("ps-sweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mine = dir.join("p_stale.md");
    let theirs = dir.join("My shopping list.md");
    let obsidian = dir.join("Reading list.md");
    let kept = dir.join("p_live.md");
    fs::write(&mine, prompt_to_md(&sync_test_prompt(), &SyncPlacement::default())).unwrap();
    fs::write(&theirs, "# Shopping\n\n- milk\n").unwrap();
    // Someone else's frontmatter is still not ours.
    fs::write(&obsidian, "---\ntitle: Books\ntags: reading\n---\n\n- Dune\n").unwrap();
    fs::write(&kept, prompt_to_md(&sync_test_prompt(), &SyncPlacement::default())).unwrap();
    fs::write(dir.join("AGENTS.md"), sync_agents_md()).unwrap();

    sync_known_note(&mine);
    sync_seen_record_own_write(&mine);
    sync_known_note(&theirs);
    sync_known_note(&obsidian);
    let mut wanted = std::collections::HashSet::new();
    wanted.insert("p_live.md".to_string());
    sync_sweep_orphans(&dir, &wanted);

    assert!(!mine.exists(), "an orphaned prompt file must go");
    assert!(theirs.exists(), "a document without our frontmatter must survive");
    assert!(obsidian.exists(), "someone else's frontmatter must survive");
    assert!(kept.exists(), "a live prompt's file must stay");
    assert!(dir.join("AGENTS.md").exists(), "the readme must stay");
    let _ = fs::remove_dir_all(&dir);
}

// The checks below compare multi-line source text; a checkout with Windows line endings
// must read the same as one with Unix endings.
macro_rules! source {
    ($path:literal) => {
        include_str!($path).replace("\r\n", "\n")
    };
}

#[test]
fn a_copy_restored_from_a_confidential_entry_is_confidential_and_sealed() {
    let key = [7u8; 32];
    let mut source = test_prompt("src");
    source.secret = true;
    let mut copy = build_restored_prompt(&source, "n", "plain body".to_string(), vec!["plain entry".to_string()], "");
    assert!(!copy.secret);
    seal_restored_prompt(&mut copy, &key);
    assert!(copy.secret);
    assert_ne!(copy.text, "plain body");
    assert_eq!(secret_decrypt(&copy.text, &key).unwrap(), "plain body");
    assert_eq!(secret_decrypt(&copy.variants[0], &key).unwrap(), "plain entry");
}

#[test]
fn result_parts_survive_the_sweep_even_when_old() {
    let dir = std::env::temp_dir().join(format!("ps-test-result-parts-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let part = part_info(&dir, "rec-2026-01-01-00-00-00.mp4");
    let part_mic = dir.join("rec-2026-01-01-00-00-00.mic.wav");
    fs::write(&part_mic, b"mic").unwrap();
    let stray = dir.join("rec-2026-01-01-00-00-09.mp4");
    fs::write(&stray, b"video").unwrap();
    for f in [Path::new(&part.path), part_mic.as_path(), stray.as_path()] {
        backdate(f, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    }
    *REC_RESULT_PARTS.lock().unwrap() = vec![part.clone()];
    let protected = protected_take_paths();
    REC_RESULT_PARTS.lock().unwrap().clear();
    let empty = std::collections::HashSet::new();
    let (count, _) = sweep_stale_recordings_in(&dir, &empty, &protected, false, Duration::from_secs(60), None);
    assert_eq!(count, 1, "only the file that is no part of the open take goes");
    assert!(Path::new(&part.path).exists(), "an earlier part waiting in the result dialog stays");
    assert!(part_mic.exists(), "and so does its mic track");
    assert!(!stray.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn pending_result_file_survives_the_sweep_even_when_old() {
    let dir = std::env::temp_dir().join(format!("ps-test-pending-result-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let shown = dir.join("rec-2026-01-01-00-00-00.mp4");
    let shown_keys = dir.join("rec-2026-01-01-00-00-00.keys.json");
    let other = dir.join("rec-2026-01-01-00-00-05.mp4");
    for f in [&shown, &shown_keys, &other] {
        fs::write(f, b"video").unwrap();
        backdate(f, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    }
    set_pending_recording(Some(shown.to_string_lossy().to_string()));
    let protected = protected_take_paths();
    set_pending_recording(None);
    assert!(protected_take_paths().iter().all(|p| p != &shown), "closing the dialog releases the file");
    let empty = std::collections::HashSet::new();
    let (count, _) = sweep_stale_recordings_in(&dir, &empty, &protected, false, Duration::from_secs(60), None);
    assert_eq!(count, 1);
    assert!(shown.exists() && shown_keys.exists(), "the file an open dialog shows stays with its sidecar");
    assert!(!other.exists());
    let _ = fs::remove_dir_all(&dir);
}

//
// The writers are stood in for by closures: `settings_fails` makes the settings write return
// false (a real write failure, not a locked row), `rows_called` records whether the prompt
// rows were written at all.

pub(super) fn paired_fixture() -> Store {
    let mut settings = Settings::default();
    settings.migrate();
    Store { prompts: Vec::new(), trash: Vec::new(), settings, prompts_locked: false, written: Default::default() }
}

pub(super) fn paired_save<'a>(settings_fails: bool, rows_fail: bool, rows_called: &'a std::cell::Cell<bool>, undone: &'a std::cell::Cell<bool>) -> impl Fn(&Store, &Settings) -> bool + 'a {
    move |_st, _orig| {
        save_pair_or_undo_first(
            || !settings_fails,
            || {
                rows_called.set(true);
                !rows_fail
            },
            || undone.set(true),
        )
    }
}

#[test]
fn a_failed_settings_write_leaves_a_new_prompt_out_of_the_store_and_the_grid() {
    let mut store = paired_fixture();
    let before = serde_json::to_string(&store.settings).unwrap();
    let (rows, undone) = (std::cell::Cell::new(false), std::cell::Cell::new(false));
    let p = test_prompt("n1");
    let r = add_placed_prompt_core(&mut store, &p, paired_save(true, false, &rows, &undone));
    assert_eq!(r, Err("save-failed".to_string()));
    assert!(store.prompts.is_empty());
    assert_eq!(serde_json::to_string(&store.settings).unwrap(), before, "no cell, no member left behind");
    assert!(!rows.get(), "the prompt rows are not written when the settings write failed");
    // And when only the rows fail, the settings written first are taken back.
    let (rows, undone) = (std::cell::Cell::new(false), std::cell::Cell::new(false));
    let r = add_placed_prompt_core(&mut store, &p, paired_save(false, true, &rows, &undone));
    assert_eq!(r, Err("save-failed".to_string()));
    assert!(rows.get() && undone.get());
    assert!(store.prompts.is_empty());
    assert_eq!(serde_json::to_string(&store.settings).unwrap(), before);
    // Both writes landing keeps the prompt and its seat.
    let (rows, undone) = (std::cell::Cell::new(false), std::cell::Cell::new(false));
    add_placed_prompt_core(&mut store, &p, paired_save(false, false, &rows, &undone)).unwrap();
    assert_eq!(store.prompts.len(), 1);
    assert!(store.settings.active_view_mut().members.iter().any(|m| m == "n1"));
}

#[test]
fn a_failed_settings_write_keeps_a_deleted_prompt_in_the_library_with_its_placement() {
    let mut store = paired_fixture();
    let p = test_prompt("d1");
    let (rows, undone) = (std::cell::Cell::new(false), std::cell::Cell::new(false));
    add_placed_prompt_core(&mut store, &p, paired_save(false, false, &rows, &undone)).unwrap();
    store.settings.floating.insert("d1".into(), Pos { x: 1, y: 2, ..Default::default() });
    let before = serde_json::to_string(&store.settings).unwrap();
    let (rows, undone) = (std::cell::Cell::new(false), std::cell::Cell::new(false));
    let r = delete_prompt_core(&mut store, "d1", 9, paired_save(true, false, &rows, &undone));
    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(store.prompts.len(), 1);
    assert!(store.trash.is_empty());
    assert_eq!(serde_json::to_string(&store.settings).unwrap(), before, "view, cell and pill still name the prompt");
    assert!(!rows.get());
    // The batch variant behaves the same.
    let ids = vec!["d1".to_string()];
    let r = delete_batch_core(&mut store, &ids, 9, paired_save(true, false, &rows, &undone));
    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(store.prompts.len(), 1);
    assert!(store.trash.is_empty());
    assert_eq!(serde_json::to_string(&store.settings).unwrap(), before);
    // A pair that lands moves it to the trash.
    assert_eq!(delete_batch_core(&mut store, &ids, 9, paired_save(false, false, &rows, &undone)), Ok(true));
    assert!(store.prompts.is_empty() && store.trash.len() == 1);
}

#[test]
fn a_failed_settings_write_seeds_no_example_prompts() {
    let mut store = paired_fixture();
    let before = serde_json::to_string(&store.settings).unwrap();
    let (rows, undone) = (std::cell::Cell::new(false), std::cell::Cell::new(false));
    let created = vec![test_prompt("e1"), test_prompt("e2")];
    let r = seed_examples_core(&mut store, created.clone(), paired_save(true, false, &rows, &undone));
    assert_eq!(r.err(), Some("save-failed".to_string()));
    assert!(store.prompts.is_empty());
    assert!(!store.settings.examples_seeded);
    assert_eq!(serde_json::to_string(&store.settings).unwrap(), before);
    assert!(!rows.get(), "nothing reached the prompt table, so a retry cannot duplicate");
    let seeded = seed_examples_core(&mut store, created, paired_save(false, false, &rows, &undone)).unwrap();
    assert_eq!(seeded.len(), 2);
    assert!(store.settings.examples_seeded && store.prompts.len() == 2);
}

#[test]
fn leftover_picture_text_is_cleared_at_boot_only_when_the_index_is_off_and_the_set_is_writable() {
    let _epoch = OCR_EPOCH_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = secret_test_store(vec![image_text_prompt("p1", "read from a picture")]);
    store.settings.ui_flags.insert("ocrIndex".into(), true);
    assert!(!heal_ocr_text_at_boot(&mut store), "index on: the text is wanted");
    assert!(!store.prompts[0].image_text.is_empty());

    store.settings.ui_flags.insert("ocrIndex".into(), false);
    store.prompts_locked = true;
    assert!(!heal_ocr_text_at_boot(&mut store), "locked prompt set is never rewritten");
    assert!(!store.prompts[0].image_text.is_empty());
    store.prompts_locked = false;

    store.settings.ui_flags.insert("demoMode".into(), true);
    assert!(!heal_ocr_text_at_boot(&mut store), "demo mode keeps its sample library");
    assert!(!store.prompts[0].image_text.is_empty());
    store.settings.ui_flags.remove("demoMode");

    assert!(heal_ocr_text_at_boot(&mut store), "index off with stored text: cleared");
    assert!(store.prompts[0].image_text.is_empty());
    assert!(!heal_ocr_text_at_boot(&mut store), "nothing left to clear");
}

#[test]
fn a_shutdown_during_voice_prepare_cancels_the_pending_start() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let (claiming, cancel) = (AtomicBool::new(true), AtomicBool::new(false));
    voice_cmds::mark_pending_start_cancelled(&claiming, &cancel);
    assert!(cancel.load(Ordering::SeqCst), "a start that is still preparing must be cancelled");
    let (idle, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
    voice_cmds::mark_pending_start_cancelled(&idle, &cancel);
    assert!(!cancel.load(Ordering::SeqCst), "no pending start, nothing to cancel");
}

// Structural (the start needs an app handle and a microphone): voice_shutdown sets the cancel flag
// while holding the handle slot, so the start has to read the flag, spawn the session and store
// the handle under one hold of that same slot. A check outside it leaves a window in which the
// flag is set after the check and never read, and the microphone session runs with the window hidden.
#[test]
fn the_voice_start_checks_cancel_and_stores_its_handle_under_one_lock() {
    let src = include_str!("../voice_cmds.rs").replace("\r\n", "\n");
    let at = src.find("pub(crate) async fn voice_start").unwrap();
    let body = &src[at..];
    let body = &body[..body.find("\n}\n").unwrap()];
    let after_prepare = &body[body.find("voice::prepare(lang, true)").unwrap()..];
    let slot = after_prepare.find("let mut slot = vstate.0.lock()").expect("the slot is taken after prepare");
    let cancel = after_prepare.find("vstate.2.swap(false").expect("cancel flag is read");
    let spawn = after_prepare.find("voice::start_session(").expect("session spawn");
    let store = after_prepare.find("*slot = Some(VoiceHandle").expect("handle stored through the held slot");
    assert!(slot < cancel && cancel < spawn && spawn < store, "check, spawn and store under the one guard");
    assert_eq!(after_prepare.matches("vstate.0.lock()").count(), 1, "no second, separate lock for the handle");
    // The tray tooltip call blocks on the main thread, whose window-close handler takes this slot:
    // calling it with the slot held deadlocks a hide during start.
    let held = &after_prepare[slot..];
    for (i, _) in held.match_indices("set_tray_recording(") {
        assert!(held[..i].trim_end().ends_with("drop(slot);"), "tray tooltip set while the voice slot is held");
    }
    assert!(after_prepare[..slot].contains("set_tray_recording(&app, true)"), "recording tooltip set before the slot");
}

#[cfg(windows)]
#[test]
fn a_hung_powershell_query_is_killed_at_its_deadline() {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", "Start-Sleep -Seconds 60"]).creation_flags(0x0800_0000);
    let t = std::time::Instant::now();
    let res = ocr::run_with_deadline(cmd, std::time::Duration::from_millis(1500));
    assert_eq!(res, Err("timeout".to_string()));
    assert!(t.elapsed() < std::time::Duration::from_secs(20), "the child must not be waited out");
    let mut quick = std::process::Command::new("powershell.exe");
    quick.args(["-NoProfile", "-NonInteractive", "-Command", "Write-Output ok"]).creation_flags(0x0800_0000);
    assert_eq!(ocr::run_with_deadline(quick, std::time::Duration::from_secs(60)).map(|s| s.trim().to_string()), Ok("ok".to_string()));
}

// Structural fix, no red unit test: a write that fails half-way cannot be provoked on demand.
#[test]
fn the_ocr_temp_guard_exists_before_the_png_is_written() {
    let src = source!("../ocr.rs");
    for save in ["im.save(&out)", "crop.save(&path)"] {
        let at = src.find(save).expect(save);
        let guard = src[..at].rfind("TempFileGuard(Some(").expect("guard");
        assert!(at - guard < 400, "the guard sits right before {save}");
    }
}

// Structural fix, no red unit test: where the work runs cannot be observed from a unit test.
#[cfg(windows)]
#[test]
fn the_ocr_data_url_is_decoded_on_the_blocking_pool() {
    let src = source!("../ocr_cmds.rs");
    let cmd = src.split("pub(crate) async fn ocr_image_data").nth(1).unwrap().split("fn decode_ocr_data_url").next().unwrap();
    let spawn = cmd.find("spawn_blocking").expect("blocking pool");
    let decode = cmd.find("decode_ocr_data_url(&data_url)").expect("decode call");
    assert!(decode > spawn, "decoding must happen inside the blocking closure");
    assert!(!cmd.contains("data_url_bytes("), "no decode before the pool");
    assert!(crate::ocr_cmds::decode_ocr_data_url("data:image/png;base64,AAAA").is_err());
}

// Structural fix, no red unit test: a panicking decoder inside the blocking pool cannot be
// provoked without the real engine.
#[test]
fn a_failed_ocr_index_worker_marks_the_job_as_error() {
    let src = source!("../ocr_cmds.rs");
    let body = src.split("pub(crate) async fn ocr_index_backfill").nth(1).unwrap().split("pub(crate) async fn ocr_capabilities(").next().unwrap();
    let at = body.find("Err(e) => {").expect("join error arm");
    let arm = &body[at..at + 200];
    assert!(arm.contains("push_bg_op(&app, \"ocr-index\", \"ocr-index\", \"error\""), "{arm}");
}

#[test]
fn ocr_validate_path_refuses_a_network_path_on_text() {
    for p in [r"\\host\share\a.png", "//host/share/a.png", r"\\?\UNC\host\share\a.png"] {
        assert_eq!(ocr_validate_path(p), Err("unsupported image type".to_string()), "{p}");
    }
}

#[test]
fn the_import_confirm_step_spends_the_picked_file_grant_once() {
    let picked = std::env::temp_dir().join("import_grant_probe_once.json");
    let other = std::env::temp_dir().join("import_grant_probe_other.json");
    assert!(take_import_file_grant(&picked).is_err(), "never picked");
    grant_picked_dir(&picked, "importFile");
    assert!(take_import_file_grant(&other).is_err(), "another file is not covered");
    assert!(take_import_file_grant(&picked).is_ok());
    assert!(take_import_file_grant(&picked).is_err(), "a grant is spent once");
    assert!(take_import_file_grant(Path::new(r"\\host\share\x.json")).is_err());
}
