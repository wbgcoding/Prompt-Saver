use super::*;

#[test]
fn the_skip_list_accepts_a_pre_release_tag() {
    let mut list = Vec::new();
    assert!(add_skipped_version(&mut list, "1.2.3-beta.1".to_string()), "a pre-release tag is stored");
    assert!(add_skipped_version(&mut list, "2.0.0-rc.2".to_string()));
    assert_eq!(list, ["1.2.3-beta.1", "2.0.0-rc.2"]);
    for bad in ["beta", "-1.2.3", "1.2.3-", "1.2.3 beta", "1.2.3_beta", "1.2.3-<b>", "1.2.3-é", "1.2.3-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"] {
        assert!(!add_skipped_version(&mut list, bad.to_string()), "{bad:?} is not version-shaped");
    }
}

// The WebView2 bootstrapper gate runs only a Microsoft-signed file: a valid
// signature from anybody else must not pass.
#[test]
fn only_a_microsoft_signature_passes_the_bootstrapper_gate() {
    assert!(is_microsoft_signer("Microsoft Corporation"));
    assert!(!is_microsoft_signer("Microsoft Corporation Ltd"));
    assert!(!is_microsoft_signer("Evil Corp"));
    assert!(!is_microsoft_signer(""));
    // The unsigned test binary itself is refused.
    let me = std::env::current_exe().unwrap();
    assert!(!signed_by_microsoft(&me));
    // A real Microsoft-signed program, where this machine has one, is accepted.
    for p in [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    ] {
        if Path::new(p).exists() {
            assert!(signed_by_microsoft(Path::new(p)), "{p}");
        }
    }
}

#[test]
fn update_waits_for_a_recording_or_a_running_job() {
    assert_eq!(update_blocker(true, false, 0), Some("recording-active"));
    assert_eq!(update_blocker(false, true, 0), Some("recording-active"), "a file switch still counts as recording");
    assert_eq!(update_blocker(false, false, 2), Some("jobs-running"));
    assert_eq!(update_blocker(true, false, 2), Some("recording-active"));
    assert_eq!(update_blocker(false, false, 0), None);
}

#[test]
fn portable_update_points_at_the_release_page_of_the_offered_version() {
    let url = format!("{}/releases/download/v9.9.9/{}-setup.exe", repo_url(), brand::SLUG);
    assert_eq!(release_page_for(&url), Some(format!("{}/releases/tag/v9.9.9", repo_url())));
    assert_eq!(release_page_for("https://example.com/releases/download/v9.9.9/x-setup.exe"), None, "only our own repo");
    assert_eq!(release_page_for(&format!("{}/releases/download/../x/y.exe", repo_url())), None);
}

#[test]
fn an_update_signature_must_match_the_digest_and_the_key() {
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let pair = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    let key = base64_encode(ring::signature::KeyPair::public_key(&pair).as_ref());
    let digest = [7u8; 32];
    let sig = base64_encode(pair.sign(&digest).as_ref());
    assert!(update_signature_valid(&digest, &sig, &key));
    let mut other = digest;
    other[0] ^= 1;
    assert!(!update_signature_valid(&other, &sig, &key));
    assert!(!update_signature_valid(&digest, "", &key));
    // Garbage that is not base64 decodes to nothing and must fail closed, never verify.
    for bad in ["not base64 at all!", "####", "AAAA", "   ", "=", "\u{20ac}\u{20ac}"] {
        assert!(!update_signature_valid(&digest, bad, &key), "signature {bad:?}");
    }
    assert!(!update_signature_valid(&digest, &sig, ""), "an empty key never verifies");
    assert!(!update_signature_valid(&digest, &sig, "%%%%"), "a garbage key never verifies");
    assert!(!update_signature_valid(&digest, &sig, UPDATE_PUBLIC_KEY_B64));
}

// The helper script is a batch file: `%NAME%` expands even inside quotes, so a user folder
// named with percent signs must not run an environment lookup, and a quote cannot be escaped.
#[test]
fn update_script_escapes_percent_and_refuses_quotes() {
    let s = update_script_content(Path::new("C:\\t\\%TEMP%\\setup.exe"), Path::new("C:\\a\\x.exe")).unwrap();
    assert!(s.contains("\"C:\\t\\%%TEMP%%\\setup.exe\" /S"));
    assert!(s.contains("del \"%~f0\""));
    // The unique stage folder does not stay behind in the temp folder.
    assert!(s.trim_end().ends_with("rmdir \"C:\\t\\%%TEMP%%\""));
    assert!(update_script_content(Path::new("C:\\t\\a\"b.exe"), Path::new("C:\\a\\x.exe")).is_err());
    assert!(update_script_content(Path::new("C:\\t\\s.exe"), Path::new("C:\\a\nx.exe")).is_err());
}

// The renderer may send anything as a version to skip; only version-shaped text is stored
// and the list stays bounded.
#[test]
fn the_skip_list_takes_only_version_shaped_text_and_stays_bounded() {
    let mut list = Vec::new();
    assert!(add_skipped_version(&mut list, "1.2.3".to_string()));
    assert!(!add_skipped_version(&mut list, "1.2.3".to_string()));
    for bad in ["", "abc", "1.2.3
", &"1".repeat(33), "<script>"] {
        assert!(!add_skipped_version(&mut list, bad.to_string()), "{bad:?}");
    }
    for n in 0..40 {
        add_skipped_version(&mut list, format!("2.{n}.0"));
    }
    assert_eq!(list.len(), 20);
    assert_eq!(list.last().unwrap(), "2.39.0");
    assert!(!list.contains(&"1.2.3".to_string()));
}
