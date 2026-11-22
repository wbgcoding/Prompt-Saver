use super::*;
use std::io::{Read, Write};

// ---------- Local speech recognition: model catalog, PC profile, model files ----------
//
// The models are ggml files (whisper.cpp format) fetched on demand from Hugging Face after the
// user agreed to it, stored in the per-user app folder and verified against a pinned SHA-256
// before they are ever used. Nothing in this file runs recognition; it decides which model
// fits the PC, and keeps the downloaded files honest.

// One pinned download: the file name it is stored under, where it comes from, its exact size
// and its SHA-256 (lower-case hex). The size is also the hard cap while streaming.
#[derive(Clone, Copy)]
pub(crate) struct WhisperAsset {
    pub(crate) file: &'static str,
    pub(crate) url: &'static str,
    pub(crate) bytes: u64,
    pub(crate) sha256: &'static str,
}

// One model the user can pick. `label_key` / `quality_key` are i18n keys the model manager
// resolves; the id is what every command takes.
#[derive(Clone, Copy)]
pub(crate) struct WhisperModel {
    pub(crate) id: &'static str,
    pub(crate) label_key: &'static str,
    pub(crate) quality_key: &'static str,
    pub(crate) asset: WhisperAsset,
}

pub(crate) const WHISPER_BASE: &str = "base";
pub(crate) const WHISPER_SMALL: &str = "small";
pub(crate) const WHISPER_TURBO: &str = "turbo";

// Quantised multilingual models, smallest first. Hashes are the files' own SHA-256 as listed by
// the host; the tiny model used by the tests is never offered here.
pub(crate) static WHISPER_MODELS: [WhisperModel; 3] = [
    WhisperModel {
        id: WHISPER_BASE,
        label_key: "whisperModelBase",
        quality_key: "whisperQualityBase",
        asset: WhisperAsset {
            file: "ggml-base-q5_1.bin",
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin",
            bytes: 59_707_625,
            sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
        },
    },
    WhisperModel {
        id: WHISPER_SMALL,
        label_key: "whisperModelSmall",
        quality_key: "whisperQualitySmall",
        asset: WhisperAsset {
            file: "ggml-small-q5_1.bin",
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
            bytes: 190_085_487,
            sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        },
    },
    WhisperModel {
        id: WHISPER_TURBO,
        label_key: "whisperModelTurbo",
        quality_key: "whisperQualityTurbo",
        asset: WhisperAsset {
            file: "ggml-large-v3-turbo-q5_0.bin",
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
            bytes: 574_041_195,
            sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        },
    },
];

// The Silero voice-activity model that splits speech into phrases. It is tiny and downloads
// with the first model under the same consent.
pub(crate) const WHISPER_VAD: WhisperAsset = WhisperAsset {
    file: "ggml-silero-v5.1.2.bin",
    url: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v5.1.2.bin",
    bytes: 885_098,
    sha256: "29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf",
};

pub(crate) fn whisper_model(id: &str) -> Option<&'static WhisperModel> {
    WHISPER_MODELS.iter().find(|m| m.id == id)
}

// ---------- PC profile and recommendation ----------

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SystemProfile {
    pub(crate) physical_cores: u32,
    pub(crate) logical_cores: u32,
    pub(crate) ram_gb: u32,
    pub(crate) avx2: bool,
}

// Total memory in whole GB, rounded to the nearest: a 16 GB PC reports about 15.9 GB because
// part is reserved for hardware, and must still count as 16.
pub(crate) fn ram_gb_from_bytes(bytes: u64) -> u32 {
    (bytes.saturating_add(1 << 29) >> 30).min(u64::from(u32::MAX)) as u32
}

// Counts physical cores (one record each) and logical processors (set bits of each record's
// affinity masks) in a GetLogicalProcessorInformationEx(RelationProcessorCore) buffer: a run
// of variable-sized records, each starting with its relationship and byte size.
pub(crate) fn count_cores(buf: &[u8]) -> (u32, u32) {
    const RELATION_PROCESSOR_CORE: u32 = 0;
    // Record header (relationship, size) then Flags, EfficiencyClass and 20 reserved bytes
    // put the group count at 30 and the first affinity mask at 32.
    const GROUP_COUNT_AT: usize = 30;
    const MASKS_AT: usize = 32;
    let word = std::mem::size_of::<usize>();
    // One GROUP_AFFINITY: the mask, then the group number and three reserved 16-bit words.
    let stride = word + 8;
    let u32_at = |at: usize| u32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]);
    let (mut physical, mut logical) = (0u32, 0u32);
    let mut off = 0usize;
    while off + 8 <= buf.len() {
        let size = u32_at(off + 4) as usize;
        if size < 8 || off + size > buf.len() {
            break;
        }
        if u32_at(off) == RELATION_PROCESSOR_CORE && size >= MASKS_AT {
            physical += 1;
            let groups = u16::from_le_bytes([buf[off + GROUP_COUNT_AT], buf[off + GROUP_COUNT_AT + 1]]) as usize;
            for g in 0..groups {
                let at = off + MASKS_AT + g * stride;
                if at + word > off + size {
                    break;
                }
                let mut mask = [0u8; 8];
                mask[..word].copy_from_slice(&buf[at..at + word]);
                logical += u64::from_le_bytes(mask).count_ones();
            }
        }
        off += size;
    }
    (physical, logical)
}

#[cfg(windows)]
fn read_core_records() -> Option<Vec<u8>> {
    use windows::Win32::System::SystemInformation::{
        GetLogicalProcessorInformationEx, RelationProcessorCore, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
    };
    let mut len = 0u32;
    // The first call only reports the size the records need.
    unsafe {
        let _ = GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut len);
    }
    if len == 0 {
        return None;
    }
    // u64 words keep the buffer aligned for the record structs the call writes into it.
    let mut words = vec![0u64; (len as usize).div_ceil(8)];
    let ptr = words.as_mut_ptr() as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX;
    unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, Some(ptr), &mut len) }.ok()?;
    let bytes = unsafe { std::slice::from_raw_parts(words.as_ptr() as *const u8, len as usize) };
    Some(bytes.to_vec())
}

#[cfg(windows)]
fn total_ram_bytes() -> Option<u64> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
    Some(status.ullTotalPhys)
}

#[cfg(not(windows))]
fn read_core_records() -> Option<Vec<u8>> {
    None
}

#[cfg(not(windows))]
fn total_ram_bytes() -> Option<u64> {
    None
}

fn has_avx2() -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        std::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        false
    }
}

// What this PC offers. A failed memory query reads as 0 GB, so the recommendation then lands
// on the smallest model; a failed core query falls back to the logical processor count.
pub(crate) fn system_profile() -> SystemProfile {
    static PROFILE: std::sync::OnceLock<SystemProfile> = std::sync::OnceLock::new();
    *PROFILE.get_or_init(|| {
        let fallback = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1);
        let (physical, logical) = read_core_records().map(|b| count_cores(&b)).unwrap_or((0, 0));
        SystemProfile {
            physical_cores: if physical == 0 { fallback } else { physical },
            logical_cores: if logical == 0 { fallback } else { logical },
            ram_gb: total_ram_bytes().map(ram_gb_from_bytes).unwrap_or(0),
            avx2: has_avx2(),
        }
    })
}

// ggml is built without native CPU tuning, which still needs AVX2; on a processor without it
// the first compute kernel would hit an illegal instruction and take the app down.
pub(crate) fn whisper_supported(p: &SystemProfile) -> bool {
    p.avx2
}

// Runs `load` (the first thing that touches ggml) only on a supported PC; otherwise it is never
// called and the start is refused with the CPU code.
#[cfg_attr(not(feature = "whisper"), allow(dead_code))]
pub(crate) fn load_if_supported<T>(p: &SystemProfile, load: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    if whisper_supported(p) {
        load()
    } else {
        Err(VOICE_ERR_CPU.to_string())
    }
}

// The model that suits a PC, from measured speed (seconds of work per second of audio, host
// with 20 physical cores): base 0.07 at 10 threads and 0.10 at 4; small 0.20 at 10 and 0.32 to
// 0.37 at 4; large-v3-turbo about 1.0 at 10 and 1.6 to 2.0 at 4, so no PC reaches a comfortable
// speed with it and it is offered but never recommended. Small is therefore the pick from 4
// physical cores and 8 GB memory with AVX2; anything weaker gets base.
pub(crate) fn recommend(p: &SystemProfile) -> &'static str {
    if !p.avx2 || p.physical_cores < 4 || p.ram_gb < 8 {
        WHISPER_BASE
    } else {
        WHISPER_SMALL
    }
}

// The model a dictation session loads: the chosen one when it is downloaded, else the one that
// suits this PC when it is, else the first downloaded one in catalog order. None = nothing downloaded.
#[cfg_attr(not(feature = "whisper"), allow(dead_code))]
pub(crate) fn pick_session_model(
    downloaded: &dyn Fn(&WhisperModel) -> bool,
    chosen: &str,
    recommended: &str,
) -> Option<&'static WhisperModel> {
    [chosen, recommended]
        .iter()
        .filter_map(|id| whisper_model(id))
        .find(|m| downloaded(m))
        .or_else(|| WHISPER_MODELS.iter().find(|m| downloaded(m)))
}

// ---------- Model files on disk ----------

// Where the models live: the per-user app folder, never the data folder (which can be shared)
// and never beside the exe.
pub(crate) fn whisper_dir() -> Option<PathBuf> {
    local_app_dir().map(|d| d.join("whisper"))
}

fn is_complete(dir: &Path, asset: &WhisperAsset) -> bool {
    fs::metadata(dir.join(asset.file)).map(|m| m.is_file() && m.len() == asset.bytes).unwrap_or(false)
}

// A model counts as downloaded when its file has exactly the pinned size; the hash was checked
// before the file got its final name.
pub(crate) fn model_downloaded(dir: &Path, model: &WhisperModel) -> bool {
    is_complete(dir, &model.asset)
}

pub(crate) fn vad_downloaded(dir: &Path) -> bool {
    is_complete(dir, &WHISPER_VAD)
}

pub(crate) fn any_model_downloaded(dir: &Path) -> bool {
    WHISPER_MODELS.iter().any(|m| model_downloaded(dir, m))
}

// The model a running dictation session holds open: it must not be deleted under it. Set and
// cleared by the session itself.
pub(crate) static WHISPER_ACTIVE_MODEL: Mutex<Option<String>> = Mutex::new(None);

pub(crate) const WHISPER_ERR_BUSY: &str = "whisper-busy";
pub(crate) const WHISPER_ERR_CONSENT: &str = "whisper-consent-missing";
pub(crate) const WHISPER_ERR_UNKNOWN: &str = "whisper-unknown-model";
pub(crate) const WHISPER_ERR_IN_USE: &str = "whisper-model-in-use";
pub(crate) const WHISPER_ERR_TOO_LARGE: &str = "whisper-too-large";
pub(crate) const WHISPER_ERR_DAMAGED: &str = "whisper-damaged";
pub(crate) const WHISPER_ERR_INSECURE: &str = "whisper-insecure-url";
pub(crate) const WHISPER_ERR_CANCELLED: &str = "cancelled";
// The code a dictation start is refused with on a processor without AVX2.
pub(crate) const VOICE_ERR_CPU: &str = "voice-whisper-cpu";
// A downloaded model the engine could not load: intact (most often too little free memory), or
// damaged and removed.
pub(crate) const VOICE_ERR_LOAD: &str = "voice-whisper-load";
pub(crate) const VOICE_ERR_MODEL_DAMAGED: &str = "voice-whisper-damaged";

// Whether a file still has the asset's pinned size and SHA-256. Reads the whole file, so it is
// only used after a failed load, never on the way to a session.
pub(crate) fn file_matches_pin(path: &Path, asset: &WhisperAsset) -> bool {
    let Ok(mut file) = fs::File::open(path) else { return false };
    let Some(want) = hex_decode(asset.sha256) else { return false };
    let mut hasher = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                total += n as u64;
                hasher.update(&buf[..n]);
            }
            Err(_) => return false,
        }
    }
    total == asset.bytes && ct_eq(hasher.finish().as_ref(), &want)
}

// Removes a model's file (and a stale partial download of it) unless `active` names it.
pub(crate) fn delete_model_in(dir: &Path, model: &WhisperModel, active: &Mutex<Option<String>>) -> Result<(), String> {
    if active.lock().unwrap_or_else(|e| e.into_inner()).as_deref() == Some(model.id) {
        return Err(WHISPER_ERR_IN_USE.to_string());
    }
    let part = dir.join(format!("{}.part", model.asset.file));
    for path in [dir.join(model.asset.file), part] {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("delete: {e}")),
        }
    }
    Ok(())
}

// Removes every partial download (`*.part`) in the models folder; only called while no download
// runs. Returns how many files went.
pub(crate) fn sweep_partial_downloads(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "part") && e.path().is_file())
        .filter(|e| fs::remove_file(e.path()).is_ok())
        .count()
}

// ---------- Consent ----------

// How long an accepted download dialog stays valid for the download that follows it.
const WHISPER_CONSENT_TTL_MS: u64 = 120_000;
// The model the dialog was accepted for and when (ms since the epoch).
pub(crate) static WHISPER_CONSENT: Mutex<Option<(String, u64)>> = Mutex::new(None);

// Consumes the recorded consent: true only for the model it was given for, no longer than the
// validity window before `now_ms`. One-shot, so every download needs its own accepted dialog.
pub(crate) fn take_whisper_consent(slot: &Mutex<Option<(String, u64)>>, model: &str, now_ms: u64) -> bool {
    match slot.lock().unwrap_or_else(|e| e.into_inner()).take() {
        Some((id, at)) => id == model && at != 0 && now_ms >= at && now_ms - at <= WHISPER_CONSENT_TTL_MS,
        None => false,
    }
}

// Held for the whole download: a second start is refused instead of sharing one cancel flag.
pub(crate) static WHISPER_DOWNLOAD_RUNNING: AtomicBool = AtomicBool::new(false);
// Set by cancel_whisper_download and polled between chunks.
pub(crate) static WHISPER_DOWNLOAD_CANCEL: AtomicBool = AtomicBool::new(false);

// ---------- Download ----------

// The HTTP client for the model files: https only, which the client also enforces on every
// redirect, a handful of redirects (the host forwards to its CDN), proxy variables ignored
// like the other large downloads, and bounded connect and per-read times so a stalled
// transfer ends while a slow one can still finish.
fn whisper_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .proxy(None)
        .https_only(true)
        .max_redirects(5)
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .build()
        .into()
}

// What the transfer yields: the body and the announced length, if any.
pub(crate) type WhisperBody = (Box<dyn Read>, Option<u64>);

// Test hook (armed only through a command that refuses without the start-up variable): the next
// model download arrives with its first byte flipped, so the pinned hash must reject it.
pub(crate) static WHISPER_CORRUPT_NEXT: AtomicBool = AtomicBool::new(false);

struct FlipFirstByte<R: Read> {
    inner: R,
    flipped: bool,
}

impl<R: Read> Read for FlipFirstByte<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 && !self.flipped {
            buf[0] ^= 0xFF;
            self.flipped = true;
        }
        Ok(n)
    }
}

// Passes the body through, or damages it once when `armed` was set (and clears it).
pub(crate) fn maybe_corrupt(body: WhisperBody, armed: &AtomicBool) -> WhisperBody {
    if armed.swap(false, Ordering::SeqCst) {
        (Box::new(FlipFirstByte { inner: body.0, flipped: false }), body.1)
    } else {
        body
    }
}

pub(crate) fn open_https(url: &str) -> Result<WhisperBody, String> {
    if !url.starts_with("https://") {
        return Err(WHISPER_ERR_INSECURE.to_string());
    }
    let resp = whisper_agent()
        .get(url)
        .header("User-Agent", brand::USER_AGENT)
        .call()
        .map_err(|e| format!("download: {e}"))?;
    let total = resp
        .headers()
        .get("Content-Length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());
    Ok((Box::new(resp.into_body().into_reader()), total))
}

// Removes the partial file on every way out that did not rename it, a panic included.
struct PartFile {
    path: PathBuf,
    keep: bool,
}
impl Drop for PartFile {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_file(&self.path);
        }
    }
}

// Streams one asset into `<file>.part` inside `dir`, hashing while it writes, and gives it its
// final name only after the size and the pinned SHA-256 (constant-time compare) match. A
// cancel, a body larger than the pinned size, a short body, a hash mismatch or any I/O error
// leaves no file behind. `open` is the transfer (a fake one in tests); `progress` gets
// (done, total) whenever the shown whole percent changes.
pub(crate) fn fetch_asset(
    asset: &WhisperAsset,
    dir: &Path,
    cancel: &AtomicBool,
    open: &dyn Fn(&str) -> Result<WhisperBody, String>,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf, String> {
    if cancel.load(Ordering::SeqCst) {
        return Err(WHISPER_ERR_CANCELLED.to_string());
    }
    fs::create_dir_all(dir).map_err(|e| format!("target dir: {e}"))?;
    let final_path = dir.join(asset.file);
    let mut part = PartFile { path: dir.join(format!("{}.part", asset.file)), keep: false };
    let _ = fs::remove_file(&part.path);
    let (mut reader, announced) = open(asset.url)?;
    if announced.is_some_and(|n| n > asset.bytes) {
        return Err(WHISPER_ERR_TOO_LARGE.to_string());
    }
    let mut file = fs::File::create(&part.path).map_err(|e| format!("write: {e}"))?;
    let mut hasher = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    let mut last_pct: Option<u64> = None;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(WHISPER_ERR_CANCELLED.to_string());
        }
        let n = reader.read(&mut buf).map_err(|e| format!("read: {e}"))?;
        if n == 0 {
            break;
        }
        done += n as u64;
        if done > asset.bytes {
            return Err(WHISPER_ERR_TOO_LARGE.to_string());
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| format!("write: {e}"))?;
        let pct = done * 100 / asset.bytes.max(1);
        if last_pct != Some(pct) {
            last_pct = Some(pct);
            progress(done, asset.bytes);
        }
    }
    file.flush().map_err(|e| format!("write: {e}"))?;
    drop(file);
    if cancel.load(Ordering::SeqCst) {
        return Err(WHISPER_ERR_CANCELLED.to_string());
    }
    let want = hex_decode(asset.sha256).ok_or_else(|| "bad pinned hash".to_string())?;
    if done != asset.bytes || !ct_eq(hasher.finish().as_ref(), &want) {
        return Err(WHISPER_ERR_DAMAGED.to_string());
    }
    fs::rename(&part.path, &final_path).map_err(|e| format!("write: {e}"))?;
    part.keep = true;
    Ok(final_path)
}

// Installs one model: first the voice-activity file when it is missing (the same consent
// covers it), then the model itself. `progress` reports the model's bytes only; the voice file
// is under a megabyte.
pub(crate) fn install_model(
    dir: &Path,
    vad: &WhisperAsset,
    model: &WhisperAsset,
    cancel: &AtomicBool,
    open: &dyn Fn(&str) -> Result<WhisperBody, String>,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    if !is_complete(dir, vad) {
        fetch_asset(vad, dir, cancel, open, &mut |_, _| {})?;
    }
    if !is_complete(dir, model) {
        fetch_asset(model, dir, cancel, open, progress)?;
    }
    Ok(())
}
