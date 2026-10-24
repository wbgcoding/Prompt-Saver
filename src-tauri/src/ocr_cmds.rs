use super::*;

// ---------- OCR (Windows Media.Ocr, fully offline) ----------
//
// Text recognition from an image file or a snip-tool crop using Windows' built-in recognizer:
// no bundled OCR library, no network. Backend only; the buttons live in the frontend.
//
// `ocr_join_lines` is pure text logic, so plain `cargo test --lib` covers it on any platform.
// Everything else sits behind `#[cfg(windows)]`.

#[derive(Serialize)]
pub(crate) struct OcrLang {
    pub(crate) tag: String,
    pub(crate) label: String,
}

#[derive(Serialize, Clone)]
pub(crate) struct OcrResult {
    pub(crate) text: String,
    pub(crate) lang: String,
}

// ---------- OCR language packs, managed from inside the app ----------
//
// `OcrCap` mirrors one row of `Get-WindowsCapability -Online -Name "Language.OCR*"`. Size and
// version are what Windows reports: a size Windows does not know becomes `None` (the frontend
// renders "-"), never a guess.
#[derive(Serialize, Clone)]
pub(crate) struct OcrCap {
    pub(crate) name: String,
    pub(crate) tag: String,
    pub(crate) label: String,
    pub(crate) state: String,
    pub(crate) version: String,
    pub(crate) download_size: Option<u64>,
    pub(crate) install_size: Option<u64>,
}

// A capability row before its localized label is looked up. The label needs the WinRT
// `Language::CreateLanguage` call (Windows-only); the rest is plain JSON parsing, kept free of
// Windows APIs so the parser is unit-testable on any platform.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OcrCapRaw {
    pub(crate) name: String,
    pub(crate) tag: String,
    pub(crate) state: String,
    pub(crate) version: String,
    pub(crate) download_size: Option<u64>,
    pub(crate) install_size: Option<u64>,
}

// What was last seen while a language was installed, kept across restarts (serialized into
// Settings.ui_texts, see ocr_lang_load_snapshots) so a Windows feature update that silently
// drops a language pack is still detected as "needs update" after relaunch.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub(crate) struct OcrCapSnapshot {
    pub(crate) version: String,
    pub(crate) installed: bool,
}

#[derive(Serialize, Clone)]
pub(crate) struct OcrInstallResult {
    pub(crate) ok: bool,
    pub(crate) error: Option<String>,
}

// Payload for the "ocr-lang-update-due" event (background watcher).
#[derive(Serialize, Clone)]
pub(crate) struct OcrLangUpdateDue {
    pub(crate) name: String,
    pub(crate) tag: String,
    pub(crate) label: String,
}

// ---------------------------------------------------------------------------
// Voice dictation via Windows.Media.SpeechRecognition. Same COM-per-thread rule as mod ocr.
// Safety rules: recognized text is never logged, no audio is written to disk, one session
// runs at a time, and recognition fails closed rather than send audio anywhere without the
// user's consent.
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
pub(crate) struct VoiceLang {
    pub(crate) tag: String,
    pub(crate) label: String,
    pub(crate) installed: bool,
}

#[derive(Serialize, Clone)]
pub(crate) struct VoiceSupport {
    pub(crate) supported: bool,
    pub(crate) engine: &'static str,
    pub(crate) langs: Vec<VoiceLang>,
    pub(crate) needs_online: bool,
}

// voice_start's success payload. `lang` is the tag actually resolved from "auto" or the
// requested one; the frontend needs it to run spoken-command post-processing in the language
// really being recognized, which may differ from the UI language.
#[derive(Serialize, Clone)]
pub(crate) struct VoiceStartResult {
    pub(crate) id: u64,
    pub(crate) lang: String,
}

// Payload for voice-partial / voice-text.
#[derive(Serialize, Clone)]
pub(crate) struct VoiceTextEvent {
    pub(crate) id: u64,
    pub(crate) token: u64,
    pub(crate) text: String,
}

// Payload for voice-stopped.
#[derive(Serialize, Clone)]
pub(crate) struct VoiceStoppedEvent {
    pub(crate) id: u64,
    pub(crate) token: u64,
    pub(crate) reason: String, // manual | silence | max | error | shutdown
}

// Payload for voice-error.
#[derive(Serialize, Clone)]
pub(crate) struct VoiceErrorEvent {
    pub(crate) id: u64,
    pub(crate) token: u64,
    pub(crate) code: String,
}

// One session at a time: a second voice_start while this holds Some(..) gets voice-busy
// instead of opening a second recognizer. `ended` flips true once, inside the session's
// Completed handler, before voice-stopped is emitted; every stop path (button, hotkey,
// shutdown, hard cap, silence) reads it first so a finished session is never stopped twice,
// since a second StopAsync on a Completed session throws (0x80131509).
pub(crate) struct VoiceHandle {
    pub(crate) id: u64,
    pub(crate) stop: std::sync::mpsc::Sender<&'static str>,
    pub(crate) ended: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
// Second field: claimed while a voice_start call is between its busy check and the real
// VoiceHandle landing in the first field (see voice_start).
pub(crate) struct VoiceState(
    pub(crate) Mutex<Option<VoiceHandle>>,
    pub(crate) std::sync::atomic::AtomicBool,
    pub(crate) std::sync::atomic::AtomicBool,
);

pub(crate) const MAX_OCR_IMAGE_BYTES: u64 = 200 * 1024 * 1024; // a single image, not a whole export (MAX_IMPORT_BYTES)
// Encoded length of a data URL handed over IPC; a 4K PNG from the screenshot editor is
// ~40 MB, so this only stops runaway input.
pub(crate) const MAX_IMAGE_DATA_URL_BYTES: u64 = 200 * 1024 * 1024;
pub(crate) const OCR_IMAGE_EXTS: [&str; 5] = ["png", "jpg", "jpeg", "bmp", "webp"];

// Shared oversized-input guard for a data URL decoded from IPC: each caller supplies its own
// ceiling (OCR keeps MAX_OCR_IMAGE_BYTES).
pub(crate) fn data_url_within_limit(data_url: &str, limit: u64) -> bool {
    data_url.len() as u64 <= limit
}

// Pure decision for mod ocr's resize_for_ocr: shrink an oversized image to max_dim, enlarge a
// very small one 2x, or leave it. Outside `#[cfg(windows)]` so it is unit-testable anywhere.
pub(crate) fn ocr_resize_target(w: u32, h: u32, max_dim: u32, upscale_below_px: u32) -> Option<(u32, u32)> {
    if w > max_dim || h > max_dim {
        let scale = max_dim as f64 / (w.max(h) as f64);
        let nw = ((w as f64 * scale).round() as u32).max(1);
        let nh = ((h as f64 * scale).round() as u32).max(1);
        Some((nw, nh))
    } else if w.max(h) < upscale_below_px {
        Some((w * 2, h * 2))
    } else {
        None
    }
}

// StorageFile::GetFileFromPathAsync only takes a plain backslash path. A forward-slash path
// (from a webview or a script) or an extended-length one fails with a bad-pathname error.
pub(crate) fn winrt_file_path(path: &str) -> String {
    let flat = path.replace('/', r"\");
    if let Some(rest) = flat.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    match flat.strip_prefix(r"\\?\") {
        Some(rest) => rest.to_string(),
        None => flat,
    }
}

pub(crate) fn ocr_validate_path(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    let meta = fs::metadata(p).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("not a file".to_string());
    }
    let ext_ok = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| OCR_IMAGE_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false);
    if !ext_ok {
        return Err("unsupported image type".to_string());
    }
    check_import_size(p, MAX_OCR_IMAGE_BYTES)?;
    Ok(p.to_path_buf())
}

// Join OCR'd lines back into readable text. A line not ending in sentence-ending punctuation,
// followed by a line starting with a lowercase letter, is a mid-sentence wrap and is joined
// with a space. An empty recognized line is a paragraph break, kept as a blank line.
// Everything else stays on its own line. Right-to-left scripts are not special-cased:
// Windows' result already has lines in reading order, and this only inspects whitespace and
// case at the edges.
pub(crate) fn ocr_join_lines(lines: &[String]) -> String {
    fn ends_sentence(line: &str) -> bool {
        let trimmed = line.trim_end_matches(['"', '\'', ')', ']', '\u{201d}', '\u{2019}']);
        matches!(trimmed.chars().last(), Some(c) if matches!(c, '.' | '!' | '?' | '\u{2026}'))
    }
    fn starts_lowercase(line: &str) -> bool {
        line.trim_start().chars().next().map(|c| c.is_lowercase()).unwrap_or(false)
    }

    let mut out = String::new();
    let mut prev: Option<&str> = None;
    for raw in lines {
        let line = raw.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            out.push_str("\n\n");
            prev = None;
            continue;
        }
        match prev {
            None => out.push_str(line),
            Some(p) if !ends_sentence(p) && starts_lowercase(line) => {
                out.push(' ');
                out.push_str(line.trim_start());
            }
            Some(_) => {
                out.push('\n');
                out.push_str(line);
            }
        }
        prev = Some(line);
    }
    out
}

// Assembles the final OCR text from the recognized lines and trims it: an image whose lines
// are all empty must yield an empty string, not a run of newlines, so the caller can tell
// "nothing was recognized" from a result.
pub(crate) fn ocr_assemble_text(lines: &[String], join_lines: bool) -> String {
    let joined = if join_lines { ocr_join_lines(lines) } else { lines.join("\n") };
    joined.trim().to_string()
}

// Strict allowlist check for a capability Name, before it is used to build a PowerShell
// argument. Anchored on the whole string, so "Language.OCR~~~de-DE~0.0.1.0; rm -rf" is
// rejected: its trailing bytes do not match the required "~0.0.1.0" suffix. The tag is
// further restricted to [A-Za-z0-9-], so a name that passes can never contain a shell
// metacharacter. `prefix` is "Language.OCR~~~" or "Language.Speech~~~" (CapKind::prefix(),
// inside mod ocr), kept a plain &str because these functions run before the #[cfg(windows)]
// branch in every caller and CapKind is windows-only.
pub(crate) fn ocr_capability_name_is_well_formed(prefix: &str, name: &str) -> bool {
    const SUFFIX: &str = "~0.0.1.0";
    if name.len() < prefix.len() + SUFFIX.len() + 2 {
        return false; // too short to hold prefix + a 2-char tag + suffix
    }
    if !name.starts_with(prefix) || !name.ends_with(SUFFIX) {
        return false;
    }
    let tag = &name[prefix.len()..name.len() - SUFFIX.len()];
    tag.len() >= 2 && tag.len() <= 12 && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

// Pulls "<tag>" out of "Language.OCR~~~<tag>~0.0.1.0" (or the Speech equivalent). None for
// anything without that exact shape; callers pass validated names, but this stays total
// rather than panicking.
pub(crate) fn ocr_tag_from_capability_name(prefix: &str, name: &str) -> Option<String> {
    let rest = name.strip_prefix(prefix)?;
    let tag = rest.split('~').next()?;
    (!tag.is_empty()).then(|| tag.to_string())
}

// Parses `Get-WindowsCapability -Online -Name "Language.OCR*" | Select-Object Name, State,
// Version, DownloadSize, InstallSize | ConvertTo-Json` output. PowerShell's ConvertTo-Json
// emits a single object (not a one-element array) when exactly one matches; both shapes are
// accepted. A field Windows did not report (DownloadSize/InstallSize on an installed
// capability, say) becomes `None`, never `Some(0)`.
//
// `State` comes back as the bare integer on real systems
// (`Microsoft.Dism.Commands.WindowsCapabilityState`, e.g. 0 = not present, 4 = installed) but
// may be the enum's string name on other Windows/PowerShell versions, so both shapes are
// accepted. An unrecognized numeric code maps to NotPresent: it gates whether the UI thinks a
// language is usable, so understating readiness is the safe default.
pub(crate) fn ocr_capability_state_from_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(4) => "Installed".to_string(),
            _ => "NotPresent".to_string(),
        },
        _ => String::new(),
    }
}

pub(crate) fn ocr_parse_capabilities_json(prefix: &str, json: &str) -> Result<Vec<OcrCapRaw>, String> {
    let trimmed = json.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).map_err(|e| e.to_string())?;
    let items: Vec<serde_json::Value> = match value {
        serde_json::Value::Array(a) => a,
        obj @ serde_json::Value::Object(_) => vec![obj],
        serde_json::Value::Null => Vec::new(),
        _ => return Err("unexpected PowerShell output shape".to_string()),
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let name = item.get("Name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        if name.is_empty() {
            continue;
        }
        let tag = ocr_tag_from_capability_name(prefix, &name).unwrap_or_default();
        let state = item.get("State").map(ocr_capability_state_from_value).unwrap_or_default();
        let version = item.get("Version").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        // Real Windows output uses 0 as a "not reported" placeholder, even for an installed
        // capability with real disk usage. Fold it into None like an absent field, so the UI shows
        // "-" rather than a misleading "Download (0 B)".
        let download_size = item.get("DownloadSize").and_then(|v| v.as_u64()).filter(|&n| n != 0);
        let install_size = item.get("InstallSize").and_then(|v| v.as_u64()).filter(|&n| n != 0);
        out.push(OcrCapRaw { name, tag, state, version, download_size, install_size });
    }
    Ok(out)
}

// "Updatable" (worth a background notice) is narrower than "not installed": a language
// Windows never reported as installed is new, not a regression. Only a previously installed
// language that changed version or vanished (state flipped to NotPresent, as after a Windows
// feature update removes language packs) counts.
pub(crate) fn ocr_lang_needs_update(previous: Option<&OcrCapSnapshot>, current_state: &str, current_version: &str) -> bool {
    match previous {
        None => false,
        Some(prev) => {
            if !prev.installed {
                return false;
            }
            if current_state != "Installed" {
                return true;
            }
            current_version != prev.version
        }
    }
}

// Shared by OCR and voice language-pack bookkeeping: "ocrLangVersions"/"ocrLangNextCheck" for
// CapKind::Ocr, "voiceLangVersions"/"voiceLangNextCheck" for CapKind::Speech.
#[cfg(windows)]
pub(crate) fn lang_versions_key(kind: ocr::CapKind) -> String {
    format!("{}LangVersions", kind.slug())
}
#[cfg(windows)]
pub(crate) fn lang_next_check_key(kind: ocr::CapKind) -> String {
    format!("{}LangNextCheck", kind.slug())
}

// Cross-restart bookkeeping for the "updatable?" decision: a JSON blob (tag ->
// OcrCapSnapshot) in the free-form ui_texts map rather than a new Settings field.
#[cfg(windows)]
pub(crate) fn lang_load_snapshots(settings: &Settings, kind: ocr::CapKind) -> HashMap<String, OcrCapSnapshot> {
    settings
        .ui_texts
        .get(&lang_versions_key(kind))
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

#[cfg(windows)]
pub(crate) fn lang_save_snapshots(settings: &mut Settings, kind: ocr::CapKind, snapshots: &HashMap<String, OcrCapSnapshot>) {
    if let Ok(json) = serde_json::to_string(snapshots) {
        settings.ui_texts.insert(lang_versions_key(kind), json);
    }
}

#[tauri::command]
pub(crate) async fn ocr_languages() -> Result<Vec<OcrLang>, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(ocr::languages).await.map_err(|e| e.to_string())?
    }
}

// upscale_under_px, join_lines, lang and auto_pick_lang are sent by the frontend on every
// call (it holds the live values) instead of the backend re-reading them from Db state.
#[tauri::command]
pub(crate) async fn ocr_image(path: String, upscale_under_px: u32, join_lines: bool, lang: String, auto_pick_lang: bool) -> Result<OcrResult, String> {
    let validated = ocr_validate_path(&path)?;
    let upscale_under_px = upscale_under_px.clamp(100, 800);
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || ocr::recognize_file(&validated, upscale_under_px, join_lines, &lang, auto_pick_lang))
            .await
            .map_err(|e| e.to_string())?
    }
}

// Entry point for "Recognize text" in #snip-modal, which runs on the capture already shown
// there; at that point it exists only as a data URL (it may not be saved yet). Same shape as
// copy_image_data for the screenshot editor.
#[tauri::command]
pub(crate) async fn ocr_image_data(data_url: String, upscale_under_px: u32, join_lines: bool, lang: String, auto_pick_lang: bool) -> Result<OcrResult, String> {
    // Same ceiling as the file-based path (check_import_size): this arrives over IPC with no
    // on-disk size to check, so nothing else bounds the encoded, decoded and RGBA buffers below.
    if !data_url_within_limit(&data_url, MAX_OCR_IMAGE_BYTES) {
        return Err(format!(
            "file is too large ({} MB); the limit is {} MB",
            data_url.len() as u64 / (1024 * 1024),
            MAX_OCR_IMAGE_BYTES / (1024 * 1024)
        ));
    }
    let upscale_under_px = upscale_under_px.clamp(100, 800);
    #[cfg(windows)]
    {
        // The base64 and image decode of a large capture is CPU work: it runs on the blocking
        // pool together with the recognition, never on the async command worker.
        tauri::async_runtime::spawn_blocking(move || {
            let img = decode_ocr_data_url(&data_url)?;
            ocr::recognize_crop(img, upscale_under_px, join_lines, &lang, auto_pick_lang)
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

// Data URL to RGBA pixels, the CPU-heavy half of ocr_image_data.
#[cfg(windows)]
pub(crate) fn decode_ocr_data_url(data_url: &str) -> Result<image::RgbaImage, String> {
    let bytes = data_url_bytes(data_url);
    Ok(decode_image_mem_oriented(&bytes).ok_or("could not decode image")?.to_rgba8())
}

// Long enough for a real screenshot of text, short enough that a photo of a wall of text
// cannot inflate the database.
pub(crate) const OCR_INDEX_MAX_CHARS: usize = 20000;

// The cap itself, split out of ocr_index_text so it is testable without a real OCR engine.
// `.chars().take(n)` never splits a multi-byte character.
pub(crate) fn cap_ocr_text(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

// Text recognized in an attached picture, for the search index only (never shown, exported or
// synchronized). It always auto-picks the recognition language instead of reading the
// interactive OCR language setting, because indexing runs silently with ocrIndex on whether or
// not the OCR feature itself is on. A picture that cannot be decoded, an engine with no
// language pack, or a blank result all come back as "": the same "nothing to index" outcome.
pub(crate) fn ocr_index_text(data_url: &str, max_chars: usize) -> String {
    if data_url.is_empty() || !data_url_within_limit(data_url, MAX_OCR_IMAGE_BYTES) {
        return String::new();
    }
    let bytes = data_url_bytes(data_url);
    let Some(img) = decode_image_mem_oriented(&bytes) else { return String::new() };
    #[cfg(windows)]
    let text = ocr::recognize_crop(img.to_rgba8(), 480, true, "auto", true).map(|r| r.text).unwrap_or_default();
    cap_ocr_text(&text, max_chars)
}

// The decision behind "attach fills the field only while ocrIndex is on, removing the
// picture always empties it", pulled out of add_prompt/update_prompt so the gating is
// provable without a real OCR engine: `recognize` is called at most once, only for a
// non-empty picture with the switch on.
pub(crate) fn resolved_image_text(new_image: &str, ocr_index_on: bool, recognize: impl FnOnce(&str) -> String) -> String {
    if new_image.is_empty() || !ocr_index_on {
        String::new()
    } else {
        recognize(new_image)
    }
}

// Ids whose recognized picture text contains `query`, as its own pure function so it is
// testable without a running app. This is the only way image_text is read back: it never
// crosses into the frontend (see SlimPrompt's From impl), so the client-side plain-text
// search asks the backend for this one field.
pub(crate) fn image_text_matches(prompts: &[Prompt], ocr_index_on: bool, query: &str) -> Vec<String> {
    if !ocr_index_on {
        return Vec::new();
    }
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    prompts
        .iter()
        .filter(|p| !p.image_text.is_empty() && p.image_text.to_lowercase().contains(&q))
        .map(|p| p.id.clone())
        .collect()
}

#[tauri::command(async)]
pub(crate) fn search_image_text(state: State<Db>, query: String) -> Vec<String> {
    let store = lock(&state);
    let ocr_index_on = store.settings.ui_flags.get("ocrIndex").copied().unwrap_or(false);
    image_text_matches(&store.prompts, ocr_index_on, &query)
}

// Set by cancel_ocr_index_backfill, polled between pictures in the loop below.
pub(crate) static OCR_INDEX_CANCEL: AtomicBool = AtomicBool::new(false);

// One backfill pass at a time. Starting a second one would reset the cancel flag
// the first is still polling (the cancel is lost, two passes read the same pictures
// at once) and both report under the one background-job row.
static OCR_INDEX_RUNNING: AtomicBool = AtomicBool::new(false);

// Held for the length of a pass; released on every way out of it.
pub(crate) struct OcrIndexPass;

impl OcrIndexPass {
    pub(crate) fn claim() -> Option<OcrIndexPass> {
        OCR_INDEX_RUNNING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| OcrIndexPass)
    }
}

impl Drop for OcrIndexPass {
    fn drop(&mut self) {
        OCR_INDEX_RUNNING.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub(crate) fn cancel_ocr_index_backfill() {
    OCR_INDEX_CANCEL.store(true, Ordering::SeqCst);
}

// Writes recognized text onto a prompt only while it still shows the picture the text came
// from and has no text yet: the user may have replaced or removed the picture (or refreshed
// the text) while recognition ran.
pub(crate) fn apply_backfill_text(p: &mut Prompt, recognized_from: &str, text: String) -> bool {
    if p.image != recognized_from || !p.image_text.is_empty() {
        return false;
    }
    p.image_text = text;
    true
}

// Switching the picture-text index off must not leave the recognized text behind: it is
// searchable content read out of the user's pictures. Returns whether anything was cleared, so
// the caller saves only when needed.
pub(crate) fn clear_image_text(prompts: &mut [Prompt]) -> bool {
    let mut changed = false;
    for p in prompts.iter_mut().filter(|p| !p.image_text.is_empty()) {
        p.image_text.clear();
        changed = true;
    }
    changed
}

// Bumped by every clear. A recognition pass remembers the value it started with and never
// writes its text back once it changed (or the switch is off).
pub(crate) static OCR_EPOCH: AtomicU64 = AtomicU64::new(0);

pub(crate) fn ocr_epoch() -> u64 {
    OCR_EPOCH.load(Ordering::SeqCst)
}

pub(crate) fn ocr_index_flag_on(store: &Store) -> bool {
    store.settings.ui_flags.get("ocrIndex").copied().unwrap_or(false)
}

// Cancels a running pass, invalidates every pass in flight and erases the text in prompts and
// trash. The caller holds the store lock and saves when this returns true. Settings arriving
// from outside (an import or restore) may switch the index off; its text goes with it, like
// the switch itself.
pub(crate) fn clear_ocr_text_if_switched_off(was_on: bool, store: &mut Store) -> bool {
    was_on && !ocr_index_flag_on(store) && clear_ocr_index_text(store)
}

// Writes the prompts and trash after the picture text was cleared in memory. A failed write
// leaves the text in the database although the index is off: it is logged here and the next start
// clears it again (heal_ocr_text_at_boot), so callers do not have to fail an operation whose main
// data change already went through.
pub(crate) fn write_ocr_wipe(app: &AppHandle, store: &Store, context: &str) {
    if !save_prompts_and_trash(app, store) {
        append_crash_log(&format!("{context}: the picture-text wipe could not be written"));
    }
}

// Start-up self-heal for a wipe that never reached the database: the index is off, yet picture
// text is still stored. Returns true when something was cleared and has to be written. A locked
// prompt set is never rewritten, and the sample library of demo mode is left alone.
pub(crate) fn heal_ocr_text_at_boot(store: &mut Store) -> bool {
    if store.prompts_locked || ocr_index_flag_on(store) || store.settings.ui_flags.get("demoMode").copied().unwrap_or(false) {
        return false;
    }
    clear_ocr_index_text(store)
}

pub(crate) fn clear_ocr_index_text(store: &mut Store) -> bool {
    cancel_ocr_index_backfill();
    OCR_EPOCH.fetch_add(1, Ordering::SeqCst);
    let live = clear_image_text(&mut store.prompts);
    let trashed = clear_image_text(&mut store.trash);
    live || trashed
}

// Writes one recognized text back, only while the index is still on, no clear happened since
// the pass started, and the prompt still shows the same picture. Returns whether it wrote.
pub(crate) fn apply_ocr_backfill_text(store: &mut Store, id: &str, recognized_from: &str, text: String, started_epoch: u64) -> bool {
    if ocr_epoch() != started_epoch || !ocr_index_flag_on(store) {
        return false;
    }
    store.prompts.iter_mut().find(|p| p.id == id).is_some_and(|p| apply_backfill_text(p, recognized_from, text))
}

// apply_ocr_backfill_text plus its write. A write that fails takes the text out of memory again,
// so the session never searches text the disk does not have, and the pass stops with the error.
pub(crate) fn apply_ocr_backfill_and_save(
    store: &mut Store,
    id: &str,
    recognized_from: &str,
    text: String,
    started_epoch: u64,
    save: impl FnOnce(&Store) -> bool,
) -> Result<(), String> {
    if !apply_ocr_backfill_text(store, id, recognized_from, text, started_epoch) {
        return Ok(());
    }
    if save(store) {
        return Ok(());
    }
    if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
        p.image_text.clear();
    }
    Err("save-failed".to_string())
}

// The expert-menu "read existing pictures" action: every prompt that carries a picture but no
// index text yet is read once, one at a time in a single sequential pass, never a flood of
// parallel OCR calls. Progress and cancellation use the shared background-jobs registry
// (push_bg_op), so there is no progress UI of its own.
#[tauri::command]
pub(crate) async fn ocr_index_backfill(app: AppHandle, state: State<'_, Db>) -> Result<usize, String> {
    let Some(_pass) = OcrIndexPass::claim() else {
        return Err("ocr-index-busy".to_string());
    };
    OCR_INDEX_CANCEL.store(false, Ordering::SeqCst);
    let (ids, max_chars, started_epoch) = {
        let store = lock(&state);
        let ids: Vec<String> = store
            .prompts
            .iter()
            .filter(|p| !p.image.is_empty() && p.image_text.is_empty())
            .map(|p| p.id.clone())
            .collect();
        (ids, OCR_INDEX_MAX_CHARS, ocr_epoch())
    };
    let total = ids.len();
    push_bg_op(&app, "ocr-index", "ocr-index", if total == 0 { "done" } else { "running" }, Some(0), None);
    let mut done = 0usize;
    for id in ids {
        if OCR_INDEX_CANCEL.load(Ordering::SeqCst) {
            push_bg_op(&app, "ocr-index", "ocr-index", "error", None, None);
            return Ok(done);
        }
        let image = {
            let store = lock(&state);
            store.prompts.iter().find(|p| p.id == id).map(|p| p.image.clone())
        };
        if let Some(image) = image {
            if !image.is_empty() {
                let joined = tauri::async_runtime::spawn_blocking(move || (ocr_index_text(&image, max_chars), image)).await;
                // A decoder that panicked ends the pass; the job row must not stay "running".
                let (text, image) = match joined {
                    Ok(pair) => pair,
                    Err(e) => {
                        push_bg_op(&app, "ocr-index", "ocr-index", "error", None, None);
                        return Err(e.to_string());
                    }
                };
                if !text.is_empty() {
                    let mut store = lock(&state);
                    if let Err(e) = apply_ocr_backfill_and_save(&mut store, &id, &image, text, started_epoch, |st| save_prompts(&app, st)) {
                        drop(store);
                        push_bg_op(&app, "ocr-index", "ocr-index", "error", None, None);
                        return Err(e);
                    }
                }
            }
        }
        done += 1;
        let pct = ((done * 100) / total.max(1)) as u32;
        push_bg_op(&app, "ocr-index", "ocr-index", "running", Some(pct), None);
    }
    push_bg_op(&app, "ocr-index", "ocr-index", "done", None, None);
    Ok(done)
}

// List recognition-language capabilities. Usually no elevated rights are needed, but some
// Windows builds throw ERROR_ELEVATION_REQUIRED even for this read-only query; see the
// HResult check in ocr::fetch_capabilities, which surfaces it as "ocr-caps-need-elevation".
// `force` bypasses the in-memory cache (OCR_LANG_CACHE_H hours); the cache is what
// ocr_install_language's whitelist check validates against, so a name can never be installed
// without having been enumerated.
#[tauri::command]
pub(crate) async fn ocr_capabilities(force: bool) -> Result<Vec<OcrCap>, String> {
    let cache_hours = OCR_LANG_CACHE_H;
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || ocr::capabilities(ocr::CapKind::Ocr, force, cache_hours))
            .await
            .map_err(|e| e.to_string())?
    }
}

// Some machines refuse the read-only capability listing to a normal user (ocr_capabilities
// then fails with "ocr-caps-need-elevation"). This runs the same listing through the UAC
// prompt. Only called from the button in the language dialog; the app never asks for rights
// on its own. Returns once the elevated process was launched; the frontend polls
// ocr_capabilities_elevated_result for the list.
// async + spawn_blocking: ShellExecuteW's "runas" blocks the calling thread until the user
// answers the UAC prompt (walking away leaves it blocked), so the wait goes onto the blocking
// pool rather than the tokio runtime's small async-worker set, where it could starve every
// other async command.
#[tauri::command]
pub(crate) async fn ocr_capabilities_elevate() -> Result<(), String> {
    #[cfg(windows)]
    {
        // spawn_blocking: ShellExecuteW("runas") sits blocked until the user answers the UAC prompt.
        // An async fn runs on the runtime's small worker set, so blocking one there would starve
        // every other async command.
        tauri::async_runtime::spawn_blocking(|| ocr::elevate_capabilities(ocr::CapKind::Ocr)).await.map_err(|e| e.to_string())?
    }
}

// None while the elevated listing is still running (or never started), Some(list) once it
// wrote one. A failure inside the elevated process is an error, never an empty list. Async +
// spawn_blocking: this reads a file and calls into COM (caps_from_json's Language lookups)
// via ensure_winrt, which must never run on the UI thread.
#[tauri::command]
pub(crate) async fn ocr_capabilities_elevated_result() -> Result<Option<Vec<OcrCap>>, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(|| ocr::elevated_capabilities_result(ocr::CapKind::Ocr)).await.map_err(|e| e.to_string())?
    }
}

// Kick off an elevated install for one language capability. Validates `name` before any
// string is built or a process is touched: first the anchored character-class check, then,
// only if that passed, membership in the capability set ocr_capabilities() most recently
// enumerated. Returns once the elevated process was launched (or rejected) and never waits
// for Add-WindowsCapability; the frontend polls ocr_install_result and
// ocr_capabilities(force: true).
// async: same ShellExecuteW/UAC blocking reason as ocr_capabilities_elevate.
#[tauri::command]
pub(crate) async fn ocr_install_language(name: String) -> Result<(), String> {
    if !ocr_capability_name_is_well_formed("Language.OCR~~~", &name) {
        return Err("invalid OCR capability name".to_string());
    }
    #[cfg(windows)]
    {
        if !ocr::is_known_capability(ocr::CapKind::Ocr, &name) {
            return Err("unknown OCR capability name".to_string());
        }
        // spawn_blocking: same UAC-wait reasoning as ocr_capabilities_elevate.
        tauri::async_runtime::spawn_blocking(move || ocr::install_language(ocr::CapKind::Ocr, &name)).await.map_err(|e| e.to_string())?
    }
}

// Read (and, on a terminal result, clear) the result file the elevated installer writes for
// `name`. None while the install is still running or never started; Some(...) once it
// finished. A successful result is when the capability and engine caches are invalidated (see
// the comment in mod ocr::install_result).
#[tauri::command]
pub(crate) fn ocr_install_result(name: String) -> Option<OcrInstallResult> {
    if !ocr_capability_name_is_well_formed("Language.OCR~~~", &name) {
        return None;
    }
    #[cfg(windows)]
    {
        ocr::install_result(ocr::CapKind::Ocr, &name)
    }
}

// The "Cancel" on the background update notice defers the next automatic notice by
// OCR_LANG_SNOOZE_H hours so the user is not re-nagged on every restart. It does not touch
// the check cadence; a manual check (ocr_capabilities(force: true)) is unaffected.
#[tauri::command(async)]
pub(crate) fn ocr_lang_snooze(app: AppHandle, state: State<Db>) {
    #[cfg(windows)]
    lang_snooze(&app, &state, ocr::CapKind::Ocr);
}

// Speech (voice dictation) language packs: thin wrappers around the same
// Get-WindowsCapability/Add-WindowsCapability machinery OCR uses (mod ocr) with
// CapKind::Speech. ocrAutoCopy/ocrJoinLines have no counterpart here: that is OCR-specific
// post-processing, not something dictated text needs.
#[tauri::command]
pub(crate) async fn speech_capabilities(force: bool) -> Result<Vec<OcrCap>, String> {
    let cache_hours = VOICE_LANG_CACHE_H;
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || ocr::capabilities(ocr::CapKind::Speech, force, cache_hours))
            .await
            .map_err(|e| e.to_string())?
    }
}

// async: same ShellExecuteW/UAC blocking reason as ocr_capabilities_elevate.
#[tauri::command]
pub(crate) async fn speech_capabilities_elevate() -> Result<(), String> {
    #[cfg(windows)]
    {
        // spawn_blocking: same UAC-wait reasoning as ocr_capabilities_elevate.
        tauri::async_runtime::spawn_blocking(|| ocr::elevate_capabilities(ocr::CapKind::Speech)).await.map_err(|e| e.to_string())?
    }
}

#[tauri::command]
pub(crate) async fn speech_capabilities_elevated_result() -> Result<Option<Vec<OcrCap>>, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(|| ocr::elevated_capabilities_result(ocr::CapKind::Speech)).await.map_err(|e| e.to_string())?
    }
}

// async: same ShellExecuteW/UAC blocking reason as ocr_install_language.
#[tauri::command]
pub(crate) async fn speech_install_language(name: String) -> Result<(), String> {
    if !ocr_capability_name_is_well_formed("Language.Speech~~~", &name) {
        return Err("invalid speech capability name".to_string());
    }
    #[cfg(windows)]
    {
        if !ocr::is_known_capability(ocr::CapKind::Speech, &name) {
            return Err("unknown speech capability name".to_string());
        }
        // spawn_blocking: same UAC-wait reasoning as ocr_capabilities_elevate.
        tauri::async_runtime::spawn_blocking(move || ocr::install_language(ocr::CapKind::Speech, &name)).await.map_err(|e| e.to_string())?
    }
}

#[tauri::command]
pub(crate) fn speech_install_result(name: String) -> Option<OcrInstallResult> {
    if !ocr_capability_name_is_well_formed("Language.Speech~~~", &name) {
        return None;
    }
    #[cfg(windows)]
    {
        ocr::install_result(ocr::CapKind::Speech, &name)
    }
}

#[tauri::command(async)]
pub(crate) fn voice_lang_snooze(app: AppHandle, state: State<Db>) {
    #[cfg(windows)]
    lang_snooze(&app, &state, ocr::CapKind::Speech);
}

// Shared by ocr_lang_snooze/voice_lang_snooze: defer the next automatic notice by the kind's
// snooze hours (lang_snooze_hours) so the user is not re-nagged on every restart. The check
// cadence and a manual check (force: true) are unaffected.
#[cfg(windows)]
pub(crate) fn lang_snooze(app: &AppHandle, state: &State<Db>, kind: ocr::CapKind) {
    let mut store = lock(state);
    let hours = lang_snooze_hours(kind);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let next = now + (hours * 3600.0) as u64;
    store.settings.ui_texts.insert(lang_next_check_key(kind), next.to_string());
    save_settings(app, &store.settings);
}

// Language-pack timings, one set per kind (both kinds use the same values): how long a
// checked capability list is trusted, how often the background watcher looks for updates, and
// how long "Cancel" on the update notice defers the next notice, all in hours.
pub(crate) const OCR_LANG_CACHE_H: f64 = 24.0;
pub(crate) const VOICE_LANG_CACHE_H: f64 = 24.0;
#[cfg(windows)]
pub(crate) const OCR_LANG_CHECK_H: f64 = 168.0;
#[cfg(windows)]
pub(crate) const VOICE_LANG_CHECK_H: f64 = 168.0;
#[cfg(windows)]
pub(crate) const OCR_LANG_SNOOZE_H: f64 = 168.0;
#[cfg(windows)]
pub(crate) const VOICE_LANG_SNOOZE_H: f64 = 168.0;

#[cfg(windows)]
pub(crate) fn lang_cache_hours(kind: ocr::CapKind) -> f64 {
    match kind {
        ocr::CapKind::Ocr => OCR_LANG_CACHE_H,
        ocr::CapKind::Speech => VOICE_LANG_CACHE_H,
    }
}

#[cfg(windows)]
pub(crate) fn lang_check_hours(kind: ocr::CapKind) -> f64 {
    match kind {
        ocr::CapKind::Ocr => OCR_LANG_CHECK_H,
        ocr::CapKind::Speech => VOICE_LANG_CHECK_H,
    }
}

#[cfg(windows)]
pub(crate) fn lang_snooze_hours(kind: ocr::CapKind) -> f64 {
    match kind {
        ocr::CapKind::Ocr => OCR_LANG_SNOOZE_H,
        ocr::CapKind::Speech => VOICE_LANG_SNOOZE_H,
    }
}

// Well after launch: the first round must not compete with window build.
#[cfg(windows)]
pub(crate) const OCR_LANG_FIRST_DELAY_SECS: u64 = 30;

#[cfg(windows)]
pub(crate) fn lang_check_interval(kind: ocr::CapKind) -> Duration {
    Duration::from_secs((lang_check_hours(kind) * 3600.0) as u64)
}

// Background watcher for "an installed language pack is now updatable": one registration per
// kind on the shared scheduler thread, like auto-backup, update-check and temp-sweep.
// Notification only; it never installs anything itself (elevation always needs the frontend's
// confirmDialog explanation first). A round only lists what is due in one event, so multiple
// due packages are never processed in parallel.
#[cfg(windows)]
pub(crate) fn start_lang_update_watcher(app: AppHandle, kind: ocr::CapKind) {
    let job_name = format!("{}-lang-check", kind.slug());
    let name_for_job = job_name.clone();
    scheduler::register_periodic(&job_name, Duration::from_secs(OCR_LANG_FIRST_DELAY_SECS), lang_check_interval(kind), move || {
        lang_update_tick(&app, kind);
        scheduler::retime(&name_for_job, lang_check_interval(kind));
    });
}

// One round of the watcher. The persisted next-due time decides whether anything happens, so
// a restart neither skips nor duplicates a check.
#[cfg(windows)]
pub(crate) fn lang_update_tick(app: &AppHandle, kind: ocr::CapKind) {
    let snapshot = app.try_state::<Db>().map(|s| {
        let store = s.lock().unwrap_or_else(|e| e.into_inner());
        (
            lang_cache_hours(kind),
            store
                .settings
                .ui_texts
                .get(&lang_next_check_key(kind))
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0),
            lang_load_snapshots(&store.settings, kind),
        )
    });
    let Some((cache_hours, next_check, snapshots)) = snapshot else { return };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if now < next_check {
        return;
    }
    let Ok(caps) = ocr::capabilities(kind, false, cache_hours) else { return };
    let mut due = Vec::new();
    let mut new_snapshots = snapshots.clone();
    for cap in &caps {
        let prev = snapshots.get(&cap.tag);
        if ocr_lang_needs_update(prev, &cap.state, &cap.version) {
            due.push(OcrLangUpdateDue {
                name: cap.name.clone(),
                tag: cap.tag.clone(),
                label: cap.label.clone(),
            });
        }
        // Keep the remembered snapshot in step with what Windows reports now, so the next check
        // compares against today's truth.
        let now_installed = cap.state == "Installed";
        if now_installed || prev.is_some() {
            new_snapshots.insert(
                cap.tag.clone(),
                OcrCapSnapshot { version: cap.version.clone(), installed: now_installed },
            );
        }
    }
    if !due.is_empty() && app.emit(kind.update_due_event(), &due).is_err() {
        // Delivery failed: leave the snapshot and the next-check time as they were, so the next tick
        // redoes this comparison and retries the emit. Persisting either would make the notice look
        // handled (a since-updated snapshot no longer compares as "needs update") though nothing was
        // shown, suppressing it for up to a full check interval.
        return;
    }
    if let Some(s) = app.try_state::<Db>() {
        let mut store = s.lock().unwrap_or_else(|e| e.into_inner());
        lang_save_snapshots(&mut store.settings, kind, &new_snapshots);
        // Persist the next-due time so a user who dismisses the notice any way other than "Cancel"
        // (lang_snooze) is not re-nagged on every start; the in-process cadence resets on launch.
        let next = now + (lang_check_hours(kind) * 3600.0) as u64;
        store.settings.ui_texts.insert(lang_next_check_key(kind), next.to_string());
        save_settings(app, &store.settings);
    }
}
