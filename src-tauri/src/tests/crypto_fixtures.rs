use super::*;

// Ciphertexts written by the previous cipher implementation and frozen here. They must keep
// opening byte for byte: a confidential row, a vault entry and two backup files (fixed key and
// password key). Key material: password "fixture-pw", vault salt "0123456789abcdef".
const VAULT_ENTRY: &str = include_str!("fixtures/vault_entry.b64");
const FIXED_KEY_PSB: &[u8] = include_bytes!("fixtures/fixed_key.psb");
const PASSWORD_PSB: &[u8] = include_bytes!("fixtures/password.psb");
const BACKUP_BODY: &str = r#"{"prompts":[{"id":"fx1","title":"Fixture","text":"backup body ä"}]}"#;
const ENTRY_TEXT: &str = "Hallo {#{Kunde}#} \u{e4}\u{20ac} fixture row";

fn fixture_key() -> [u8; 32] {
    secret_derive_key("fixture-pw", b"0123456789abcdef").expect("derive")
}

#[test]
fn a_frozen_vault_entry_still_decrypts() {
    let entry = VAULT_ENTRY.trim();
    assert_eq!(secret_decrypt(entry, &fixture_key()).unwrap(), ENTRY_TEXT);
    assert_eq!(secret_decrypt(entry, &[7u8; 32]).unwrap_err(), "wrong-password");
}

#[test]
fn a_frozen_confidential_row_still_reveals_its_text() {
    let mut p = test_prompt("fx");
    p.secret = true;
    p.text = VAULT_ENTRY.trim().to_string();
    p.variants = vec![VAULT_ENTRY.trim().to_string()];
    let (text, variants) = reveal_secret_text(&p, Some(fixture_key())).unwrap();
    // The reveal path also rewrites the old placeholder spelling.
    let shown = "Hallo {?Kunde} \u{e4}\u{20ac} fixture row";
    assert_eq!(text, shown);
    assert_eq!(variants, vec![shown.to_string()]);
    assert_eq!(reveal_secret_text(&p, None).unwrap_err(), "secret-locked");
}

#[test]
fn frozen_backup_files_still_open() {
    let fixed = backup_decrypt(FIXED_KEY_PSB, None).unwrap().expect("our format");
    assert_eq!(String::from_utf8(fixed).unwrap(), BACKUP_BODY);
    let locked = backup_decrypt(PASSWORD_PSB, Some("fixture-pw")).unwrap().expect("our format");
    assert_eq!(String::from_utf8(locked).unwrap(), BACKUP_BODY);
    assert_eq!(backup_decrypt(PASSWORD_PSB, None).unwrap_err(), "password-required");
    assert_eq!(backup_decrypt(PASSWORD_PSB, Some("nope")).unwrap_err(), "wrong password or corrupt backup");
}

#[test]
fn fresh_ciphertexts_keep_the_documented_layout_and_round_trip() {
    let key = fixture_key();
    let sealed = secret_encrypt(ENTRY_TEXT, &key);
    let raw = base64_decode(&sealed);
    // PSE1 | nonce(12) | ciphertext | tag(16)
    assert_eq!(&raw[0..4], b"PSE1");
    assert_eq!(raw.len(), 4 + 12 + ENTRY_TEXT.len() + 16);
    assert_eq!(secret_decrypt(&sealed, &key).unwrap(), ENTRY_TEXT);
    // A new nonce every time.
    assert_ne!(sealed, secret_encrypt(ENTRY_TEXT, &key));
    let empty = secret_encrypt("", &key);
    assert_eq!(secret_decrypt(&empty, &key).unwrap(), "");

    let plain = BACKUP_BODY.as_bytes();
    for pw in [None, Some("fixture-pw")] {
        let file = backup_encrypt(plain, pw).unwrap();
        assert_eq!(&file[0..4], BACKUP_MAGIC);
        assert_eq!(file[4], if pw.is_some() { 3 } else { 2 });
        let head = if pw.is_some() { 5 + 16 + 12 } else { 5 + 12 };
        assert!(file.len() > head + 16);
        assert_eq!(backup_decrypt(&file, pw).unwrap().unwrap(), plain);
    }
}

#[test]
fn tampered_or_truncated_ciphertexts_are_refused() {
    let key = fixture_key();
    // Vault entry: flip a bit in the nonce, the body and the tag; cut it short.
    let raw = base64_decode(VAULT_ENTRY.trim());
    for at in [5usize, 20, raw.len() - 1] {
        let mut bad = raw.clone();
        bad[at] ^= 1;
        assert!(secret_decrypt(&base64_encode(&bad), &key).is_err(), "byte {at}");
    }
    assert!(secret_decrypt(&base64_encode(&raw[..raw.len() - 1]), &key).is_err());
    assert!(secret_decrypt(&base64_encode(&raw[..16]), &key).is_err());
    assert!(secret_decrypt(&base64_encode(&raw[..10]), &key).is_err());
    // Backup: same for both modes.
    for (file, pw) in [(FIXED_KEY_PSB, None), (PASSWORD_PSB, Some("fixture-pw"))] {
        for at in [file.len() - 1, file.len() - 20, file.len() / 2] {
            let mut bad = file.to_vec();
            bad[at] ^= 1;
            assert!(backup_decrypt(&bad, pw).is_err(), "byte {at}");
        }
        assert!(backup_decrypt(&file[..file.len() - 1], pw).is_err());
        assert!(backup_decrypt(&file[..24], pw).is_err());
    }
}
