use super::*;

// ---------- Encrypted single-file backups ----------
// Format: b"PSB1" | mode(1) | [salt(16) for a password mode] | nonce(12) | AES-256-GCM.
// Modes 0/2 = fixed key (portable: any install restores it), 1/3 = user password
// (Argon2id(password + pepper, salt)); 2/3 additionally deflate the JSON before
// encrypting. Old plaintext .json backups are still read.
pub(crate) const BACKUP_MAGIC: &[u8; 4] = b"PSB1";
// Fixed obfuscation key: MUST stay identical across all versions or old fixed-key
// backups become unreadable. Not a secret (open source); it only keeps backups from
// being plainly readable while staying restorable on any machine.
pub(crate) const BACKUP_FIXED_KEY: [u8; 32] = [
    0x8f, 0x2a, 0x41, 0xd7, 0x63, 0x1c, 0xb9, 0x05, 0xe4, 0x77, 0x38, 0xaa, 0x1e, 0x90, 0x6d, 0xc2,
    0x54, 0x0b, 0xf3, 0x82, 0x9c, 0x2d, 0x71, 0x48, 0xba, 0x66, 0x0f, 0xd5, 0x37, 0xe1, 0x59, 0x84,
];
// Appended to the user's password before the KDF to harden brute force ("extend by code").
pub(crate) const BACKUP_PEPPER: &[u8] = b"prompt-saver::backup::pepper::v1";
pub(crate) const BACKUP_PW_MAX: usize = 128; // password length ceiling (bounds KDF input)
pub(crate) const BACKUP_PW_KEY: &str = "backupPw"; // ui_texts slot (DPAPI-sealed via enc_str)

pub(crate) fn backup_derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    use argon2::Argon2;
    let mut input = Vec::with_capacity(password.len() + BACKUP_PEPPER.len());
    input.extend_from_slice(password.as_bytes());
    input.extend_from_slice(BACKUP_PEPPER);
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(&input, salt, &mut key)
        .map_err(|e| e.to_string())?;
    Ok(key)
}

// A backup is JSON, so deflating it before encryption shrinks the file to a
// fraction. Compression must happen BEFORE encryption: ciphertext does not
// compress. Modes 2/3 mark a deflated payload; 0/1 stay readable forever.
pub(crate) fn deflate(raw: &[u8]) -> Result<Vec<u8>, String> {
    use flate2::{write::DeflateEncoder, Compression};
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::best());
    std::io::Write::write_all(&mut enc, raw).map_err(|e| e.to_string())?;
    enc.finish().map_err(|e| e.to_string())
}

// Ceiling for anything we decompress. The default backup key ships inside the exe, so
// a valid-looking .psb is something anyone can craft: without a cap a few KB of
// deflate expands until the process is killed. Far above any real library.
pub(crate) const MAX_INFLATED: u64 = 512 * 1024 * 1024;

// Pure predicate, kept apart from do_backup's own AppHandle/filesystem side
// effects so the size threshold itself is directly unit-testable.
pub(crate) fn too_large_to_restore(len: u64) -> bool {
    len > MAX_INFLATED
}

// Decompress at most MAX_INFLATED bytes, refusing anything larger instead of growing
// the buffer until the allocator gives up.
pub(crate) fn inflate_capped<R: std::io::Read>(reader: R) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut out = Vec::new();
    // cap + 1 byte: reaching the extra byte is what proves the stream was too big.
    reader
        .take(MAX_INFLATED + 1)
        .read_to_end(&mut out)
        .map_err(|_| "corrupt backup".to_string())?;
    if out.len() as u64 > MAX_INFLATED {
        return Err("backup expands to more than 512 MB".to_string());
    }
    Ok(out)
}

pub(crate) fn inflate(packed: &[u8]) -> Result<Vec<u8>, String> {
    use flate2::read::DeflateDecoder;
    inflate_capped(DeflateDecoder::new(packed))
}

pub(crate) fn backup_encrypt(plain: &[u8], password: Option<&str>) -> Result<Vec<u8>, String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Key, Nonce};
    use rand::RngCore;
    let body = deflate(plain)?;
    let mut out = Vec::with_capacity(body.len() + 64);
    out.extend_from_slice(BACKUP_MAGIC);
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let key_bytes: [u8; 32] = match password {
        Some(pw) => {
            out.push(3); // password + deflate
            let mut salt = [0u8; 16];
            rand::rngs::OsRng.fill_bytes(&mut salt);
            let k = backup_derive_key(pw, &salt)?;
            out.extend_from_slice(&salt);
            k
        }
        None => {
            out.push(2); // fixed key + deflate
            BACKUP_FIXED_KEY
        }
    };
    out.extend_from_slice(&nonce);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), body.as_slice())
        .map_err(|_| "backup encryption failed".to_string())?;
    out.extend_from_slice(&ct);
    Ok(out)
}

// Ok(Some(json)) = decrypted; Ok(None) = not our format (caller tries plaintext JSON);
// Err("password-required") = a password-mode backup and no/failed key.
pub(crate) fn backup_decrypt(data: &[u8], password: Option<&str>) -> Result<Option<Vec<u8>>, String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Key, Nonce};
    if data.len() < 5 || &data[0..4] != BACKUP_MAGIC {
        return Ok(None);
    }
    // 0/1 = uncompressed (pre-2.6.0 files), 2/3 = deflated payload.
    let packed = matches!(data[4], 2 | 3);
    let (key_bytes, rest): ([u8; 32], &[u8]) = match data[4] {
        0 | 2 => (BACKUP_FIXED_KEY, &data[5..]),
        1 | 3 => {
            if data.len() < 21 {
                return Err("corrupt backup".to_string());
            }
            let pw = password.ok_or_else(|| "password-required".to_string())?;
            (backup_derive_key(pw, &data[5..21])?, &data[21..])
        }
        _ => return Err("unknown backup format".to_string()),
    };
    if rest.len() < 12 {
        return Err("corrupt backup".to_string());
    }
    let (nonce, ct) = rest.split_at(12);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let body = cipher
        .decrypt(Nonce::from_slice(nonce), ct)
        .map_err(|_| {
            if matches!(data[4], 0 | 2) {
                // A fixed-key backup has no password to get wrong: asking for one
                // would send the user after something that cannot help.
                "corrupt backup".to_string()
            } else {
                "wrong password or corrupt backup".to_string()
            }
        })?;
    Ok(Some(if packed { inflate(&body)? } else { body }))
}

// ---------- Confidential entries (per-prompt encryption) ----------
// Same derivation and cipher as the encrypted backups above (Argon2id + AES-256-GCM),
// with its own pepper, salt and verification value: this feature has nothing to do
// with a backup password and must not accept one as a substitute. Format: b"PSE1" |
// nonce(12) | AES-256-GCM ciphertext, base64. No compression: a prompt's text is far
// too small for it to matter, and it would only add a length side channel.
pub(crate) const SECRET_MAGIC: &[u8; 4] = b"PSE1";
pub(crate) const SECRET_PEPPER: &[u8] = b"prompt-saver::secret::pepper::v1";
pub(crate) const SECRET_PW_MAX: usize = 128;
// ui_texts slots: the vault's own salt and a ciphertext of a known marker, used to
// check a password before trusting it: never the password itself, and never
// DPAPI-sealed on top, since neither value is a secret on its own.
pub(crate) const SECRET_SALT_KEY: &str = "secretVaultSalt";
pub(crate) const SECRET_VERIFY_KEY: &str = "secretVaultVerify";
pub(crate) const SECRET_VERIFY_MARKER: &str = "prompt-saver-secret-vault-ok";

pub(crate) fn secret_derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    use argon2::Argon2;
    let mut input = Vec::with_capacity(password.len() + SECRET_PEPPER.len());
    input.extend_from_slice(password.as_bytes());
    input.extend_from_slice(SECRET_PEPPER);
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(&input, salt, &mut key)
        .map_err(|e| e.to_string())?;
    Ok(key)
}

pub(crate) fn secret_encrypt(plain: &str, key: &[u8; 32]) -> String {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Key, Nonce};
    use rand::RngCore;
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), plain.as_bytes())
        .unwrap_or_default();
    let mut out = Vec::with_capacity(SECRET_MAGIC.len() + nonce.len() + ct.len());
    out.extend_from_slice(SECRET_MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    base64_encode(&out)
}

// Wrong password and corrupt ciphertext fail the exact same way (AEAD tag
// mismatch): there is no separate "wrong password" code path to get subtly
// wrong, the cipher itself is the check.
pub(crate) fn secret_decrypt(stored: &str, key: &[u8; 32]) -> Result<String, String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Key, Nonce};
    let raw = base64_decode(stored);
    if raw.len() < SECRET_MAGIC.len() + 12 || &raw[0..4] != SECRET_MAGIC {
        return Err("corrupt-secret".to_string());
    }
    let nonce = &raw[4..16];
    let ct = &raw[16..];
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let pt = cipher
        .decrypt(Nonce::from_slice(nonce), ct)
        .map_err(|_| "wrong-password".to_string())?;
    String::from_utf8(pt).map_err(|_| "corrupt-secret".to_string())
}

// Resolves a prompt's copyable text and variants: passed through unchanged for
// a plain prompt, decrypted with the session key for a confidential one, or
// refused outright with "secret-locked" when the vault has no key right now.
// This is the one place a confidential entry's real text is allowed to exist
// in memory again -- every copy, peek and rotation-changed path goes through
// it instead of reading Prompt.text/variants directly, so none of them can
// hand ciphertext to the clipboard or an event payload by mistake.
pub(crate) fn reveal_secret_text(p: &Prompt, key: Option<[u8; 32]>) -> Result<(String, Vec<String>), String> {
    if !p.secret {
        return Ok((p.text.clone(), p.variants.clone()));
    }
    let Some(key) = key else {
        return Err("secret-locked".to_string());
    };
    // Ciphertext cannot be rewritten at load, so the old placeholder spelling inside
    // a confidential entry is upgraded here, where it is read back; the next save
    // stores it that way.
    let text = upgrade_placeholders(&secret_decrypt(&p.text, &key)?);
    let mut variants = Vec::with_capacity(p.variants.len());
    for v in &p.variants {
        variants.push(upgrade_placeholders(&secret_decrypt(v, &key)?));
    }
    Ok((text, variants))
}

// A version-history snapshot taken before an entry became confidential holds
// plain text; one taken afterwards holds ciphertext already readable with the
// current vault key. Used when restoring a snapshot into an entry that is
// confidential right now: keeps an already-encrypted value as-is instead of
// encrypting it a second time, and encrypts a plain one so it never lands on
// disk unencrypted under a confidential entry.
pub(crate) fn secret_encrypt_if_plain(value: &str, key: &[u8; 32]) -> String {
    if secret_decrypt(value, key).is_ok() {
        value.to_string()
    } else {
        secret_encrypt(value, key)
    }
}

// One derived key for the whole running session, forgotten on quit (never
// written anywhere) and on an idle timeout (secretTimeoutMin, 15 minutes by
// default, 0 = only on quit). Every confidential entry shares this one key; nothing about it is
// per-entry, matching "own salt, own verification value" for the FEATURE, not
// for each prompt.
pub(crate) struct SecretSession {
    pub(crate) key: [u8; 32],
    pub(crate) unlocked_at: std::time::Instant,
}
pub(crate) static SECRET_SESSION: Mutex<Option<SecretSession>> = Mutex::new(None);

pub(crate) const SECRET_TIMEOUT_DEFAULT_MIN: f64 = 15.0;

pub(crate) fn secret_timeout_min(settings: &Settings) -> u32 {
    settings
        .ui_values
        .get("secretTimeoutMin")
        .copied()
        .unwrap_or(SECRET_TIMEOUT_DEFAULT_MIN)
        .clamp(0.0, 240.0) as u32
}

// None means locked: no session yet, or the idle timeout already elapsed. A
// timeout of 0 means "until quit" and is never checked here.
pub(crate) fn secret_session_key(timeout_min: u32) -> Option<[u8; 32]> {
    let mut guard = SECRET_SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(sess) = guard.as_ref() {
        if timeout_min > 0 && sess.unlocked_at.elapsed().as_secs() >= (timeout_min as u64) * 60 {
            *guard = None;
            return None;
        }
        return Some(sess.key);
    }
    None
}

// Seconds until the idle timeout locks the session again. None = it never does
// (timeout 0, until quit). Pure, so the arithmetic can be tested without a clock.
pub(crate) fn secret_relock_in_secs(timeout_min: u32, elapsed_secs: u64) -> Option<u64> {
    (timeout_min > 0).then(|| (timeout_min as u64 * 60).saturating_sub(elapsed_secs))
}

#[derive(Serialize)]
pub(crate) struct SecretVaultStatus {
    pub(crate) configured: bool,
    pub(crate) unlocked: bool,
    // While unlocked with a timeout: how long until the key is dropped. The window
    // schedules a refetch for that moment, since the backend only notices the
    // expiry when it is next asked and nothing else tells the window.
    pub(crate) remaining_secs: Option<u64>,
}

#[tauri::command(async)]
pub(crate) fn secret_vault_status(state: State<Db>) -> SecretVaultStatus {
    let store = lock(&state);
    let configured = !store
        .settings
        .ui_texts
        .get(SECRET_SALT_KEY)
        .map(|s| s.is_empty())
        .unwrap_or(true);
    let timeout = secret_timeout_min(&store.settings);
    let unlocked = secret_session_key(timeout).is_some();
    let remaining_secs = if unlocked {
        let guard = SECRET_SESSION.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().and_then(|s| secret_relock_in_secs(timeout, s.unlocked_at.elapsed().as_secs()))
    } else {
        None
    };
    SecretVaultStatus { configured, unlocked, remaining_secs }
}

// Verifies a password against the stored salt and verifier and returns the key.
pub(crate) fn secret_vault_verify(password: &str, salt_b64: &str, verify_b64: &str) -> Result<[u8; 32], String> {
    let salt = base64_decode(salt_b64);
    if salt.is_empty() {
        return Err("corrupt-vault".to_string());
    }
    let key = secret_derive_key(password, &salt)?;
    secret_decrypt(verify_b64, &key).map_err(|_| "wrong-password".to_string())?;
    Ok(key)
}

// First-time vault setup: stores the salt and verifier and reports whether they
// reached disk. Ok(false) means metadata already exists and nothing was touched
// (never replace an existing salt: it would orphan every ciphertext). On a failed
// save both slots return to what they held, so no unlocked state can ever rest on
// a salt that is not durable.
pub(crate) fn secret_vault_first_setup(
    settings: &mut Settings,
    salt: &[u8],
    verify: String,
    mut save: impl FnMut(&Settings) -> bool,
) -> Result<bool, String> {
    if settings.ui_texts.get(SECRET_SALT_KEY).map(|s| !s.is_empty()).unwrap_or(false) {
        return Ok(false);
    }
    let old_salt = settings.ui_texts.get(SECRET_SALT_KEY).cloned();
    let old_verify = settings.ui_texts.get(SECRET_VERIFY_KEY).cloned();
    settings.ui_texts.insert(SECRET_SALT_KEY.to_string(), base64_encode(salt));
    settings.ui_texts.insert(SECRET_VERIFY_KEY.to_string(), verify);
    if !save(settings) {
        for (k, old) in [(SECRET_SALT_KEY, old_salt), (SECRET_VERIFY_KEY, old_verify)] {
            match old {
                Some(v) => { settings.ui_texts.insert(k.to_string(), v); }
                None => { settings.ui_texts.remove(k); }
            }
        }
        return Err("save-failed".to_string());
    }
    Ok(true)
}

// Converts a copy of one prompt to the requested confidential state, every field
// authenticated or encrypted before anything is returned. None = already in that
// state. The live prompt is never touched here.
pub(crate) fn secret_convert_prompt(p: &Prompt, secret: bool, key: &[u8; 32]) -> Result<Option<Prompt>, String> {
    if secret == p.secret {
        return Ok(None);
    }
    let mut out = p.clone();
    if secret {
        out.text = secret_encrypt(&p.text, key);
        out.variants = p.variants.iter().map(|v| secret_encrypt(v, key)).collect();
        out.secret = true;
    } else {
        let (text, variants) = reveal_secret_text(p, Some(*key))?;
        out.text = text;
        out.variants = variants;
        out.secret = false;
    }
    Ok(Some(out))
}

// Swaps the converted prompt in, persists, and puts the original back when the
// save fails, so a failed conversion can never ride along on a later save.
pub(crate) fn apply_prompt_secret(
    store: &mut Store,
    id: &str,
    secret: bool,
    key: &[u8; 32],
    save: impl FnOnce(&Store) -> bool,
) -> Result<(), String> {
    let Some(idx) = store.prompts.iter().position(|p| p.id == id) else {
        return Err("not-found".to_string());
    };
    let converted = secret_convert_prompt(&store.prompts[idx], secret, key)?;
    let original = converted.map(|new| std::mem::replace(&mut store.prompts[idx], new));
    if !save(store) {
        if let Some(orig) = original {
            store.prompts[idx] = orig;
        }
        return Err("save-failed".to_string());
    }
    Ok(())
}

// First call ever (no salt stored yet) sets the password up as the vault's
// password. Every later call verifies against the stored salt/marker instead:
// there is no way to change it short of turning secretPrompts off, which is a
// deliberate consequence of "forgetting it means total loss", never a shortcut
// around that promise.
#[tauri::command]
pub(crate) async fn secret_vault_unlock(app: AppHandle, state: State<'_, Db>, password: String) -> Result<AppState, String> {
    if password.is_empty() || password.len() > SECRET_PW_MAX {
        return Err("invalid-password".to_string());
    }
    let read_meta = || {
        let store = lock(&state);
        (
            store.settings.ui_texts.get(SECRET_SALT_KEY).cloned().unwrap_or_default(),
            store.settings.ui_texts.get(SECRET_VERIFY_KEY).cloned().unwrap_or_default(),
        )
    };
    let (mut salt_b64, mut verify_b64) = read_meta();
    let mut fresh_key: Option<[u8; 32]> = None;
    if salt_b64.is_empty() {
        use rand::RngCore;
        let mut salt = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        let key = secret_derive_key(&password, &salt)?;
        let verify = secret_encrypt(SECRET_VERIFY_MARKER, &key);
        let created = {
            let mut store = lock(&state);
            secret_vault_first_setup(&mut store.settings, &salt, verify, |s| save_settings(&app, s))?
        };
        if created {
            fresh_key = Some(key);
        } else {
            // Another call set the vault up first: verify against what it stored
            // instead of replacing its salt.
            (salt_b64, verify_b64) = read_meta();
        }
    }
    let key = match fresh_key {
        Some(k) => k,
        None => secret_vault_verify(&password, &salt_b64, &verify_b64)?,
    };
    *SECRET_SESSION.lock().unwrap_or_else(|e| e.into_inner()) = Some(SecretSession {
        key,
        unlocked_at: std::time::Instant::now(),
    });
    let store = lock(&state);
    let secret_key = secret_session_key(secret_timeout_min(&store.settings));
    Ok(AppState {
        prompts: slim_prompts(&store.prompts, secret_key),
        settings: webview_settings(&store.settings),
    })
}

// Turns one prompt's confidential flag on or off, encrypting/decrypting its text
// and every variant with the current session key. Requires an unlocked session
// either way -- turning it OFF still needs the key to recover the plain text, so
// there is no path that leaves a prompt's `secret` flag lying about what its own
// text field holds.
#[tauri::command]
pub(crate) async fn set_prompt_secret(app: AppHandle, state: State<'_, Db>, id: String, secret: bool) -> Result<AppState, String> {
    let timeout = secret_timeout_min(&lock(&state).settings);
    let Some(key) = secret_session_key(timeout) else {
        return Err("locked".to_string());
    };
    let mut store = lock(&state);
    apply_prompt_secret(&mut store, &id, secret, &key, |st| save_prompts(&app, st))?;
    if secret {
        // A draft written before the entry turned confidential holds its plain text.
        if let Some(conn) = db_conn(&app) {
            discard_editor_draft_if(&conn, |t| t == id);
        }
    }
    let secret_key = secret_session_key(timeout);
    Ok(AppState {
        prompts: slim_prompts(&store.prompts, secret_key),
        settings: webview_settings(&store.settings),
    })
}

// Store (DPAPI-sealed) or clear the optional backup password. Never returned in clear.
#[tauri::command]
pub(crate) async fn set_backup_password(app: AppHandle, state: State<'_, Db>, password: String) -> Result<(), String> {
    if password.len() > BACKUP_PW_MAX {
        return Err("password too long".to_string());
    }
    {
        let mut store = lock(&state);
        let original = store.settings.ui_texts.get(BACKUP_PW_KEY).cloned();
        if password.is_empty() {
            store.settings.ui_texts.remove(BACKUP_PW_KEY);
        } else {
            store
                .settings
                .ui_texts
                .insert(BACKUP_PW_KEY.to_string(), enc_str(&password));
        }
        // A failed save must not report the new backup password as set: the
        // very next export would otherwise silently use the OLD (or no) password
        // while the UI believes it changed.
        if !save_settings(&app, &store.settings) {
            match original {
                Some(v) => { store.settings.ui_texts.insert(BACKUP_PW_KEY.to_string(), v); }
                None => { store.settings.ui_texts.remove(BACKUP_PW_KEY); }
            }
            return Err("save-failed".into());
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn has_backup_password(state: State<'_, Db>) -> Result<bool, String> {
    let store = lock(&state);
    Ok(store
        .settings
        .ui_texts
        .get(BACKUP_PW_KEY)
        .map(|v| !v.is_empty())
        .unwrap_or(false))
}

// The clear backup password for this machine (sealed → dec_str), if one is set.
pub(crate) fn stored_backup_password(store: &Store) -> Option<String> {
    store
        .settings
        .ui_texts
        .get(BACKUP_PW_KEY)
        .map(|v| dec_str(v))
        .filter(|s| !s.is_empty())
}

#[tauri::command]
pub(crate) async fn export_all(app: AppHandle, state: State<'_, Db>) -> Result<usize, String> {
    let (json, count, password) = {
        let store = lock(&state);
        // The export itself takes the live block, so it is right either way: but
        // an export is also the moment the store on disk should agree with it.
        flush_settings(&app, &store.settings);
        let mut backup_settings = store.settings.clone();
        remote_tls::strip_tls_keys(&mut backup_settings.ui_texts);
        let backup = Backup {
            prompts: store.prompts.clone(),
            settings: backup_settings,
        };
        let json = serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())?;
        (json, store.prompts.len(), stored_backup_password(&store))
    };
    // Always encrypted: fixed key by default (restorable anywhere), user password if set.
    let blob = backup_encrypt(json.as_bytes(), password.as_deref())?;
    let file = file_dialog(&app)
        .set_file_name(format!("{}-backup.psb", brand::SLUG))
        .add_filter(format!("{} backup", brand::DISPLAY_NAME), &["psb", "json"])
        .save_file();
    match file {
        Some(path) => {
            write_bytes_atomic(&path, &blob).map_err(|e| e.to_string())?;
            Ok(count)
        }
        None => Err("canceled".to_string()),
    }
}

// What a full import restored: the UI names the file in its confirmation.
#[derive(Serialize)]
pub(crate) struct ImportSummary {
    pub(crate) name: String,
    pub(crate) count: usize,
    // Prompt hotkeys the restore had to give up (unusable or already bound).
    pub(crate) dropped_hotkeys: usize,
    // The file carried its own (foreign-machine, undecryptable-here) backup
    // password, which was discarded in favour of this machine's own.
    pub(crate) backup_pw_dropped: bool,
    // Confidential entries in the trash were sealed with a vault key that the
    // imported prompt set replaced.
    pub(crate) vault_mismatch: bool,
}

#[tauri::command]
pub(crate) async fn pick_backup_file(app: AppHandle) -> Option<String> {
    file_dialog(&app)
        .add_filter(format!("{} backup", brand::DISPLAY_NAME), &["psb", "json"])
        .pick_file()
        .map(|p| p.display().to_string())
}

// Settings-only export/import: a plain, unencrypted JSON of just the expert-menu
// state -- never prompts, never hotkey bindings, the same "settings vs data"
// split the app's own prompt-library export already draws. Deliberately a
// separate, smaller pair of commands rather than export_all/import_all's
// "settings" scope, which carries the WHOLE Settings struct (window
// geometry, the hotkey, ...).
pub(crate) const MAX_SETTINGS_IMPORT_BYTES: u64 = 2 * 1024 * 1024;

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub(crate) struct SettingsOnlyExport {
    #[serde(default)]
    pub(crate) ui_flags: HashMap<String, bool>,
    #[serde(default)]
    pub(crate) ui_values: HashMap<String, f64>,
    #[serde(default)]
    pub(crate) ui_texts: HashMap<String, String>,
}

// The settings-only export copy. Everything an import would skip is left out
// of the file too: the receiver's password hash and certificate, the sealed
// backup password, network and machine switches. The vault's salt and
// verifier stay in (not secrets).
pub(crate) fn settings_only_payload(settings: &Settings) -> SettingsOnlyExport {
    let mut payload = SettingsOnlyExport {
        ui_flags: settings.ui_flags.clone(),
        ui_values: settings.ui_values.clone(),
        ui_texts: settings.ui_texts.clone(),
    };
    payload.ui_flags.retain(|k, _| !is_import_denied_flag(k));
    payload.ui_values.retain(|k, _| !is_import_denied_value(k));
    payload.ui_texts.retain(|k, _| !is_machine_text_key(k));
    payload
}

#[tauri::command]
pub(crate) async fn export_settings_only(app: AppHandle, state: State<'_, Db>) -> Result<String, String> {
    let payload = settings_only_payload(&lock(&state).settings);
    let json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    let file = file_dialog(&app)
        .set_file_name(format!("{}-settings.json", brand::SLUG))
        .add_filter("JSON", &["json"])
        .save_file();
    match file {
        Some(path) => {
            write_bytes_atomic(&path, json.as_bytes()).map_err(|e| e.to_string())?;
            Ok(path.display().to_string())
        }
        None => Err("canceled".to_string()),
    }
}

#[tauri::command]
pub(crate) async fn pick_settings_import_file(app: AppHandle) -> Option<String> {
    file_dialog(&app).add_filter("JSON", &["json"]).pick_file().map(|p| p.display().to_string())
}

pub(crate) fn parse_settings_only(raw: &[u8]) -> Result<SettingsOnlyExport, String> {
    let text = std::str::from_utf8(raw).map_err(|_| "bad-file".to_string())?;
    // A wholly empty or non-object file (no ui_flags/ui_values/ui_texts key at
    // all) is rejected too -- this ONLY accepts a file this same export produced,
    // never an unrelated JSON document that would otherwise silently parse into
    // three empty maps and "successfully" change nothing.
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| "bad-file".to_string())?;
    if !value.is_object()
        || !(value.get("ui_flags").is_some() || value.get("ui_values").is_some() || value.get("ui_texts").is_some())
    {
        return Err("bad-file".to_string());
    }
    serde_json::from_value(value).map_err(|_| "bad-file".to_string())
}

// Counts how many of the file's own rows would actually change against the
// live store, without writing anything -- what the import preview shows before
// the user confirms.
pub(crate) fn settings_import_diff(current: &Settings, incoming: &SettingsOnlyExport) -> (usize, usize) {
    let mut changed = 0;
    let total = incoming.ui_flags.len() + incoming.ui_values.len() + incoming.ui_texts.len();
    for (k, v) in &incoming.ui_flags {
        if current.ui_flags.get(k) != Some(v) {
            changed += 1;
        }
    }
    for (k, v) in &incoming.ui_values {
        if current.ui_values.get(k) != Some(v) {
            changed += 1;
        }
    }
    for (k, v) in &incoming.ui_texts {
        if current.ui_texts.get(k) != Some(v) {
            changed += 1;
        }
    }
    (changed, total)
}

// Settings no imported file may set, whether a settings-only file or a whole
// backup: network exposure, credentials and the machine's own integrations. A
// file is untrusted (it can be a hostile one the user was talked into picking),
// and each of these has its own dedicated command with its own validation, so
// the value this machine already has is the one that stays. The receiver's
// password hash and certificate values come in through the same helper the
// generic setters use; every other `remote*` row is covered by the prefix, so
// a receiver setting added later is denied without touching this list.
pub(crate) const IMPORT_DENIED_FLAGS: [&str; 12] = [
    "clipWatcher",
    "remoteApi",
    "remoteNetworkFilter",
    "urlScheme",
    "urlSchemeOpenAllowed",
    "urlSchemeCopyAllowed",
    "demoMode",
    "allowMultipleInstances",
    "promptSync",
    // Consent and lock switches: a file never grants the online speech consent,
    // lifts the data-folder lock or turns off secret skipping for the clipboard.
    "voiceOnlineOk",
    "dataDirLock",
    "clipSkipSecrets",
];

pub(crate) fn is_import_denied_flag(key: &str) -> bool {
    key.starts_with("remote") || IMPORT_DENIED_FLAGS.contains(&key)
}

pub(crate) fn is_import_denied_value(key: &str) -> bool {
    key.starts_with("remote")
}

// Text rows that belong to this machine: the receiver's password hash,
// allowed ranges and certificate, the sealed backup password and the folder
// authority keys (see PROTECTED_FOLDER_KEYS).
pub(crate) fn is_machine_text_key(key: &str) -> bool {
    remote::is_remote_secret_key(key) || key.starts_with("remote") || is_protected_folder_key(key) || key == BACKUP_PW_KEY
}

// The confidential-prompt vault's salt and verifier. Not secrets, but the
// prompts on this machine are sealed with a key derived from THIS salt, so a
// foreign one would make every confidential prompt unreadable.
pub(crate) fn is_vault_key(key: &str) -> bool {
    key == SECRET_SALT_KEY || key == SECRET_VERIFY_KEY
}

// Copies the vault salt and verifier of `from` into `to`, overwriting what `to`
// has. A `from` without them leaves `to` untouched.
pub(crate) fn carry_vault_keys(from: &Settings, to: &mut Settings) {
    for key in [SECRET_SALT_KEY, SECRET_VERIFY_KEY] {
        if let Some(v) = from.ui_texts.get(key) {
            to.ui_texts.insert(key.to_string(), v.clone());
        }
    }
}

pub(crate) fn clear_vault_keys(settings: &mut Settings) {
    settings.ui_texts.remove(SECRET_SALT_KEY);
    settings.ui_texts.remove(SECRET_VERIFY_KEY);
}

pub(crate) fn has_vault_keys(settings: &Settings) -> bool {
    settings.ui_texts.contains_key(SECRET_SALT_KEY) || settings.ui_texts.contains_key(SECRET_VERIFY_KEY)
}

pub(crate) fn vault_keys_differ(a: &Settings, b: &Settings) -> bool {
    [SECRET_SALT_KEY, SECRET_VERIFY_KEY].iter().any(|k| a.ui_texts.get(*k) != b.ui_texts.get(*k))
}

// Removes every denied row from a settings block that came from a file.
pub(crate) fn drop_machine_rows(incoming: &mut Settings) {
    incoming.ui_flags.retain(|k, _| !is_import_denied_flag(k));
    incoming.ui_values.retain(|k, _| !is_import_denied_value(k));
    incoming.ui_texts.retain(|k, _| !is_machine_text_key(k));
    // A settings block from another machine never carries its update consent.
    incoming.update_consent = UpdateConsent::Unasked;
}

// After a whole Settings block was replaced by an imported one: every denied
// row goes back to what `local` had (or stays absent), plus the autostart
// registration flag. Hotkeys, themes and layouts do come along.
pub(crate) fn keep_machine_settings(local: &Settings, incoming: &mut Settings) {
    drop_machine_rows(incoming);
    for (k, v) in &local.ui_flags {
        if is_import_denied_flag(k) {
            incoming.ui_flags.insert(k.clone(), *v);
        }
    }
    for (k, v) in &local.ui_values {
        if is_import_denied_value(k) {
            incoming.ui_values.insert(k.clone(), *v);
        }
    }
    for (k, v) in &local.ui_texts {
        if is_machine_text_key(k) {
            incoming.ui_texts.insert(k.clone(), v.clone());
        }
    }
    // Machine-level behaviour that is not an expert row: the registry autostart
    // entry and the update choices made on this computer.
    incoming.autostart = local.autostart;
    incoming.auto_update = local.auto_update;
    incoming.update_consent = local.update_consent;
    incoming.skipped_versions = local.skipped_versions.clone();
}

// Drops every denied row from an incoming settings file and returns how many
// the user would have expected to be applied. Called on every incoming file
// before it is diffed or applied, so preview and apply always agree on what
// would change. The vault rows are dropped too but not counted: they are never
// something the user chose.
pub(crate) fn strip_denied_import_keys(incoming: &mut SettingsOnlyExport) -> usize {
    let count = |i: &SettingsOnlyExport| i.ui_flags.len() + i.ui_values.len() + i.ui_texts.len();
    incoming.ui_texts.retain(|k, _| !is_vault_key(k));
    let before = count(incoming);
    incoming.ui_flags.retain(|k, _| !is_import_denied_flag(k));
    incoming.ui_values.retain(|k, _| !is_import_denied_value(k));
    incoming.ui_texts.retain(|k, _| !is_machine_text_key(k));
    before - count(incoming)
}

#[tauri::command]
pub(crate) async fn preview_settings_import(state: State<'_, Db>, path: String) -> Result<(usize, usize, usize), String> {
    let raw = read_file_capped(&PathBuf::from(path), MAX_SETTINGS_IMPORT_BYTES)?;
    let mut incoming = parse_settings_only(&raw)?;
    let skipped = strip_denied_import_keys(&mut incoming);
    let store = lock(&state);
    let (changed, total) = settings_import_diff(&store.settings, &incoming);
    Ok((changed, total, skipped))
}

// Merges the file's rows into `settings` and persists through `persist`. A
// failed save puts the previous settings back and reports save-failed, so a
// settings block that is not on disk is never left active.
pub(crate) fn merge_settings_only(
    settings: &mut Settings,
    incoming: SettingsOnlyExport,
    persist: impl FnOnce(&Settings) -> bool,
) -> Result<usize, String> {
    let (changed, _) = settings_import_diff(settings, &incoming);
    let original = settings.clone();
    // Merge, not replace: a row this file never mentions (e.g. a newer
    // setting an older export predates) is left exactly as it is.
    for (k, v) in incoming.ui_flags {
        settings.ui_flags.insert(k, v);
    }
    for (k, v) in incoming.ui_values {
        settings.ui_values.insert(k, v);
    }
    for (k, v) in incoming.ui_texts {
        settings.ui_texts.insert(k, v);
    }
    clamp_ui_values(&mut settings.ui_values);
    // The prompts stay, so their vault keys stay.
    carry_vault_keys(&original, settings);
    if !persist(settings) {
        *settings = original;
        return Err("save-failed".to_string());
    }
    Ok(changed)
}

// What every import re-applies without a restart, for the settings that just
// landed: theme, screenshot exclusion, global hotkeys, tray, the pill windows
// and the error log's size cap.
pub(crate) fn refresh_after_import(app: &AppHandle, settings: &Settings) {
    push_theme(app, settings);
    apply_capture_exclusion_all(app, capture_excluded(settings));
    rearm_hotkeys(app);
    apply_tray_menu(app);
    sync_float_windows(app);
    if let Some(kb) = settings.ui_values.get("errorLogKeepKb").copied() {
        ERROR_LOG_KEEP_BYTES.store((kb.max(1.0) * 1024.0) as u64, Ordering::Relaxed);
    }
    notify_float_settings(app);
}

#[tauri::command]
pub(crate) async fn apply_settings_import(app: AppHandle, state: State<'_, Db>, path: String) -> Result<usize, String> {
    let raw = read_file_capped(&PathBuf::from(path), MAX_SETTINGS_IMPORT_BYTES)?;
    let mut incoming = parse_settings_only(&raw)?;
    strip_denied_import_keys(&mut incoming);
    let (changed, settings) = {
        let mut store = lock(&state);
        let ocr_was_on = ocr_index_flag_on(&store);
        let changed = merge_settings_only(&mut store.settings, incoming, |s| save_settings(&app, s))?;
        if clear_ocr_text_if_switched_off(ocr_was_on, &mut store) {
            save_prompts_and_trash(&app, &store);
        }
        (changed, store.settings.clone())
    };
    refresh_after_import(&app, &settings);
    Ok(changed)
}

// Pure scope-branching core of import_all -- split out so it is unit-testable
// without an AppHandle/State (the async command below owns the save/rollback/
// AppHandle-dependent side of the contract). Returns (prompt count, dropped
// hotkeys, whether the backup's own backup-password was dropped in favour of
// this machine's).
pub(crate) fn apply_import(store: &mut Store, backup: Backup, want_data: bool, want_settings: bool) -> (usize, usize, bool) {
    if want_data {
        store.prompts = backup.prompts;
        // A backup written by hand could miss ids; keep every prompt addressable.
        for p in store.prompts.iter_mut() {
            if p.id.is_empty() {
                p.id = gen_id();
            }
        }
        migrate_prompts(&mut store.prompts);
        store.prompts_locked = false; // a full import supersedes any decrypt lock
        // Deliberately NOT forget_written_rows() here: write_rows matches an
        // imported row against the disk by (id, content hash), never by trusting
        // `written` alone, so a reused id with different content is still
        // rewritten correctly. But write_rows ALSO only deletes a row that has
        // disappeared from the new set when THIS process already knew about it
        // (present in `written`) -- clearing the map here would silently make
        // every pre-import prompt un-deletable, since a wholesale replacement
        // with a smaller set would leave every old row orphaned on disk, back
        // after the next restart. The prompts this process already loaded are
        // exactly the ones `written` must keep so their removal still lands.
    }
    // The imported backupPw is DPAPI-sealed on a DIFFERENT machine: it
    // decrypts to "" here, and a following export_all would then silently write
    // UNPROTECTED with the fixed key, with nothing telling the user their
    // password stopped applying. Keep THIS machine's own password across the
    // import instead (reset_settings already does the identical rescue for the
    // identical reason). Only meaningful once settings are actually touched.
    let backup_pw_dropped = want_settings && backup.settings.ui_texts.contains_key(BACKUP_PW_KEY);
    // Vault keys follow the prompt set: confidential prompts are sealed with a
    // key derived from the salt that was in the settings they were saved with.
    let mut backup_vault = Settings::default();
    carry_vault_keys(&backup.settings, &mut backup_vault);
    if want_settings {
        // A full backup can come from another machine, or be a hostile file the
        // user was talked into restoring: network exposure, credentials, folder
        // authority and OS registrations (see IMPORT_DENIED_FLAGS) stay as this
        // machine has them.
        let local = std::mem::replace(&mut store.settings, backup.settings);
        // migrate() fills defaults and guarantees at least one view: without it a
        // hand-edited backup with "views": [] makes active_view_mut index [0] panic
        // on the next mutation (and poisons the Db mutex).
        store.settings.migrate();
        keep_machine_settings(&local, &mut store.settings);
        // The backup's own vault keys belong to its own prompts: they travel
        // with them, and a backup without any keeps the local ones. With the
        // current prompts staying, the local ones stay whatever the file says.
        if !want_data {
            clear_vault_keys(&mut store.settings);
        }
        if !want_data || !has_vault_keys(&backup_vault) {
            carry_vault_keys(&local, &mut store.settings);
        }
        load_pick_history(&store.settings); // restore brings its own history mirror
    }
    if want_data && !want_settings {
        carry_vault_keys(&backup_vault, &mut store.settings);
    }
    // A colliding or unusable accelerator must not fail the restore: drop
    // it and let the summary say so. Runs regardless of scope: an imported
    // app-level hotkey can collide with untouched prompts, and imported prompts
    // can collide with an untouched app-level hotkey.
    let app_hotkey = store.settings.hotkey.clone();
    let dropped = drop_colliding_hotkeys(&app_hotkey, &mut store.prompts);
    // Folders index into prompts, so only re-normalize when prompts actually
    // changed -- a settings-only import must not touch the current grid.
    if want_data {
        store.normalize_folders();
    }
    (store.prompts.len(), dropped, backup_pw_dropped)
}

// Applies an import to the in-memory store and persists it through `persist`,
// which gets the store and whether the prompt rows and the settings row have
// to be written. A failed write puts the ENTIRE previous state back (prompts,
// settings and both lock flags), so memory never holds an import the disk does
// not, and the caller reports a failure only for an import that is not active.
pub(crate) fn run_import(
    store: &mut Store,
    backup: Backup,
    want_data: bool,
    want_settings: bool,
    persist: impl FnOnce(&Store, bool, bool) -> bool,
) -> Result<(usize, usize, bool, bool), String> {
    let original_prompts = store.prompts.clone();
    let original_settings = store.settings.clone();
    let original_prompts_locked = store.prompts_locked;
    let original_trash = store.trash.clone();
    let original_settings_locked = SETTINGS_LOCKED.load(Ordering::Relaxed);
    let ocr_was_on = ocr_index_flag_on(store);
    let (count, dropped, backup_pw_dropped) = apply_import(store, backup, want_data, want_settings);
    // Imported settings may switch the picture-text index off while the prompts
    // (and the trash) still carry its text.
    let ocr_cleared = want_settings && clear_ocr_text_if_switched_off(ocr_was_on, store);
    // The trash is not part of an import: confidential entries in it stay sealed
    // with the key that was in use, which a replaced vault no longer matches.
    let vault_mismatch = want_data && store.trash.iter().any(|p| p.secret) && vault_keys_differ(&original_settings, &store.settings);
    if want_settings {
        SETTINGS_LOCKED.store(false, Ordering::Relaxed); // a full settings import supersedes any corrupt-row lock
    }
    // Colliding accelerators are dropped from the prompts whatever the scope, so
    // a settings-only import that dropped one has changed prompt rows too.
    let write_prompts = want_data || dropped > 0 || ocr_cleared;
    // A data-only import still carries the backup's vault salt and verifier into the
    // settings (the imported confidential prompts are sealed with them): leaving them
    // out of the write would restart with the old keys against the new prompts.
    let write_settings = want_settings || vault_keys_differ(&original_settings, &store.settings);
    if !persist(store, write_prompts, write_settings) {
        store.prompts = original_prompts;
        store.trash = original_trash;
        store.settings = original_settings;
        store.prompts_locked = original_prompts_locked;
        SETTINGS_LOCKED.store(original_settings_locked, Ordering::Relaxed);
        mirror_minimize_to_tray(&store.settings);
        // Put back a set the disk was never told about, so nothing may be
        // skipped as "already written" on the next save either.
        store.forget_written_rows();
        return Err("save-failed".into());
    }
    // An import replaces the settings without going through the settings writer,
    // so the window-close handler's copy of the tray setting is refreshed here.
    if want_settings {
        mirror_minimize_to_tray(&store.settings);
    }
    // The session key belongs to the salt it was derived from: once the import
    // installed another salt, the old key would decrypt nothing and would seal
    // new confidential prompts against a verifier that no longer matches.
    if vault_keys_differ(&original_settings, &store.settings) {
        *SECRET_SESSION.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    Ok((count, dropped, backup_pw_dropped, vault_mismatch))
}

// Writes the halves an import touched in one database transaction. No JSON
// fallback: that file pair cannot be replaced atomically, so an import that
// cannot reach the database fails and changes nothing.
pub(crate) fn db_write_import(conn: &mut Connection, store: &Store, prompts: &[Prompt], write_prompts: bool, write_settings: bool) -> rusqlite::Result<()> {
    let mut written = store.written.borrow_mut();
    match (write_prompts, write_settings) {
        (true, true) => db_write_prompts_and_settings(conn, prompts, &store.settings, &mut written),
        (true, false) => db_write_prompts(conn, prompts, &mut written),
        (false, true) => db_write_settings(conn, &store.settings),
        (false, false) => Ok(()),
    }
}

pub(crate) fn save_import(app: &AppHandle, store: &Store, write_prompts: bool, write_settings: bool) -> bool {
    if data_dir_write_blocked(app) || (store.prompts_locked && write_prompts) {
        return false;
    }
    if write_settings && SETTINGS_LOCKED.load(Ordering::Relaxed) {
        return false;
    }
    let _writing = write_settings.then(settings_write_lock);
    let dir = data_dir(app);
    let prompts = strip_data_dir_prefix(&dir, data_dir_is_demo(app, &dir), &store.prompts);
    let Some(mut conn) = db_conn(app) else {
        append_crash_log("persist failed: the import could not open the database");
        return false;
    };
    if db_write_import(&mut conn, store, &prompts, write_prompts, write_settings).is_err() {
        append_crash_log("persist failed: the import could not be written to the database");
        return false;
    }
    if write_prompts {
        drop_stale_fallback(&dir, "prompts.json");
    }
    if write_settings {
        drop_stale_fallback(&dir, "settings.json");
        SETTINGS_DELAY.discard();
        if SETTINGS_WRITE_FAILED.swap(false, Ordering::Relaxed) {
            let _ = app.emit("settings-write-recovered", ());
        }
    }
    if write_prompts {
        spawn_sync_write_all(app, store); // Prompt-folders-as-Markdown, only once the rows are committed
    }
    true
}

// The file is picked separately so a wrong password can be retried on the SAME
// file instead of sending the user back through the picker.
#[tauri::command]
pub(crate) async fn import_all(
    app: AppHandle,
    state: State<'_, Db>,
    path: String,
    password: Option<String>,
    // "all" (default) | "data" | "settings" -- same three values and the same
    // restore_selective-style "only touch/roll back what that scope owns" contract
    // as restore_backup, just applied to a file import instead of a stored snapshot.
    scope: String,
) -> Result<ImportSummary, String> {
    // An import replaces the store wholesale and rolls the whole thing back if a
    // write fails, so the disk has to hold the exact state it is replacing first:
    // otherwise a rollback lands on a store missing the last collected counters.
    valid_scope(&scope)?;
    flush_settings_now(&app);
    let path = PathBuf::from(path);
    let raw = read_file_capped(&path, MAX_IMPORT_BYTES)?;
    // Try the caller-supplied password first, else the one stored for this machine.
    let pw = password.filter(|p| !p.is_empty()).or_else(|| {
        let store = lock(&state);
        stored_backup_password(&store)
    });
    // Encrypted (PSB1) or a legacy plaintext .json backup.
    let content = match backup_decrypt(&raw, pw.as_deref())? {
        Some(plain) => String::from_utf8(plain).map_err(|_| "corrupt backup".to_string())?,
        None => String::from_utf8(raw).map_err(|_| format!("not a {} backup", brand::DISPLAY_NAME))?,
    };
    let backup: Backup =
        serde_json::from_str(&content).map_err(|_| format!("not a {} backup", brand::DISPLAY_NAME))?;
    let want_data = scope != "settings";
    let want_settings = scope != "data";

    let (settings, count, dropped_hotkeys, backup_pw_dropped, vault_mismatch) = {
        let mut store = lock(&state);
        let ocr_was_on = ocr_index_flag_on(&store);
        let (count, dropped, backup_pw_dropped, vault_mismatch) = run_import(&mut store, backup, want_data, want_settings, |st, prompts, settings| {
            save_import(&app, st, prompts, settings)
        })?;
        // The import's own save covers the prompt rows; the trash is written here.
        if want_settings && ocr_was_on && !ocr_index_flag_on(&store) {
            save_prompts_and_trash(&app, &store);
        }
        (store.settings.clone(), count, dropped, backup_pw_dropped, vault_mismatch)
    };
    // Re-apply the imported state without a restart, the same way a settings-only
    // import does.
    refresh_after_import(&app, &settings);
    Ok(ImportSummary {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        count,
        dropped_hotkeys,
        backup_pw_dropped,
        vault_mismatch,
    })
}
