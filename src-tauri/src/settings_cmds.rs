use super::*;

// ---------- Settings commands ----------

// The copy of the settings a window may hold: the receiver's private key stays
// in the backend.
pub(crate) fn webview_settings(settings: &Settings) -> Settings {
    let mut copy = settings.clone();
    remote_tls::strip_private_key(&mut copy);
    // The password hash is checked inside the backend only; its dialog has its own command.
    copy.ui_texts.remove(remote::REMOTE_PASSWORD_KEY);
    copy
}

#[tauri::command(async)]
pub(crate) fn get_settings(state: State<Db>) -> Settings {
    webview_settings(&lock(&state).settings)
}

// What the window needs from a prompt to draw it: every field of `Prompt` except
// the image itself. A stored image is a PNG data URL of up to ~400 KB, and sending
// all of them again on every grid rebuild was the single most expensive thing the
// app did. `has_image` tells the tile an image exists, `image_rev` changes whenever
// the stored image does: so a copy already cached in the window knows when it went
// stale. The bytes themselves come from `prompt_image`, one prompt at a time.
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
        // Destructured rather than read field by field on purpose: a field added to
        // `Prompt` later breaks this line until someone decides whether the window
        // needs it, instead of silently never arriving there. image_text is the one
        // deliberate exception, bound to `_`: it is a search index, never handed to
        // the frontend at all.
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

// `secret_key` is the already-checked, already-timeout-tested session key (see
// secret_session_key): None means locked (no key, or the timeout expired).
// A confidential prompt's ciphertext never reaches the frontend while locked:
// text/variants are blanked instead of sent as-is, so a locked entry cannot be
// searched, previewed or copied from the window's own JSON state. Unlocked, the
// real text is decrypted here and flows exactly like any other prompt from that
// point on (search finds it, the tile copies it): nothing else needs to know
// the entry was ever confidential.
pub(crate) fn slim_prompts(prompts: &[Prompt], secret_key: Option<[u8; 32]>) -> Vec<SlimPrompt> {
    prompts
        .iter()
        .map(|p| {
            let mut s = SlimPrompt::from(p);
            if p.secret {
                match secret_key {
                    Some(key) => {
                        // Same upgrade as reveal_secret_text: the window must see the
                        // spelling it can resolve, whatever the sealed text still holds.
                        s.text = upgrade_placeholders(&secret_decrypt(&p.text, &key).unwrap_or_default());
                        s.variants = p
                            .variants
                            .iter()
                            .map(|v| upgrade_placeholders(&secret_decrypt(v, &key).unwrap_or_default()))
                            .collect();
                        s.locked = false;
                    }
                    None => {
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

// The quick launcher only ever reads id/name/text/color, on a surface that has to
// be ready the instant its hotkey fires: cloning a whole prompt each (tags, notes,
// entries and all) was exactly the wrong cost to pay there. The four fields are
// small strings, so cloning just those is the actual saving.
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
