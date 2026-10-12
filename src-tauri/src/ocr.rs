use super::{
    brand, fs, gen_id, ocr_parse_capabilities_json, ocr_tag_from_capability_name, OcrCap,
    OcrInstallResult, OcrLang, OcrResult,
};
use image::imageops::FilterType;
use image::GenericImageView;
use std::collections::HashMap;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use windows::core::{HSTRING, PCWSTR};
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapDecoder, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::{FileAccessMode, StorageFile};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

// A very small crop is upscaled 2x before recognition -- the actual trick that
// makes small on-screen text (button labels, tooltips) recognizable. An image
// can only ever be too big (downscaled to MaxImageDimension) or too small
// (upscaled here), never both. The threshold is threaded down as a parameter
// (300 px, see OCR_UPSCALE_UNDER_PX).

// COM must be initialized once per thread that calls into WinRT, and never on
// the UI thread. Every OCR call runs inside spawn_blocking; those pool threads
// are reused across calls, so a thread-local guard keeps this to once per
// thread rather than once per recognition.
pub(crate) fn ensure_winrt() {
    thread_local! {
        static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    DONE.with(|d| {
        if !d.get() {
            // Only remember success. A genuine failure here (rare -- e.g. out of
            // memory) must not be recorded as done, or a later call on this same
            // thread would never retry COM init again and every WinRT call after
            // it would keep failing for a reason nothing here ever surfaces.
            if unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok() {
                d.set(true);
            }
        }
    });
}

// Engine cache: one OcrEngine per language tag, created once and reused --
// never recreated per call.
fn engine_cache() -> &'static Mutex<HashMap<String, OcrEngine>> {
    static CACHE: OnceLock<Mutex<HashMap<String, OcrEngine>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn engine_tag(engine: &OcrEngine) -> String {
    engine
        .RecognizerLanguage()
        .and_then(|l| l.LanguageTag())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

// "auto" -> the user's profile languages; if none of those has a recognition
// pack installed, fall back to whichever recognition language IS installed
// instead of failing (use whatever language packs are already present).
// Only when NOTHING is installed does this become a real error.
fn create_auto_engine() -> Result<OcrEngine, String> {
    if let Ok(e) = OcrEngine::TryCreateFromUserProfileLanguages() {
        return Ok(e);
    }
    let langs = OcrEngine::AvailableRecognizerLanguages().map_err(|e| e.to_string())?;
    let size = langs.Size().map_err(|e| e.to_string())?;
    if size == 0 {
        return Err("ocr-no-language".to_string());
    }
    let first = langs.GetAt(0).map_err(|e| e.to_string())?;
    OcrEngine::TryCreateFromLanguage(&first).map_err(|_| "ocr-no-language".to_string())
}

// Resolve (and cache) the OcrEngine for `lang` ("auto" or an explicit BCP-47
// tag). Never recreates an engine that is already cached. When `lang` is
// an explicit tag whose recognition pack is not actually installed,
// `auto_pick` (always on in the app) decides
// whether to fall back to whatever IS installed (use whatever language
// packs are already present) or to fail with ocr-no-language. The
// cache key stays the ORIGINAL requested `lang`, matching how "auto" already
// caches whatever engine it happened to resolve to -- installing a new
// language pack clears the whole cache (see mod ocr::install_result), so a
// stale fallback never lingers once the desired pack becomes available.
fn resolve_engine(lang: &str, auto_pick: bool) -> Result<OcrEngine, String> {
    // Hold ONE guard across the whole check-then-insert -- releasing it between
    // the read and the write let two concurrent OCR calls both miss the cache
    // and each build their own engine (wasted work, not a correctness bug).
    let mut cache = engine_cache().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(e) = cache.get(lang) {
        return Ok(e.clone());
    }
    let engine = if lang == "auto" {
        create_auto_engine()?
    } else {
        let explicit = Language::CreateLanguage(&HSTRING::from(lang))
            .ok()
            .and_then(|language| OcrEngine::TryCreateFromLanguage(&language).ok());
        match explicit {
            Some(e) => e,
            None if auto_pick => create_auto_engine()?,
            None => return Err("ocr-no-language".to_string()),
        }
    };
    cache.insert(lang.to_string(), engine.clone());
    Ok(engine)
}

fn bitmap_from_path(path: &Path) -> Result<SoftwareBitmap, String> {
    let hpath = HSTRING::from(super::winrt_file_path(path.to_string_lossy().as_ref()));
    let file = StorageFile::GetFileFromPathAsync(&hpath)
        .map_err(|e| e.to_string())?
        .get()
        .map_err(|e| e.to_string())?;
    let stream = file
        .OpenAsync(FileAccessMode::Read)
        .map_err(|e| e.to_string())?
        .get()
        .map_err(|e| e.to_string())?;
    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(|e| e.to_string())?
        .get()
        .map_err(|e| e.to_string())?;
    decoder.GetSoftwareBitmapAsync().map_err(|e| e.to_string())?.get().map_err(|e| e.to_string())
}

// Deletes its file on drop, on every path (success, recognition failure, or
// an early return) -- guarantees the temp PNG never survives the call.
struct TempFileGuard(Option<PathBuf>);
impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = fs::remove_file(p);
        }
    }
}

fn ocr_temp_path() -> PathBuf {
    std::env::temp_dir().join(brand::TEMP_DIR).join(format!("{}-ocr-{}.png", brand::TEMP_PREFIX, gen_id()))
}

// Downscale a too-large image (proportionally, to MaxImageDimension) or
// upscale a very small one 2x, writing the result to a NEW temp file --
// `path` itself (which may be the user's own file for ocr_image) is never
// modified. Returns `path` unchanged, with no extra disk write at all, when
// no resize is needed.
fn resize_for_ocr(path: &Path, max_dim: u32, upscale_below_px: u32) -> Result<(PathBuf, TempFileGuard), String> {
    // The file is decoded upright (phone photos carry an EXIF rotation that
    // BitmapDecoder would ignore) and handed on as a temp copy whenever its
    // pixels differ from the file's own.
    let reader = image::ImageReader::open(path).map_err(|e| e.to_string())?;
    let (img, orientation) = super::decode_image_with_orientation(reader).ok_or_else(|| "could not decode image".to_string())?;
    let (w, h) = img.dimensions();
    let resized = super::ocr_resize_target(w, h, max_dim, upscale_below_px)
        .map(|(nw, nh)| img.resize(nw, nh, FilterType::Lanczos3))
        .or_else(|| (orientation != image::metadata::Orientation::NoTransforms).then_some(img));
    match resized {
        None => Ok((path.to_path_buf(), TempFileGuard(None))),
        Some(im) => {
            let out = ocr_temp_path();
            if let Some(dir) = out.parent() {
                let _ = fs::create_dir_all(dir);
            }
            im.save(&out).map_err(|e| e.to_string())?;
            Ok((out.clone(), TempFileGuard(Some(out))))
        }
    }
}

// join_lines picks between the paragraph-joined text (ocr_join_lines) and
// the raw one-line-per-recognized-line text -- the ocrJoinLines expert setting.
fn recognize(path: &Path, upscale_below_px: u32, join_lines: bool, lang: &str, auto_pick_lang: bool) -> Result<OcrResult, String> {
    ensure_winrt();
    let engine = resolve_engine(lang, auto_pick_lang)?;
    let max_dim = OcrEngine::MaxImageDimension().map_err(|e| e.to_string())?;
    let (sized_path, _resize_guard) = resize_for_ocr(path, max_dim, upscale_below_px)?;
    let bitmap = bitmap_from_path(&sized_path)?;
    // Recognition runs exactly once per call.
    let result = engine.RecognizeAsync(&bitmap).map_err(|e| e.to_string())?.get().map_err(|e| e.to_string())?;
    let win_lines = result.Lines().map_err(|e| e.to_string())?;
    let size = win_lines.Size().map_err(|e| e.to_string())?;
    let mut lines = Vec::with_capacity(size as usize);
    for i in 0..size {
        let l = win_lines.GetAt(i).map_err(|e| e.to_string())?;
        lines.push(l.Text().map_err(|e| e.to_string())?.to_string());
    }
    // Nothing recognized -> Ok with empty text, never an error (a later pass
    // turns that into a friendly toast, not an error dialog).
    let text = super::ocr_assemble_text(&lines, join_lines);
    Ok(OcrResult { text, lang: engine_tag(&engine) })
}

pub(super) fn languages() -> Result<Vec<OcrLang>, String> {
    ensure_winrt();
    let langs = OcrEngine::AvailableRecognizerLanguages().map_err(|e| e.to_string())?;
    let size = langs.Size().map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(size as usize);
    for i in 0..size {
        let lang = langs.GetAt(i).map_err(|e| e.to_string())?;
        out.push(OcrLang {
            tag: lang.LanguageTag().map_err(|e| e.to_string())?.to_string(),
            label: lang.DisplayName().map_err(|e| e.to_string())?.to_string(),
        });
    }
    Ok(out)
}

pub(super) fn recognize_file(path: &Path, upscale_below_px: u32, join_lines: bool, lang: &str, auto_pick_lang: bool) -> Result<OcrResult, String> {
    recognize(path, upscale_below_px, join_lines, lang, auto_pick_lang)
}

// Writes the crop out as a temp PNG under the branded temp dir (same naming
// convention as write_snip_preview / pdfium_cache_path), then runs it through
// the same recognize() path -- deleted in every code path via TempFileGuard.
pub(super) fn recognize_crop(crop: image::RgbaImage, upscale_below_px: u32, join_lines: bool, lang: &str, auto_pick_lang: bool) -> Result<OcrResult, String> {
    let path = ocr_temp_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    crop.save(&path).map_err(|e| e.to_string())?;
    let _guard = TempFileGuard(Some(path.clone()));
    recognize(&path, upscale_below_px, join_lines, lang, auto_pick_lang)
}

// ---- OCR language-pack management ----

// Which language-pack family a capabilities/install call is about. Both
// families share the exact same Get-WindowsCapability/Add-WindowsCapability
// machinery below -- OCR and voice dictation each want their own
// Windows "Language.*" feature, nothing else differs. The module keeps its
// original name (OCR was here first); voice dictation reuses it wholesale
// rather than duplicating a COM-interfacing module tree, per 126 ("nichts
// davon wird kopiert").
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CapKind {
    Ocr,
    Speech,
}
impl CapKind {
    fn filter(self) -> &'static str {
        match self {
            CapKind::Ocr => "Language.OCR*",
            CapKind::Speech => "Language.Speech*",
        }
    }
    // The capability Name prefix Windows reports for this kind. Three functions
    // (validation, tag extraction, install-result file naming) depend on this
    // varying by kind -- hardcoding the OCR prefix regardless of kind would
    // make every speech language pack fail validation, lose its tag, and share
    // an install-result file with the OCR one.
    pub(super) fn prefix(self) -> &'static str {
        match self {
            CapKind::Ocr => "Language.OCR~~~",
            CapKind::Speech => "Language.Speech~~~",
        }
    }
    // Temp-file name segment AND the ui_flags/ui_values/ui_texts setting-key
    // prefix -- "voice", not "speech", to match the expert-menu keys (the
    // table of language-pack settings).
    pub(super) fn slug(self) -> &'static str {
        match self {
            CapKind::Ocr => "ocr",
            CapKind::Speech => "voice",
        }
    }
    pub(super) fn update_due_event(self) -> &'static str {
        match self {
            CapKind::Ocr => "ocr-lang-update-due",
            CapKind::Speech => "voice-lang-update-due",
        }
    }
}

struct CapCache {
    fetched_at: std::time::Instant,
    caps: Vec<OcrCap>,
}
// One cache per kind -- installing/refreshing OCR packages must never
// invalidate a freshly-fetched voice list or vice versa.
fn cap_cache(kind: CapKind) -> &'static Mutex<Option<CapCache>> {
    static OCR: OnceLock<Mutex<Option<CapCache>>> = OnceLock::new();
    static SPEECH: OnceLock<Mutex<Option<CapCache>>> = OnceLock::new();
    match kind {
        CapKind::Ocr => OCR.get_or_init(|| Mutex::new(None)),
        CapKind::Speech => SPEECH.get_or_init(|| Mutex::new(None)),
    }
}

// Whether this kind's listing failure has already reached the crash log
// this session -- a machine that always refuses the unelevated query would
// otherwise write the same multi-line PowerShell/DISM text on every poll.
fn caps_log_once(kind: CapKind) -> &'static AtomicBool {
    static OCR: OnceLock<AtomicBool> = OnceLock::new();
    static SPEECH: OnceLock<AtomicBool> = OnceLock::new();
    match kind {
        CapKind::Ocr => OCR.get_or_init(|| AtomicBool::new(false)),
        CapKind::Speech => SPEECH.get_or_init(|| AtomicBool::new(false)),
    }
}

// PowerShell/DISM output for this failure is often several lines of banner
// text ahead of the one line that actually says what happened -- the crash
// log gets just that first line; the frontend's details disclosure still
// gets the raw text untouched via caps_elevation_error.
pub(super) fn strip_ps_banner(raw: &str) -> &str {
    raw.lines().next().unwrap_or(raw).trim()
}

// The set most recently returned by `capabilities()` doubles as the
// install allowlist (see install_language) -- a name that was never
// actually enumerated is rejected even if it happens to match the naming
// regex.
pub(super) fn is_known_capability(kind: CapKind, name: &str) -> bool {
    cap_cache(kind)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|c| c.caps.iter().any(|cap| cap.name == name))
        .unwrap_or(false)
}

pub(super) fn capabilities(kind: CapKind, force: bool, cache_hours: f64) -> Result<Vec<OcrCap>, String> {
    if !force {
        if let Some(c) = cap_cache(kind).lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            if c.fetched_at.elapsed() < std::time::Duration::from_secs_f64(cache_hours * 3600.0) {
                return Ok(c.caps.clone());
            }
        }
    }
    let caps = fetch_capabilities(kind)?;
    *cap_cache(kind).lock().unwrap_or_else(|e| e.into_inner()) =
        Some(CapCache { fetched_at: std::time::Instant::now(), caps: caps.clone() });
    Ok(caps)
}

// Runs PowerShell exactly once (no window), parses its JSON, and resolves
// each capability's localized display name via the same WinRT Language
// lookup the recognizer itself uses -- so this list is localized without
// this app keeping its own name table.
fn fetch_capabilities(kind: CapKind) -> Result<Vec<OcrCap>, String> {
    ensure_winrt();
    let _ = fs::create_dir_all(dism_temp_dir());
    // The spec (and Microsoft's own docs) says listing needs no elevated rights,
    // and with the log-path redirect above that mostly holds -- but some
    // Windows builds/policies still refuse the read-only query outright for a
    // reason that varies by machine (confirmed: one throws over the DISM log
    // path, another over a flat "elevated rights required", neither sharing an
    // HResult). There is exactly one remedy either way -- the elevated path
    // below -- so ANY failure here maps to the same code, never a raw,
    // locale-specific PowerShell blob left for the UI to guess at. The raw
    // text still rides along (second field, "|"-joined) for the details
    // disclosure and the crash log.
    let script = format!(
        r#"Get-WindowsCapability -Online -Name "{filter}" {logargs} | Select-Object Name, State, Version, DownloadSize, InstallSize | ConvertTo-Json"#,
        filter = kind.filter(),
        logargs = dism_log_args(kind),
    );
    run_powershell_command(&script).and_then(|output| caps_from_json(kind, &output)).map_err(|raw| {
        if !caps_log_once(kind).swap(true, Ordering::Relaxed) {
            super::append_crash_log(&format!(
                "ocr caps listing failed ({}): {}",
                kind.slug(),
                strip_ps_banner(&raw)
            ));
        }
        caps_elevation_error(&raw)
    })
}
// The stable code the frontend matches on, plus the raw PowerShell/DISM text
// as a second "|"-joined field for the details disclosure -- split out as its
// own pure function so the wire format itself is unit-tested, not just typed
// inline and trusted.
pub(super) fn caps_elevation_error(raw: &str) -> String {
    format!("ocr-caps-need-elevation|{raw}")
}

// The PowerShell query's JSON -> the list the UI shows. Shared by the normal
// (unelevated) query and by the elevated one below, so both produce exactly
// the same list, localized the same way.
fn caps_from_json(kind: CapKind, output: &str) -> Result<Vec<OcrCap>, String> {
    ensure_winrt();
    let raw = ocr_parse_capabilities_json(kind.prefix(), output)?;
    let mut out = Vec::with_capacity(raw.len());
    for r in raw {
        let label = Language::CreateLanguage(&HSTRING::from(r.tag.as_str()))
            .and_then(|l| l.DisplayName())
            .map(|s| s.to_string())
            .unwrap_or_else(|_| r.tag.clone());
        out.push(OcrCap {
            name: r.name,
            tag: r.tag,
            label,
            state: r.state,
            version: r.version,
            download_size: r.download_size,
            install_size: r.install_size,
        });
    }
    Ok(out)
}

// Runs a PowerShell command with no visible window and returns its stdout.
// The explicit working directory is ours, always writable, never
// whatever the process happened to inherit -- DISM's "could not set the
// current directory" half of the log-path error had nothing to do with the
// actual query, so there is no reason to hand it a directory it might not
// be able to write to either.
fn run_powershell_command(script: &str) -> Result<String, String> {
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .current_dir(dism_temp_dir())
        .creation_flags(super::CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if stderr.is_empty() { "PowerShell command failed".to_string() } else { stderr });
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

// PowerShell single-quoted-string escaping (double an embedded quote). Used
// for both the (already-validated) capability name and our own generated
// temp paths, which can contain non-ASCII characters from the user's
// profile path even though they can never contain a single quote in
// practice -- escaping unconditionally costs nothing and removes the doubt.
fn ps_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

// Every elevated round trip gets its own unguessable file names. The temp
// folder is writable by anything running as this user, so a name derived only
// from the language tag or the process id could be created in advance: a
// planted result file makes the app report an install that never happened, and
// a planted (or swapped) script file is a way to get whatever the user just
// approved at the UAC prompt. The nonce is remembered for exactly as long as
// the request is in flight, so the poll below still finds its own file.
fn request_nonce() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..24).map(|_| char::from(b"abcdefghijklmnopqrstuvwxyz0123456789"[rng.gen_range(0..36)])).collect()
}
fn pending_nonces() -> &'static Mutex<std::collections::HashMap<String, String>> {
    static PENDING: OnceLock<Mutex<std::collections::HashMap<String, String>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}
fn nonce_key(kind: CapKind, tag: &str) -> String {
    format!("{}-{}", kind.slug(), tag)
}
pub(super) fn start_request(kind: CapKind, tag: &str) -> String {
    let nonce = request_nonce();
    pending_nonces()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(nonce_key(kind, tag), nonce.clone());
    nonce
}
fn pending_nonce(kind: CapKind, tag: &str) -> Option<String> {
    pending_nonces().lock().unwrap_or_else(|e| e.into_inner()).get(&nonce_key(kind, tag)).cloned()
}
fn finish_request(kind: CapKind, tag: &str) {
    pending_nonces().lock().unwrap_or_else(|e| e.into_inner()).remove(&nonce_key(kind, tag));
}

fn install_result_path(kind: CapKind, nonce: &str) -> PathBuf {
    std::env::temp_dir().join(brand::TEMP_DIR).join(format!("{}-{}-install-{}.json", brand::TEMP_PREFIX, kind.slug(), nonce))
}

// Launches an elevated PowerShell that runs Add-WindowsCapability and writes
// its own success/failure (including whatever error message Windows gave)
// to a result file, then returns immediately -- this call never waits for
// that process. The script goes over as an encoded command (see
// elevated_params), so there is no quoting of it to get wrong: `name` is
// regex+whitelist-validated by the caller and the result path is entirely ours.
pub(super) fn install_language(kind: CapKind, name: &str) -> Result<(), String> {
    let tag = ocr_tag_from_capability_name(kind.prefix(), name).unwrap_or_default();
    let nonce = start_request(kind, &tag);
    let result_path = install_result_path(kind, &nonce);
    let _ = fs::remove_file(&result_path); // a stale result must never look fresh

    // The script travels on the command line, never as a file: there is no
    // path a same-user process could swap between the prompt and the read.
    let script = format!(
        "$ErrorActionPreference = 'Stop'\r\ntry {{\r\n  Add-WindowsCapability -Online -Name {name} {logargs}\r\n  @{{ ok = $true }} | ConvertTo-Json | Out-File -Encoding utf8 {result}\r\n}} catch {{\r\n  @{{ ok = $false; error = $_.Exception.Message }} | ConvertTo-Json | Out-File -Encoding utf8 {result}\r\n}}\r\n",
        name = ps_single_quote(name),
        logargs = dism_log_args(kind),
        result = ps_single_quote(&result_path.to_string_lossy()),
    );
    // A launch that never happened has nothing to poll for: drop the nonce
    // again so a later result file under that name cannot be believed.
    run_elevated_script(&script).inspect_err(|_| finish_request(kind, &tag))
}

// The argument string that hands `script` to an elevated PowerShell on its own
// command line. The script is never written to a file: a file in the shared
// temp folder can be replaced by any process of this user between the UAC
// prompt and PowerShell opening it, which would run that replacement with the
// rights the user just granted. PowerShell takes the script as base64 of its
// UTF-16LE bytes, so no quoting of the script text is involved either.
pub(super) fn elevated_params(script: &str) -> Result<String, String> {
    use base64::Engine;
    let bytes: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let params = format!(
        "-NoProfile -NonInteractive -EncodedCommand {}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    );
    // CreateProcess accepts a command line of at most 32767 characters.
    if params.len() > 30_000 {
        return Err("the elevated script is too long for a command line".to_string());
    }
    Ok(params)
}

// Starts `script` through the UAC prompt, returning as soon as the elevated
// process has been launched (or the launch was refused). Used by both
// elevated paths -- installing a language pack and, on machines that demand
// rights even for the read-only listing, the listing itself. Any new
// elevated call site goes through here, never a bespoke ShellExecuteW.
fn run_elevated_script(script: &str) -> Result<(), String> {
    // The elevated script writes its result into this folder.
    fs::create_dir_all(std::env::temp_dir().join(brand::TEMP_DIR)).map_err(|e| e.to_string())?;
    let params = elevated_params(script)?;
    let code = unsafe {
        ShellExecuteW(
            HWND::default(),
            &HSTRING::from("runas"),
            &HSTRING::from("powershell.exe"),
            &HSTRING::from(params),
            PCWSTR::null(),
            SW_HIDE,
        )
    };
    let code = code.0 as isize;
    if code <= 32 {
        // SE_ERR_ACCESSDENIED (5) is what ShellExecuteW returns when the user
        // declines the UAC prompt; ERROR_CANCELLED (1223) shows up on some
        // Windows versions for the same outcome. Neither is a real failure on
        // its own -- what it MEANS differs by policy, not by whether this
        // particular account happens to be an administrator (Windows still
        // lets the credential prompt take a DIFFERENT admin's password).
        // The wire strings keep their original names for the frontend's own
        // dispatch on them (ui/dictation.js), even though "no-rights" now means
        // "Group Policy denies the prompt outright" rather than "this
        // account has none".
        if code == 5 || code == 1223 {
            return Err(match classify_elevation_refusal(elevation_denied_by_policy()) {
                ElevationRefusal::PolicyBlocked => "ocr-elevate-no-rights".to_string(),
                ElevationRefusal::Retryable => "ocr-elevate-cancelled".to_string(),
            });
        }
        return Err(format!("could not start the elevated process (ShellExecuteW code {})", code));
    }
    Ok(())
}

// Whether THIS account's own token can complete "runas" without anyone
// else's help -- never by string-matching a localised Windows message,
// which varies by machine and language. `TokenElevationTypeLimited` is a
// split-token administrator (UAC on): the consent prompt can succeed.
// `Full` means the process is already elevated. `Default` means no split
// token exists at all: either a standard account or an administrator
// running with UAC turned off entirely, so membership in the
// Administrators group is the only thing that tells the two apart. A
// `false` here does NOT mean elevation can never succeed at all -- the
// credential prompt Windows shows a standard account still takes a
// DIFFERENT administrator's password, see `elevation_denied_by_policy`.
fn account_can_elevate() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevationType, TOKEN_ELEVATION_TYPE, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows::Win32::UI::Shell::IsUserAnAdmin;
    unsafe {
        let mut token = windows::Win32::Foundation::HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return true; // probe failed for an unrelated reason -- never claim "no rights" on that
        }
        let mut kind = TOKEN_ELEVATION_TYPE::default();
        let mut ret_len = 0u32;
        let queried = GetTokenInformation(
            token,
            TokenElevationType,
            Some(&mut kind as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION_TYPE>() as u32,
            &mut ret_len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        if !queried {
            return true;
        }
        match kind.0 {
            3 => true,                    // TokenElevationTypeLimited -- split-token admin
            2 => true,                    // TokenElevationTypeFull -- already elevated
            _ => IsUserAnAdmin().as_bool(), // TokenElevationTypeDefault -- membership decides
        }
    }
}

// Whether Group Policy denies the elevation prompt outright for this
// account: "User Account Control: Behavior of the elevation prompt for
// standard users" set to "Automatically deny elevation requests"
// (`ConsentPromptBehaviorUser` = 0) skips the credential dialog entirely,
// so there is no prompt left for even a different administrator's
// password to complete -- the one case a retry genuinely cannot fix. Only
// meaningful for an account that cannot elevate itself: an administrator's
// own consent prompt is governed by a different key
// (`ConsentPromptBehaviorAdmin`) and is never denied outright by this one.
// Missing key or unreadable = not policy-denied, same as the key being
// absent entirely (the out-of-the-box default).
fn elevation_denied_by_policy() -> bool {
    if account_can_elevate() {
        return false;
    }
    winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System")
        .and_then(|k| k.get_value::<u32, _>("ConsentPromptBehaviorUser"))
        .map(|v| v == 0)
        .unwrap_or(false)
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ElevationRefusal {
    // Group Policy denies the elevation prompt outright: no dialog will
    // ever appear for this account, so a retry cannot help.
    PolicyBlocked,
    // The dialog itself was declined or cancelled this time. Windows lets
    // its credential prompt take a DIFFERENT administrator's password, so
    // even an account that cannot elevate itself may still succeed next
    // time -- trying again may succeed.
    Retryable,
}
// Pure decision, kept separate from the registry/token probes above so it
// is unit-testable without touching Windows: given the same
// refused/cancelled ShellExecuteW outcome, which of the two distinct
// results the user sees.
pub(super) fn classify_elevation_refusal(denied_by_policy: bool) -> ElevationRefusal {
    if denied_by_policy { ElevationRefusal::PolicyBlocked } else { ElevationRefusal::Retryable }
}

// DISM unconditionally writes a log to %WINDIR%\Logs\DISM at startup,
// before it does anything else -- unwritable there without admin rights, so
// an unprivileged Get-/Add-WindowsCapability call fails before the actual
// query even runs, misread as "needs elevation" when it is really just a
// log-path problem. Redirecting both -LogPath and -ScratchDirectory into
// our own always-writable temp folder sidesteps it entirely.
fn dism_temp_dir() -> PathBuf {
    std::env::temp_dir().join(brand::TEMP_DIR)
}
pub(super) fn dism_log_args(kind: CapKind) -> String {
    let dir = dism_temp_dir();
    let log = dir.join(format!("{}-{}-dism.log", brand::TEMP_PREFIX, kind.slug()));
    format!(
        "-LogPath {} -ScratchDirectory {}",
        ps_single_quote(&log.to_string_lossy()),
        ps_single_quote(&dir.to_string_lossy()),
    )
}

// The nonce also keeps two windows (allowMultipleInstances) from overwriting
// each other's script/result file for the SAME kind: it is per request, so it
// separates them at least as well as the process id it replaced.
pub(super) const CAPS_TAG: &str = "caps";
pub(super) fn caps_result_path(kind: CapKind, nonce: &str) -> PathBuf {
    std::env::temp_dir().join(brand::TEMP_DIR).join(format!("{}-{}-caps-{}.json", brand::TEMP_PREFIX, kind.slug(), nonce))
}
fn caps_tmp_path(kind: CapKind, nonce: &str) -> PathBuf {
    std::env::temp_dir().join(brand::TEMP_DIR).join(format!("{}-{}-caps-{}.json.tmp", brand::TEMP_PREFIX, kind.slug(), nonce))
}

// The same query fetch_capabilities runs, but through an elevated PowerShell
// that writes its JSON to a temp file. Needed on machines where Windows
// refuses even the read-only listing to a normal user: without a list there
// is no install allowlist either, so no language could be added at all.
// Never starts by itself -- only when the user presses the button in the dialog.
//
// PowerShell writes to a `.tmp` sibling first and Move-Item -Force's it onto
// the real result path in one step, so a poll landing mid-write either finds
// no file yet (not renamed) or the complete one (post-rename) -- never a
// half-written JSON blob.
pub(super) fn elevate_capabilities(kind: CapKind) -> Result<(), String> {
    let nonce = start_request(kind, CAPS_TAG);
    let result_path = caps_result_path(kind, &nonce);
    let tmp_path = caps_tmp_path(kind, &nonce);
    let _ = fs::remove_file(&result_path); // a stale list must never look fresh
    let _ = fs::remove_file(&tmp_path);
    let script = format!(
        "$ErrorActionPreference = 'Stop'\r\ntry {{\r\n  Get-WindowsCapability -Online -Name \"{filter}\" {logargs} | Select-Object Name, State, Version, DownloadSize, InstallSize | ConvertTo-Json | Out-File -Encoding utf8 {tmp}\r\n}} catch {{\r\n  @{{ error = $_.Exception.Message }} | ConvertTo-Json | Out-File -Encoding utf8 {tmp}\r\n}}\r\nMove-Item -LiteralPath {tmp} -Destination {result} -Force\r\n",
        filter = kind.filter(),
        logargs = dism_log_args(kind),
        tmp = ps_single_quote(&tmp_path.to_string_lossy()),
        result = ps_single_quote(&result_path.to_string_lossy()),
    );
    run_elevated_script(&script).inspect_err(|_| finish_request(kind, CAPS_TAG))
}

// Reads what the elevated listing wrote. Ok(None) = still running, or never
// started. A successful list also fills the cache the normal query fills,
// so the install allowlist keeps working unchanged.
//
// Deletes the file ONLY on a definite terminal outcome (a parseable error,
// or a parseable list) -- never on a read that came back empty or
// unparseable, which is treated as "not ready yet" instead of discarding a
// result that might still be mid-write. Mirrors install_result's shape.
pub(super) fn elevated_capabilities_result(kind: CapKind) -> Result<Option<Vec<OcrCap>>, String> {
    // No nonce means no elevated listing of ours is in flight.
    let Some(nonce) = pending_nonce(kind, CAPS_TAG) else { return Ok(None) };
    let path = caps_result_path(kind, &nonce);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };
    // Windows PowerShell's `Out-File -Encoding utf8` writes a byte-order mark.
    let text = text.trim_start_matches('\u{feff}').trim().to_string();
    if text.is_empty() {
        return Ok(None);
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
        if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
            let _ = fs::remove_file(&path);
            finish_request(kind, CAPS_TAG);
            return Err(err.to_string());
        }
    } else {
        return Ok(None); // not valid JSON (yet) -- leave it for the next poll
    }
    let caps = match caps_from_json(kind, &text) {
        Ok(c) => c,
        Err(e) => {
            let _ = fs::remove_file(&path);
            finish_request(kind, CAPS_TAG);
            return Err(e);
        }
    };
    let _ = fs::remove_file(&path);
    finish_request(kind, CAPS_TAG);
    *cap_cache(kind).lock().unwrap_or_else(|e| e.into_inner()) =
        Some(CapCache { fetched_at: std::time::Instant::now(), caps: caps.clone() });
    Ok(Some(caps))
}

// Parses the install result file's text. Windows PowerShell's `Out-File
// -Encoding utf8` prepends a byte-order mark, which neither `trim` nor the
// JSON parser accepts; None means "not parseable (yet)".
pub(super) fn parse_install_result(text: &str) -> Option<OcrInstallResult> {
    let text = text.trim_start_matches('\u{feff}').trim();
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    let error = value.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());
    Some(OcrInstallResult { ok, error })
}

// Reads (and, once a terminal result is found, deletes) the result file
// install_language's elevated process writes. Deleting on a terminal read
// keeps a later poll from re-triggering the cache invalidation below on
// every subsequent call.
pub(super) fn install_result(kind: CapKind, name: &str) -> Option<OcrInstallResult> {
    let tag = ocr_tag_from_capability_name(kind.prefix(), name).unwrap_or_default();
    // No nonce means no install of this pack is in flight: whatever happens to
    // sit in the temp folder was not written for us.
    let nonce = pending_nonce(kind, &tag)?;
    let path = install_result_path(kind, &nonce);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        // Not there yet -- the elevated process has not written it (or
        // finished the UAC prompt) -- genuinely still pending.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        // The file IS there but cannot be read (permissions, a sharing
        // violation, a disk error) -- unlike "not found", this will never
        // resolve itself on the next poll, so it must not look like "still
        // running" forever. Left on disk for inspection rather than deleted,
        // since a locked/inaccessible file could not be removed either.
        Err(_) => {
            finish_request(kind, &tag);
            return Some(OcrInstallResult { ok: false, error: Some("install-result-unreadable".to_string()) });
        }
    };
    // Content that fails to parse stays "pending" rather than terminal --
    // Out-File's write is not guaranteed atomic, so a poll can legitimately
    // catch the file mid-write; the next poll a few seconds later normally
    // sees the finished JSON. A permanently corrupt file is caught by the
    // frontend's own poll cutoff instead of being declared a failure here.
    let OcrInstallResult { ok, error } = parse_install_result(&text)?;
    let _ = fs::remove_file(&path);
    finish_request(kind, &tag);
    if ok {
        // Installation is done -- both caches must reflect it immediately, no
        // restart needed. This command is the natural place to do that:
        // it is the point where the backend already has fresh proof the
        // install finished, rather than waiting on the frontend to notice
        // state == "Installed" via a second ocr_capabilities() round trip.
        *cap_cache(kind).lock().unwrap_or_else(|e| e.into_inner()) = None;
        // The recognition engine cache is OCR-only (mod voice has no such
        // cache -- a new voice session always resolves its language fresh).
        if kind == CapKind::Ocr {
            engine_cache().lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
    }
    Some(OcrInstallResult { ok, error })
}
