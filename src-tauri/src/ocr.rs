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

// COM must be initialized once per thread that calls into WinRT, never on the UI thread.
// OCR calls run in spawn_blocking on reused pool threads, so a thread-local guard keeps it
// to once per thread.
pub(crate) fn ensure_winrt() {
    thread_local! {
        static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    DONE.with(|d| {
        if !d.get() {
            // Only remember success: recording a genuine failure (e.g. out of memory) as done would
            // stop later calls on this thread from retrying.
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

// "auto" -> the user's profile languages; if none has a recognition pack installed, fall
// back to any installed recognition language. Only when nothing is installed is it an error.
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

// Resolve (and cache) the OcrEngine for `lang` ("auto" or a BCP-47 tag). For an explicit
// tag whose pack is not installed, `auto_pick` decides between falling back to an installed
// language and failing with ocr-no-language. The cache key stays the requested `lang`, like
// "auto"; installing a pack clears the whole cache (see mod ocr::install_result), so a
// stale fallback never lingers.
fn resolve_engine(lang: &str, auto_pick: bool) -> Result<OcrEngine, String> {
    // One guard spans the whole check-then-insert so two concurrent calls cannot both miss the
    // cache and each build an engine.
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
// an early return): guarantees the temp PNG never survives the call.
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

// Downscale a too-large image (proportionally, to MaxImageDimension) or upscale a very
// small one 2x, which is what makes small on-screen text recognizable. The result goes to a
// new temp file: `path` itself, possibly the user's own file, is never modified. Returns
// `path` unchanged, with no disk write, when no resize is needed.
fn resize_for_ocr(path: &Path, max_dim: u32, upscale_below_px: u32) -> Result<(PathBuf, TempFileGuard), String> {
    // Decoded upright (phone photos carry an EXIF rotation BitmapDecoder would ignore) and
    // handed on as a temp copy whenever the pixels differ from the file's.
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
// the raw one-line-per-recognized-line text: the ocrJoinLines expert setting.
fn recognize(path: &Path, upscale_below_px: u32, join_lines: bool, lang: &str, auto_pick_lang: bool) -> Result<OcrResult, String> {
    ensure_winrt();
    let engine = resolve_engine(lang, auto_pick_lang)?;
    let max_dim = OcrEngine::MaxImageDimension().map_err(|e| e.to_string())?;
    let (sized_path, _resize_guard) = resize_for_ocr(path, max_dim, upscale_below_px)?;
    let bitmap = bitmap_from_path(&sized_path)?;
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

// Writes the crop out as a temp PNG under the branded temp dir (like write_snip_preview)
// and runs it through recognize(); TempFileGuard deletes it on every path.
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

// Which language-pack family a capabilities/install call is about. OCR and voice dictation
// share the same Get-WindowsCapability/Add-WindowsCapability machinery and differ only in
// the Windows "Language.*" feature, so voice reuses this module instead of duplicating a
// COM-interfacing tree.
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
    // The capability Name prefix Windows reports for this kind. Validation, tag extraction and
    // install-result file naming all depend on it: a hardcoded OCR prefix would fail every
    // speech pack's validation, lose its tag and share its result file with OCR.
    pub(super) fn prefix(self) -> &'static str {
        match self {
            CapKind::Ocr => "Language.OCR~~~",
            CapKind::Speech => "Language.Speech~~~",
        }
    }
    // Temp-file name segment and ui_flags/ui_values/ui_texts setting-key prefix: "voice", not
    // "speech", to match the expert-menu keys.
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
// One cache per kind: installing/refreshing OCR packages must never
// invalidate a freshly-fetched voice list or vice versa.
fn cap_cache(kind: CapKind) -> &'static Mutex<Option<CapCache>> {
    static OCR: OnceLock<Mutex<Option<CapCache>>> = OnceLock::new();
    static SPEECH: OnceLock<Mutex<Option<CapCache>>> = OnceLock::new();
    match kind {
        CapKind::Ocr => OCR.get_or_init(|| Mutex::new(None)),
        CapKind::Speech => SPEECH.get_or_init(|| Mutex::new(None)),
    }
}

// Whether this kind's listing failure already reached the crash log this session: a
// machine that always refuses the unelevated query would otherwise log the same multi-line
// PowerShell/DISM text on every poll.
fn caps_log_once(kind: CapKind) -> &'static AtomicBool {
    static OCR: OnceLock<AtomicBool> = OnceLock::new();
    static SPEECH: OnceLock<AtomicBool> = OnceLock::new();
    match kind {
        CapKind::Ocr => OCR.get_or_init(|| AtomicBool::new(false)),
        CapKind::Speech => SPEECH.get_or_init(|| AtomicBool::new(false)),
    }
}

// PowerShell/DISM failures often put several lines of banner text ahead of the one that
// says what happened: the crash log gets only the first line, the frontend's details
// disclosure the raw text via caps_elevation_error.
pub(super) fn strip_ps_banner(raw: &str) -> &str {
    raw.lines().next().unwrap_or(raw).trim()
}

// The set most recently returned by `capabilities()` is the install allowlist (see
// install_language): a name never enumerated is rejected even if it matches the naming regex.
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

// Runs PowerShell once (no window), parses its JSON and resolves each capability's
// localized display name via the WinRT Language lookup the recognizer uses, so no name
// table is kept here.
fn fetch_capabilities(kind: CapKind) -> Result<Vec<OcrCap>, String> {
    ensure_winrt();
    let _ = fs::create_dir_all(dism_temp_dir());
    // Listing needs no elevated rights per the docs, and with the log-path redirect above that
    // mostly holds, but some Windows builds/policies refuse the read-only query anyway (a DISM
    // log path error on one, a flat "elevated rights required" on another). The only remedy is
    // the elevated path below, so any failure here maps to the same code instead of a
    // locale-specific PowerShell blob; the raw text rides along (second field, "|"-joined) for
    // the details disclosure and the crash log.
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
// The stable code the frontend matches on plus the raw PowerShell/DISM text as a second
// "|"-joined field for the details disclosure. A pure function so the wire format is
// unit-tested.
pub(super) fn caps_elevation_error(raw: &str) -> String {
    format!("ocr-caps-need-elevation|{raw}")
}

// The PowerShell query's JSON -> the UI's list; shared by the unelevated and elevated
// queries so both are localized the same way.
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

// Runs a PowerShell command with no visible window and returns its stdout. The working
// directory is our own always-writable one, not the inherited one: DISM's "could not set
// the current directory" failure had nothing to do with the query itself.
fn run_powershell_command(script: &str) -> Result<String, String> {
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script])
        .current_dir(dism_temp_dir())
        .creation_flags(super::CREATE_NO_WINDOW);
    run_with_deadline(cmd, POWERSHELL_DEADLINE)
}

// A servicing stack that is busy (Windows Update) or hung can keep Get-WindowsCapability
// from ever returning; the dialog must get an answer instead of loading forever.
const POWERSHELL_DEADLINE: std::time::Duration = std::time::Duration::from_secs(120);

// Runs the command to completion or kills it at the deadline. The pipes are drained on their
// own threads so a chatty child cannot fill a pipe and stall before it exits.
pub(crate) fn run_with_deadline(mut cmd: std::process::Command, deadline: std::time::Duration) -> Result<String, String> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    }
    let out_reader = drain(child.stdout.take());
    let err_reader = drain(child.stderr.take());
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status,
            None if started.elapsed() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("timeout".to_string());
            }
            None => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    };
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
        return Err(if stderr.is_empty() { "PowerShell command failed".to_string() } else { stderr });
    }
    Ok(String::from_utf8_lossy(&stdout).to_string())
}

// PowerShell single-quoted-string escaping (doubles an embedded quote). Applied
// unconditionally to the validated capability name and our temp paths, which may contain
// non-ASCII profile-path characters.
fn ps_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

// Every elevated round trip gets unguessable file names. The temp folder is writable by
// anything running as this user, so a name derived from the language tag or process id
// could be created in advance: a planted result file would report an install that never
// happened. The nonce is remembered while the request is in flight so the poll finds its
// own file.
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

// Launches an elevated PowerShell that runs Add-WindowsCapability and writes its
// success/failure (with Windows' error message) to a result file, then returns without
// waiting. The script goes over as an encoded command (see elevated_params), so there is no
// quoting to get wrong: `name` is regex+whitelist-validated by the caller and the result
// path is ours.
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

// The argument string that hands `script` to an elevated PowerShell on its own command
// line. A script file in the shared temp folder could be replaced by any process of this
// user between the UAC prompt and PowerShell opening it, running the replacement with the
// rights just granted. PowerShell takes the script as base64 of its UTF-16LE bytes, so no
// quoting is involved.
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

// Starts `script` through the UAC prompt and returns once the elevated process is launched
// (or the launch is refused). Both elevated paths (installing a pack, and the listing on
// machines that demand rights even for it) go through here, never a bespoke ShellExecuteW.
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
        // SE_ERR_ACCESSDENIED (5) is what ShellExecuteW returns when the user declines the UAC
        // prompt; ERROR_CANCELLED (1223) is the same outcome on some Windows versions. What it
        // means depends on policy, not on whether this account is an administrator (the credential
        // prompt can take a different admin's password). The wire strings keep their names for
        // ui/dictation.js; "no-rights" now means Group Policy denies the prompt outright.
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

// Whether this account's own token can complete "runas" unaided, judged from the token and
// never by matching a localised Windows message. `Limited` is a split-token admin (UAC on):
// the prompt can succeed. `Full` is already elevated. `Default` means no split token (a
// standard account, or an admin with UAC off), so Administrators membership tells them
// apart. `false` does not rule out elevation: a standard account's credential prompt still
// takes a different administrator's password (see `elevation_denied_by_policy`).
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

// Whether Group Policy denies the elevation prompt outright for this account ("Behavior of
// the elevation prompt for standard users" = "Automatically deny", `ConsentPromptBehaviorUser`
// = 0): no dialog appears, so not even a different administrator's password helps and a
// retry cannot fix it. Only meaningful for an account that cannot elevate itself; an
// administrator's prompt is governed by `ConsentPromptBehaviorAdmin`. A missing or
// unreadable key counts as not denied (the default).
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
    // The dialog was declined or cancelled this time; Windows lets its credential prompt take a
    // different administrator's password, so trying again may succeed.
    Retryable,
}
// Pure decision, apart from the registry/token probes so it is unit-testable: which of the
// two results the user sees for a refused or cancelled ShellExecuteW.
pub(super) fn classify_elevation_refusal(denied_by_policy: bool) -> ElevationRefusal {
    if denied_by_policy { ElevationRefusal::PolicyBlocked } else { ElevationRefusal::Retryable }
}

// DISM writes a log to %WINDIR%\Logs\DISM at startup, unwritable there without admin
// rights, so an unprivileged Get-/Add-WindowsCapability fails before the query runs and is
// misread as "needs elevation". Redirecting -LogPath and -ScratchDirectory into our own
// temp folder avoids it.
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

// The nonce is per request, so two windows (allowMultipleInstances) never overwrite each
// other's script/result file for the same kind.
pub(super) const CAPS_TAG: &str = "caps";
pub(super) fn caps_result_path(kind: CapKind, nonce: &str) -> PathBuf {
    std::env::temp_dir().join(brand::TEMP_DIR).join(format!("{}-{}-caps-{}.json", brand::TEMP_PREFIX, kind.slug(), nonce))
}
fn caps_tmp_path(kind: CapKind, nonce: &str) -> PathBuf {
    std::env::temp_dir().join(brand::TEMP_DIR).join(format!("{}-{}-caps-{}.json.tmp", brand::TEMP_PREFIX, kind.slug(), nonce))
}

// The same query fetch_capabilities runs, but through an elevated PowerShell that writes
// its JSON to a temp file. Needed where Windows refuses even the read-only listing to a
// normal user: without a list there is no install allowlist. Starts only when the user
// presses the button in the dialog.
//
// PowerShell writes a `.tmp` sibling and Move-Item -Force's it onto the result path in one
// step, so a poll sees either no file or the complete one, never half a JSON blob.
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

// Reads what the elevated listing wrote. Ok(None) = still running or never started. A
// successful list also fills the cache the normal query fills, so the install allowlist
// works unchanged.
//
// The file is deleted only on a definite terminal outcome (a parseable error or list); an
// empty or unparseable read counts as not ready, since the result may be mid-write.
// Mirrors install_result.
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

// Parses the install result file's text. Windows PowerShell's `Out-File -Encoding utf8`
// prepends a byte-order mark that neither `trim` nor the JSON parser accepts; None = not
// parseable (yet).
pub(super) fn parse_install_result(text: &str) -> Option<OcrInstallResult> {
    let text = text.trim_start_matches('\u{feff}').trim();
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    let error = value.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());
    Some(OcrInstallResult { ok, error })
}

// Reads (and, on a terminal result, deletes) the result file install_language's elevated
// process writes. Deleting keeps later polls from re-triggering the cache invalidation below.
pub(super) fn install_result(kind: CapKind, name: &str) -> Option<OcrInstallResult> {
    let tag = ocr_tag_from_capability_name(kind.prefix(), name).unwrap_or_default();
    // No nonce means no install of this pack is in flight: whatever happens to
    // sit in the temp folder was not written for us.
    let nonce = pending_nonce(kind, &tag)?;
    let path = install_result_path(kind, &nonce);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        // Not written yet: the elevated process is still running or at the UAC prompt.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        // The file is there but unreadable (permissions, sharing violation, disk error). Unlike
        // "not found" this never resolves on its own, so it must not look like "still running"
        // forever. Left on disk for inspection, since a locked file could not be removed either.
        Err(_) => {
            finish_request(kind, &tag);
            return Some(OcrInstallResult { ok: false, error: Some("install-result-unreadable".to_string()) });
        }
    };
    // Content that fails to parse stays pending: Out-File's write is not atomic, so a poll can
    // catch the file mid-write. A permanently corrupt file is caught by the frontend's poll
    // cutoff.
    let OcrInstallResult { ok, error } = parse_install_result(&text)?;
    let _ = fs::remove_file(&path);
    finish_request(kind, &tag);
    if ok {
        // Installation is done: both caches must reflect it immediately. This is where the backend
        // has fresh proof, instead of waiting for the frontend to see state == "Installed" via
        // another round trip.
        *cap_cache(kind).lock().unwrap_or_else(|e| e.into_inner()) = None;
        // The recognition engine cache is OCR-only; a new voice session always resolves its
        // language fresh.
        if kind == CapKind::Ocr {
            engine_cache().lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
    }
    Some(OcrInstallResult { ok, error })
}
