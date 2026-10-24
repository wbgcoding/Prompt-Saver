use super::*;

// ---------- Settings commands ----------

// Secret text rows an earlier version kept for its remote receiver: the password hash and
// the TLS key, certificate and CA. The rows are purged when settings load, but a copy that
// leaves the backend (a window, an export, a backup) is stripped again so none of them can
// reach a file or a window even if a stale one slipped in.
pub(crate) const REMOTE_PASSWORD_KEY: &str = "remotePasswordHash";
const REMOTE_SECRET_KEYS: [&str; 4] = [REMOTE_PASSWORD_KEY, "remoteTlsKey", "remoteTlsCert", "remoteTlsCa"];

pub(crate) fn is_remote_secret_key(key: &str) -> bool {
    REMOTE_SECRET_KEYS.contains(&key)
}

pub(crate) fn strip_remote_secrets(texts: &mut std::collections::HashMap<String, String>) {
    for k in REMOTE_SECRET_KEYS {
        texts.remove(k);
    }
}

// The copy of the settings a window may hold: no leftover receiver secret and no sealed
// backup password.
pub(crate) fn webview_settings(settings: &Settings) -> Settings {
    let mut copy = settings.clone();
    strip_remote_secrets(&mut copy.ui_texts);
    // The sealed backup password is read in the backend only; the window asks
    // has_backup_password whether one is set.
    copy.ui_texts.remove(BACKUP_PW_KEY);
    copy
}

#[tauri::command(async)]
pub(crate) fn get_settings(state: State<Db>) -> Settings {
    webview_settings(&lock(&state).settings)
}

// What the window needs from a prompt to draw it: every field of `Prompt` except the image
// itself. A stored image is a PNG data URL of up to ~400 KB, too costly to resend on every
// grid rebuild. `has_image` tells the tile an image exists; `image_rev` changes whenever the
// stored image does, so a cached copy knows when it went stale. The bytes come from
// `prompt_image`, one prompt at a time.
#[derive(Serialize)]
pub(crate) struct SlimPrompt {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) color: String,
    pub(crate) has_image: bool,
    pub(crate) image_rev: u64,
    pub(crate) show_image: bool,
    pub(crate) copy_image: bool,
    pub(crate) file_path: String,
    pub(crate) pdf_page: u32,
    pub(crate) icon_path: String,
    pub(crate) caption: String,
    pub(crate) caption_size: u32,
    pub(crate) font: String,
    pub(crate) font_size: u32,
    pub(crate) favorite: bool,
    pub(crate) tags: Vec<String>,
    pub(crate) notes: String,
    pub(crate) hotkey: String,
    pub(crate) hotkey_paste: String,
    pub(crate) trashed_at: u64,
    pub(crate) variants: Vec<String>,
    pub(crate) rotate_mode: String,
    pub(crate) rotate_auto: bool,
    pub(crate) media_duration_ms: u64,
    pub(crate) media_width: u32,
    pub(crate) media_height: u32,
    pub(crate) media_size_bytes: u64,
    // Mirrors Prompt.secret. `text`/`variants` above already carry the ciphertext
    // (locked) or the session-decrypted plain text (unlocked): see slim_prompts.
    pub(crate) secret: bool,
    // True while `secret` is true and the session vault is not unlocked. The
    // window shows the locked bar and refuses to copy exactly when this is true;
    // never persisted, computed fresh on every slim_prompts() call.
    pub(crate) locked: bool,
}

impl From<&Prompt> for SlimPrompt {
    fn from(p: &Prompt) -> Self {
        // Destructured on purpose: a field added to `Prompt` breaks this line until someone decides
        // whether the window needs it. image_text is the deliberate exception, bound to `_`: it is a
        // search index and never reaches the frontend.
        let Prompt {
            id, name, text, color, image, show_image, copy_image, file_path, pdf_page,
            icon_path, caption, caption_size, font, font_size, favorite, tags, notes,
            hotkey, hotkey_paste, trashed_at, variants, rotate_mode, rotate_auto,
            media_duration_ms, media_width, media_height, media_size_bytes,
            image_text: _, secret,
        } = p;
        SlimPrompt {
            id: id.clone(),
            name: name.clone(),
            text: text.clone(),
            color: color.clone(),
            has_image: !image.is_empty(),
            // 0 for "no image" keeps the empty case from hashing to some arbitrary
            // non-zero number that a cache could mistake for real content.
            image_rev: if image.is_empty() { 0 } else { text_hash(image) },
            show_image: *show_image,
            copy_image: *copy_image,
            file_path: file_path.clone(),
            pdf_page: *pdf_page,
            icon_path: icon_path.clone(),
            caption: caption.clone(),
            caption_size: *caption_size,
            font: font.clone(),
            font_size: *font_size,
            favorite: *favorite,
            tags: tags.clone(),
            notes: notes.clone(),
            hotkey: hotkey.clone(),
            hotkey_paste: hotkey_paste.clone(),
            trashed_at: *trashed_at,
            variants: variants.clone(),
            rotate_mode: rotate_mode.clone(),
            rotate_auto: *rotate_auto,
            media_duration_ms: *media_duration_ms,
            media_width: *media_width,
            media_height: *media_height,
            media_size_bytes: *media_size_bytes,
            secret: *secret,
            locked: false,
        }
    }
}

// `secret_key` is the already-checked session key (see secret_session_key): None means locked
// (no key, or the timeout expired). A confidential prompt's ciphertext never reaches the
// frontend while locked: text/variants are blanked, so a locked entry cannot be searched,
// previewed or copied from the window's JSON state. Unlocked, the text is decrypted here and
// flows like any other prompt.
pub(crate) fn slim_prompts(prompts: &[Prompt], secret_key: Option<[u8; 32]>) -> Vec<SlimPrompt> {
    prompts
        .iter()
        .map(|p| {
            let mut s = SlimPrompt::from(p);
            if p.secret {
                // A ciphertext the session key does not open (sealed under another
                // vault's salt) is sent as locked too, never as an empty open entry
                // an editor save could overwrite.
                match reveal_secret_text(p, secret_key) {
                    Ok((text, variants)) => {
                        s.text = text;
                        s.variants = variants;
                        s.locked = false;
                    }
                    Err(_) => {
                        s.text = String::new();
                        s.variants = Vec::new();
                        s.locked = true;
                    }
                }
            }
            s
        })
        .collect()
}

// Prompts + settings in one IPC roundtrip (renderGrid hot path).
#[derive(Serialize)]
pub(crate) struct AppState {
    pub(crate) prompts: Vec<SlimPrompt>,
    pub(crate) settings: Settings,
}

// Async: cloning every prompt must not run on the UI thread; a big library would
// otherwise stall the window on each render.
#[tauri::command]
pub(crate) async fn get_state(state: State<'_, Db>) -> Result<AppState, String> {
    let store = lock(&state);
    let secret_key = secret_session_key(secret_timeout_min(&store.settings));
    Ok(AppState {
        prompts: slim_prompts(&store.prompts, secret_key),
        settings: webview_settings(&store.settings),
    })
}

// One prompt's stored image, on demand. Empty string = it has none (or the id is
// gone), which the window treats the same way: nothing to draw.
#[tauri::command]
pub(crate) async fn prompt_image(state: State<'_, Db>, id: String) -> Result<String, String> {
    let store = lock(&state);
    Ok(store
        .prompts
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.image.clone())
        .unwrap_or_default())
}

// The quick launcher only reads id/name/text/color on a surface that must be ready the instant
// its hotkey fires, so only those four small strings are cloned instead of whole prompts.
#[derive(Serialize)]
pub(crate) struct LauncherItem {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) color: String,
}
#[derive(Serialize)]
pub(crate) struct LauncherState {
    pub(crate) prompts: Vec<LauncherItem>,
    pub(crate) language: String,
    pub(crate) usage: HashMap<String, u32>,
    pub(crate) last_used: HashMap<String, u64>,
    pub(crate) prefill: String,
}
#[tauri::command]
pub(crate) async fn launcher_items(state: State<'_, Db>) -> Result<LauncherState, String> {
    let store = lock(&state);
    let key = secret_session_key(secret_timeout_min(&store.settings));
    Ok(LauncherState {
        prompts: store
            .prompts
            .iter()
            .filter_map(|p| {
                // Same rule as the library search: a confidential entry with no key
                // available drops out of the list entirely, rather than showing its
                // ciphertext as if it were the real body.
                let text = if p.secret { reveal_secret_text(p, key).ok()?.0 } else { p.text.clone() };
                Some(LauncherItem { id: p.id.clone(), name: p.name.clone(), text, color: p.color.clone() })
            })
            .collect(),
        language: store.settings.language.clone(),
        usage: store.settings.usage.clone(),
        last_used: store.settings.last_used.clone(),
        prefill: std::mem::take(&mut *LAUNCHER_PREFILL.lock().unwrap_or_else(|e| e.into_inner())),
    })
}
