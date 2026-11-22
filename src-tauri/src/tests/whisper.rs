use super::*;
use crate::whisper_cmds::*;
use std::cell::Cell;
use std::io::{Cursor, Read};

// ---- helpers: no network anywhere in this file ----

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ps-whisper-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn data(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect()
}

// An asset whose pinned size and hash describe `body`, so a fake transfer of `body` verifies.
fn asset_for(file: &'static str, body: &[u8]) -> WhisperAsset {
    let hash: &'static str = Box::leak(hex_lower(&sha256(body)).into_boxed_str());
    WhisperAsset { file, url: "https://example.invalid/model.bin", bytes: body.len() as u64, sha256: hash }
}

fn serve(body: Vec<u8>) -> impl Fn(&str) -> Result<WhisperBody, String> {
    move |_| Ok((Box::new(Cursor::new(body.clone())) as Box<dyn Read>, Some(body.len() as u64)))
}

fn parts_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> =
        fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    names.sort();
    names
}

fn profile(physical: u32, ram_gb: u32, avx2: bool) -> SystemProfile {
    SystemProfile { physical_cores: physical, logical_cores: physical * 2, ram_gb, avx2 }
}

// ---- recommendation ----

#[test]
fn recommendation_table_covers_every_boundary() {
    let cases = [
        // (physical cores, GB, avx2, expected)
        (16, 64, true, WHISPER_SMALL),
        (8, 16, true, WHISPER_SMALL),
        (8, 15, true, WHISPER_SMALL),
        (7, 16, true, WHISPER_SMALL),
        (7, 64, true, WHISPER_SMALL),
        (8, 8, true, WHISPER_SMALL),
        (6, 16, true, WHISPER_SMALL),
        (4, 8, true, WHISPER_SMALL),
        (4, 7, true, WHISPER_BASE),
        (3, 16, true, WHISPER_BASE),
        (3, 64, true, WHISPER_BASE),
        (2, 4, true, WHISPER_BASE),
        (1, 2, true, WHISPER_BASE),
        (16, 64, false, WHISPER_BASE),
        (4, 8, false, WHISPER_BASE),
        (0, 0, true, WHISPER_BASE),
    ];
    for (cores, ram, avx2, want) in cases {
        assert_eq!(recommend(&profile(cores, ram, avx2)), want, "{cores} cores, {ram} GB, avx2 {avx2}");
    }
}

#[test]
fn the_slowest_model_is_never_recommended() {
    for cores in [0, 4, 8, 20, 64] {
        for ram in [0, 8, 16, 128] {
            for avx2 in [false, true] {
                assert_ne!(recommend(&profile(cores, ram, avx2)), WHISPER_TURBO);
            }
        }
    }
}

#[test]
fn every_recommendation_is_a_catalog_model() {
    for cores in [0, 1, 4, 8, 32] {
        for ram in [0, 8, 16, 128] {
            for avx2 in [false, true] {
                assert!(whisper_model(recommend(&profile(cores, ram, avx2))).is_some());
            }
        }
    }
}

#[test]
fn memory_rounds_to_the_nearest_whole_gb() {
    let gb = 1u64 << 30;
    assert_eq!(ram_gb_from_bytes(0), 0);
    assert_eq!(ram_gb_from_bytes(8 * gb), 8);
    // A 16 GB PC reports a little less than 16 GB; a 7.4 GB one stays below the 8 GB line.
    assert_eq!(ram_gb_from_bytes(16 * gb - 300 * 1024 * 1024), 16);
    assert_eq!(ram_gb_from_bytes(8 * gb - 200 * 1024 * 1024), 8);
    assert_eq!(ram_gb_from_bytes(7 * gb + gb / 4), 7);
    assert_eq!(ram_gb_from_bytes(u64::MAX), u32::MAX);
}

// One RelationProcessorCore record: header, flags, efficiency class, reserved bytes, group
// count and the affinity masks (mask, group, three reserved words each).
fn core_record(relation: u32, masks: &[u64]) -> Vec<u8> {
    let word = std::mem::size_of::<usize>();
    let mut rec = Vec::new();
    rec.extend_from_slice(&relation.to_le_bytes());
    rec.extend_from_slice(&[0u8; 4]);
    rec.extend_from_slice(&[0u8; 22]);
    rec.extend_from_slice(&(masks.len() as u16).to_le_bytes());
    // Pad up to the first mask (offset 32).
    rec.resize(32, 0);
    for m in masks {
        rec.extend_from_slice(&m.to_le_bytes()[..word]);
        rec.extend_from_slice(&[0u8; 8]);
    }
    let size = rec.len() as u32;
    rec[4..8].copy_from_slice(&size.to_le_bytes());
    rec
}

#[test]
fn core_records_count_physical_cores_and_logical_processors() {
    let mut buf = Vec::new();
    buf.extend(core_record(0, &[0b11]));
    buf.extend(core_record(0, &[0b1100]));
    // A cache record in the middle is skipped.
    buf.extend(core_record(2, &[0xFF]));
    buf.extend(core_record(0, &[0b1_0000]));
    assert_eq!(count_cores(&buf), (3, 5));
}

#[test]
fn core_records_with_no_smt_or_garbage_never_panic() {
    assert_eq!(count_cores(&[]), (0, 0));
    assert_eq!(count_cores(&[1, 2, 3]), (0, 0));
    let one = core_record(0, &[1]);
    assert_eq!(count_cores(&one), (1, 1));
    // A truncated record, and one whose size field lies, stop the walk.
    assert_eq!(count_cores(&one[..20]), (0, 0));
    let mut bad = one.clone();
    bad[4..8].copy_from_slice(&9999u32.to_le_bytes());
    assert_eq!(count_cores(&bad), (0, 0));
    let mut zero = one.clone();
    zero[4..8].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(count_cores(&zero), (0, 0));
}

#[test]
fn this_pc_reports_a_sane_profile() {
    let p = system_profile();
    assert!(p.physical_cores >= 1);
    assert!(p.logical_cores >= p.physical_cores);
    #[cfg(windows)]
    assert!(p.ram_gb >= 1);
}

// ---- catalog ----

#[test]
fn catalog_entries_are_pinned_and_distinct() {
    let mut ids = std::collections::HashSet::new();
    let mut files = std::collections::HashSet::new();
    for m in WHISPER_MODELS.iter() {
        assert!(ids.insert(m.id), "duplicate id {}", m.id);
        assert!(files.insert(m.asset.file), "duplicate file {}", m.asset.file);
        assert!(m.asset.url.starts_with("https://huggingface.co/"), "{}", m.asset.url);
        assert!(m.asset.url.ends_with(m.asset.file));
        assert_eq!(m.asset.sha256.len(), 64);
        assert!(hex_decode(m.asset.sha256).is_some());
        assert!(m.asset.bytes > 1_000_000);
        assert!(m.label_key.starts_with("whisperModel") && m.quality_key.starts_with("whisperQuality"));
        // Nothing may name a path: a file name is one plain component.
        assert!(!m.asset.file.contains(['/', '\\', ':']));
    }
    assert_eq!(WHISPER_VAD.sha256.len(), 64);
    assert!(WHISPER_VAD.url.starts_with("https://huggingface.co/"));
    assert!(!files.contains(WHISPER_VAD.file));
    assert!(whisper_model("tiny").is_none());
    assert!(whisper_model("../base").is_none());
    assert!(whisper_model("").is_none());
    assert_eq!(whisper_model("small").map(|m| m.asset.file), Some("ggml-small-q5_1.bin"));
}

#[test]
fn model_dir_is_the_whisper_folder_in_the_app_folder() {
    let dir = whisper_dir().expect("a local app folder in the test sandbox");
    assert_eq!(dir.file_name().and_then(|n| n.to_str()), Some("whisper"));
    assert_eq!(dir.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()), Some(brand::DATA_FOLDER));
}

#[test]
fn catalog_marks_only_complete_files_as_downloaded() {
    let dir = scratch("catalog");
    let base = whisper_model(WHISPER_BASE).unwrap();
    let small = whisper_model(WHISPER_SMALL).unwrap();
    // A file of the wrong size is not a model; a part file never counts.
    fs::write(dir.join(small.asset.file), b"short").unwrap();
    fs::write(dir.join(format!("{}.part", base.asset.file)), b"x").unwrap();
    let cat = build_catalog(Some(&dir), profile(8, 16, true));
    assert!(cat.models.iter().all(|m| !m.downloaded));
    assert_eq!(cat.recommended, WHISPER_SMALL);
    assert_eq!(cat.profile, profile(8, 16, true));
    assert!(!cat.vad_downloaded);
    assert!(!any_model_downloaded(&dir));
    // A sparse file of exactly the pinned size does.
    let f = fs::File::create(dir.join(base.asset.file)).unwrap();
    f.set_len(base.asset.bytes).unwrap();
    drop(f);
    let cat = build_catalog(Some(&dir), profile(2, 4, true));
    let flags: Vec<(&str, bool)> = cat.models.iter().map(|m| (m.id, m.downloaded)).collect();
    assert_eq!(flags, vec![(WHISPER_BASE, true), (WHISPER_SMALL, false), (WHISPER_TURBO, false)]);
    assert!(any_model_downloaded(&dir));
    let none = build_catalog(None, profile(2, 4, true));
    assert!(none.models.iter().all(|m| !m.downloaded) && !none.vad_downloaded);
    let _ = fs::remove_dir_all(&dir);
}

// ---- consent ----

#[test]
fn download_needs_a_fresh_consent_for_that_model() {
    let slot: Mutex<Option<(String, u64)>> = Mutex::new(None);
    // No dialog accepted: refused.
    assert!(!take_whisper_consent(&slot, "small", 1_000));
    // Accepted, used once.
    *slot.lock().unwrap() = Some(("small".into(), 1_000));
    assert!(take_whisper_consent(&slot, "small", 1_500));
    assert!(!take_whisper_consent(&slot, "small", 1_501));
    // Accepted for another model: refused, and the consent is spent.
    *slot.lock().unwrap() = Some(("base".into(), 1_000));
    assert!(!take_whisper_consent(&slot, "small", 1_100));
    assert!(!take_whisper_consent(&slot, "base", 1_100));
    // Too old, and from the future (clock moved back).
    *slot.lock().unwrap() = Some(("small".into(), 1_000));
    assert!(!take_whisper_consent(&slot, "small", 1_000 + 120_001));
    *slot.lock().unwrap() = Some(("small".into(), 1_000));
    assert!(take_whisper_consent(&slot, "small", 1_000 + 120_000));
    *slot.lock().unwrap() = Some(("small".into(), 5_000));
    assert!(!take_whisper_consent(&slot, "small", 4_000));
}

#[test]
fn granting_consent_for_an_unknown_model_is_refused() {
    assert_eq!(grant_whisper_download_consent("tiny".into()), Err(WHISPER_ERR_UNKNOWN.to_string()));
    assert_eq!(grant_whisper_download_consent("../../x".into()), Err(WHISPER_ERR_UNKNOWN.to_string()));
}

// ---- download core ----

#[test]
fn a_matching_download_is_installed_and_reports_progress() {
    let dir = scratch("ok");
    let body = data(1_000_000, 7);
    let asset = asset_for("ok.bin", &body);
    let cancel = AtomicBool::new(false);
    let mut seen: Vec<(u64, u64)> = Vec::new();
    let path = fetch_asset(&asset, &dir, &cancel, &serve(body.clone()), &mut |d, t| seen.push((d, t))).unwrap();
    assert_eq!(fs::read(&path).unwrap(), body);
    assert_eq!(parts_in(&dir), vec!["ok.bin".to_string()]);
    assert_eq!(seen.last().copied(), Some((body.len() as u64, body.len() as u64)));
    assert!(seen.windows(2).all(|w| w[0].0 < w[1].0), "progress only moves forward");
    assert!(seen.iter().all(|&(d, t)| d <= t));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_hash_mismatch_discards_everything() {
    let dir = scratch("hash");
    let body = data(300_000, 1);
    let asset = asset_for("bad.bin", &body);
    let mut tampered = body.clone();
    tampered[1234] ^= 0x01;
    let cancel = AtomicBool::new(false);
    let r = fetch_asset(&asset, &dir, &cancel, &serve(tampered), &mut |_, _| {});
    assert_eq!(r, Err(WHISPER_ERR_DAMAGED.to_string()));
    assert!(parts_in(&dir).is_empty(), "no part and no final file: {:?}", parts_in(&dir));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_short_body_is_damaged_not_installed() {
    let dir = scratch("short");
    let body = data(300_000, 2);
    let asset = asset_for("short.bin", &body);
    let cancel = AtomicBool::new(false);
    let r = fetch_asset(&asset, &dir, &cancel, &serve(body[..299_000].to_vec()), &mut |_, _| {});
    assert_eq!(r, Err(WHISPER_ERR_DAMAGED.to_string()));
    assert!(parts_in(&dir).is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_body_past_the_pinned_size_aborts_and_leaves_nothing() {
    let dir = scratch("over");
    let body = data(300_000, 3);
    let asset = asset_for("over.bin", &body);
    let cancel = AtomicBool::new(false);
    // A server that keeps sending after the pinned size, with no (or a lying) length header.
    let mut longer = body.clone();
    longer.extend(data(500_000, 9));
    let silent = move |_: &str| -> Result<WhisperBody, String> { Ok((Box::new(Cursor::new(longer.clone())), None)) };
    assert_eq!(fetch_asset(&asset, &dir, &cancel, &silent, &mut |_, _| {}), Err(WHISPER_ERR_TOO_LARGE.to_string()));
    assert!(parts_in(&dir).is_empty());
    // An announced length above the pinned size is refused before anything is written.
    let huge = move |_: &str| -> Result<WhisperBody, String> { Ok((Box::new(Cursor::new(Vec::new())), Some(u64::MAX))) };
    assert_eq!(fetch_asset(&asset, &dir, &cancel, &huge, &mut |_, _| {}), Err(WHISPER_ERR_TOO_LARGE.to_string()));
    assert!(parts_in(&dir).is_empty());
    let _ = fs::remove_dir_all(&dir);
}

// A body that asks for the download to be cancelled once it has handed out some bytes.
struct CancelsAfter {
    inner: Cursor<Vec<u8>>,
    flag: Arc<AtomicBool>,
    after: usize,
    served: usize,
}
impl Read for CancelsAfter {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.served += n;
        if self.served >= self.after {
            self.flag.store(true, Ordering::SeqCst);
        }
        Ok(n)
    }
}

#[test]
fn a_cancel_in_the_middle_removes_the_part_file() {
    let dir = scratch("cancel");
    let body = data(2_000_000, 4);
    let asset = asset_for("cancel.bin", &body);
    let flag = Arc::new(AtomicBool::new(false));
    let f2 = flag.clone();
    let open = move |_: &str| -> Result<WhisperBody, String> {
        Ok((
            Box::new(CancelsAfter { inner: Cursor::new(body.clone()), flag: f2.clone(), after: 600_000, served: 0 }),
            Some(2_000_000),
        ))
    };
    let r = fetch_asset(&asset, &dir, &flag, &open, &mut |_, _| {});
    assert_eq!(r, Err(WHISPER_ERR_CANCELLED.to_string()));
    assert!(parts_in(&dir).is_empty(), "left behind: {:?}", parts_in(&dir));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_cancel_requested_before_the_start_never_opens_a_connection() {
    let dir = scratch("precancel");
    let asset = asset_for("pre.bin", b"abc");
    let cancel = AtomicBool::new(true);
    let opened = Cell::new(0);
    let open = |_: &str| -> Result<WhisperBody, String> {
        opened.set(opened.get() + 1);
        Err("must not be called".to_string())
    };
    assert_eq!(fetch_asset(&asset, &dir, &cancel, &open, &mut |_, _| {}), Err(WHISPER_ERR_CANCELLED.to_string()));
    assert_eq!(opened.get(), 0);
    assert!(parts_in(&dir).is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_failing_transfer_and_a_stale_part_file_leave_a_clean_folder() {
    let dir = scratch("fail");
    let body = data(50_000, 5);
    let asset = asset_for("fail.bin", &body);
    let cancel = AtomicBool::new(false);
    // A leftover from a crashed earlier run is replaced, not appended to.
    fs::write(dir.join("fail.bin.part"), b"stale bytes").unwrap();
    let down = |_: &str| -> Result<WhisperBody, String> { Err("download: connection refused".to_string()) };
    assert_eq!(
        fetch_asset(&asset, &dir, &cancel, &down, &mut |_, _| {}),
        Err("download: connection refused".to_string())
    );
    assert!(parts_in(&dir).is_empty());
    fs::write(dir.join("fail.bin.part"), b"stale bytes").unwrap();
    fetch_asset(&asset, &dir, &cancel, &serve(body.clone()), &mut |_, _| {}).unwrap();
    assert_eq!(fs::read(dir.join("fail.bin")).unwrap(), body);
    assert_eq!(parts_in(&dir), vec!["fail.bin".to_string()]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_read_error_halfway_discards_the_part_file() {
    struct Breaks(usize);
    impl Read for Breaks {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.0 == 0 {
                return Err(std::io::Error::other("connection reset"));
            }
            self.0 -= 1;
            buf[..10].fill(1);
            Ok(10)
        }
    }
    let dir = scratch("reset");
    let asset = asset_for("reset.bin", &data(10_000, 6));
    let cancel = AtomicBool::new(false);
    let open = |_: &str| -> Result<WhisperBody, String> { Ok((Box::new(Breaks(3)), None)) };
    let r = fetch_asset(&asset, &dir, &cancel, &open, &mut |_, _| {});
    assert!(r.unwrap_err().starts_with("read:"));
    assert!(parts_in(&dir).is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn plain_http_is_refused_before_any_request() {
    for url in ["http://huggingface.co/x.bin", "ftp://host/x.bin", "file:///C:/x.bin", ""] {
        assert!(matches!(open_https(url), Err(e) if e == WHISPER_ERR_INSECURE), "{url}");
    }
}

#[test]
fn the_voice_file_comes_first_and_only_when_missing() {
    let dir = scratch("install");
    let vad_body = data(20_000, 11);
    let model_body = data(400_000, 12);
    let vad = asset_for("vad.bin", &vad_body);
    let model = asset_for("model.bin", &model_body);
    let cancel = AtomicBool::new(false);
    let order: std::cell::RefCell<Vec<usize>> = std::cell::RefCell::new(Vec::new());
    let (vb, mb) = (vad_body.clone(), model_body.clone());
    let open = |_: &str| -> Result<WhisperBody, String> {
        // The two assets share a url here; tell them apart by call order.
        let n = order.borrow().len();
        order.borrow_mut().push(n);
        let body = if n == 0 { vb.clone() } else { mb.clone() };
        Ok((Box::new(Cursor::new(body)), None))
    };
    install_model(&dir, &vad, &model, &cancel, &open, &mut |_, _| {}).unwrap();
    assert_eq!(parts_in(&dir), vec!["model.bin".to_string(), "vad.bin".to_string()]);
    // Both present: nothing is fetched again.
    install_model(&dir, &vad, &model, &cancel, &open, &mut |_, _| {}).unwrap();
    assert_eq!(order.borrow().len(), 2);
    // Only the model gone: only the model is fetched.
    fs::remove_file(dir.join("model.bin")).unwrap();
    let again = |_: &str| -> Result<WhisperBody, String> { Ok((Box::new(Cursor::new(model_body.clone())), None)) };
    install_model(&dir, &vad, &model, &cancel, &again, &mut |_, _| {}).unwrap();
    assert_eq!(parts_in(&dir), vec!["model.bin".to_string(), "vad.bin".to_string()]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_damaged_voice_file_stops_the_install_before_the_model() {
    let dir = scratch("vadbad");
    let vad = asset_for("vad.bin", &data(20_000, 1));
    let model = asset_for("model.bin", &data(40_000, 2));
    let cancel = AtomicBool::new(false);
    let calls = Cell::new(0);
    let open = |_: &str| -> Result<WhisperBody, String> {
        calls.set(calls.get() + 1);
        Ok((Box::new(Cursor::new(data(20_000, 99))), None))
    };
    assert_eq!(
        install_model(&dir, &vad, &model, &cancel, &open, &mut |_, _| {}),
        Err(WHISPER_ERR_DAMAGED.to_string())
    );
    assert_eq!(calls.get(), 1);
    assert!(parts_in(&dir).is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn only_one_download_runs_at_a_time() {
    // The claim is process-wide; this is the only test that takes it.
    let first = FlagClaim::take(&WHISPER_DOWNLOAD_RUNNING).expect("free");
    assert!(FlagClaim::take(&WHISPER_DOWNLOAD_RUNNING).is_none());
    assert!(build_catalog(None, profile(4, 8, true)).downloading);
    drop(first);
    assert!(FlagClaim::take(&WHISPER_DOWNLOAD_RUNNING).is_some());
}

// ---- delete ----

#[test]
fn a_model_in_use_cannot_be_deleted() {
    let dir = scratch("inuse");
    let m = whisper_model(WHISPER_SMALL).unwrap();
    fs::write(dir.join(m.asset.file), b"model bytes").unwrap();
    let active: Mutex<Option<String>> = Mutex::new(Some(WHISPER_SMALL.to_string()));
    assert_eq!(delete_model_in(&dir, m, &active), Err(WHISPER_ERR_IN_USE.to_string()));
    assert!(dir.join(m.asset.file).exists());
    // A session on another model does not block this one.
    *active.lock().unwrap() = Some(WHISPER_BASE.to_string());
    assert_eq!(delete_model_in(&dir, m, &active), Ok(()));
    assert!(!dir.join(m.asset.file).exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn deleting_removes_the_file_and_a_stale_part_and_tolerates_absence() {
    let dir = scratch("delete");
    let m = whisper_model(WHISPER_BASE).unwrap();
    let other = whisper_model(WHISPER_SMALL).unwrap();
    fs::write(dir.join(m.asset.file), b"a").unwrap();
    fs::write(dir.join(format!("{}.part", m.asset.file)), b"b").unwrap();
    fs::write(dir.join(other.asset.file), b"c").unwrap();
    fs::write(dir.join(WHISPER_VAD.file), b"v").unwrap();
    let idle: Mutex<Option<String>> = Mutex::new(None);
    assert_eq!(delete_model_in(&dir, m, &idle), Ok(()));
    assert_eq!(delete_model_in(&dir, m, &idle), Ok(()));
    // The other model and the shared voice file stay.
    let mut left = vec![other.asset.file.to_string(), WHISPER_VAD.file.to_string()];
    left.sort();
    assert_eq!(parts_in(&dir), left);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_model_file_matches_its_pin_only_with_the_right_bytes() {
    let dir = scratch("pin");
    // SHA-256 of "abc".
    let asset = WhisperAsset { file: "t.bin", url: "", bytes: 3, sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" };
    let path = dir.join(asset.file);
    assert!(!file_matches_pin(&path, &asset), "a missing file matches");
    fs::write(&path, b"abc").unwrap();
    assert!(file_matches_pin(&path, &asset));
    fs::write(&path, b"abd").unwrap();
    assert!(!file_matches_pin(&path, &asset), "same size, other bytes");
    fs::write(&path, b"abcd").unwrap();
    assert!(!file_matches_pin(&path, &asset), "other size");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_sweep_removes_only_partial_downloads() {
    let dir = scratch("sweep");
    let m = whisper_model(WHISPER_BASE).unwrap();
    fs::write(dir.join(m.asset.file), b"a").unwrap();
    fs::write(dir.join(format!("{}.part", m.asset.file)), b"b").unwrap();
    fs::write(dir.join("other.bin.part"), b"c").unwrap();
    fs::write(dir.join(WHISPER_VAD.file), b"v").unwrap();
    assert_eq!(sweep_partial_downloads(&dir), 2);
    let mut left = vec![m.asset.file.to_string(), WHISPER_VAD.file.to_string()];
    left.sort();
    assert_eq!(parts_in(&dir), left);
    assert_eq!(sweep_partial_downloads(&dir), 0);
    assert_eq!(sweep_partial_downloads(&dir.join("missing")), 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_active_model_slot_starts_empty_and_survives_a_poisoned_lock() {
    assert!(WHISPER_ACTIVE_MODEL.lock().unwrap_or_else(|e| e.into_inner()).is_none());
    let slot: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(Some(WHISPER_BASE.to_string())));
    let s2 = slot.clone();
    let _ = std::thread::spawn(move || {
        let _g = s2.lock().unwrap();
        panic!("poison");
    })
    .join();
    let dir = scratch("poison");
    let m = whisper_model(WHISPER_BASE).unwrap();
    assert_eq!(delete_model_in(&dir, m, &slot), Err(WHISPER_ERR_IN_USE.to_string()));
    let _ = fs::remove_dir_all(&dir);
}

// ---- engine setting ----

#[test]
fn only_the_first_download_switches_the_engine() {
    assert_eq!(engine_after_download(true, None), Some("whisper"));
    assert_eq!(engine_after_download(true, Some("windows")), Some("whisper"));
    assert_eq!(engine_after_download(true, Some("whisper")), None);
    assert_eq!(engine_after_download(false, None), None);
    assert_eq!(engine_after_download(false, Some("windows")), None);
    assert_eq!(VOICE_ENGINE_KEY, "voiceEngine");
}

// ---- processor guard ----

#[test]
fn a_pc_without_avx2_is_refused_and_no_engine_is_constructed() {
    let loads = Cell::new(0u32);
    let fake_engine = || {
        loads.set(loads.get() + 1);
        Ok("engine")
    };
    let refused = load_if_supported(&profile(16, 32, false), fake_engine);
    assert_eq!(refused, Err("voice-whisper-cpu".to_string()));
    assert_eq!(loads.get(), 0, "ggml must not be touched on a processor without AVX2");
    assert_eq!(load_if_supported(&profile(16, 32, true), fake_engine), Ok("engine"));
    assert_eq!(loads.get(), 1);
}

#[test]
fn the_catalog_reports_whether_the_engine_can_run_here() {
    assert!(!build_catalog(None, profile(16, 32, false)).supported);
    assert!(build_catalog(None, profile(16, 32, true)).supported);
    assert_eq!(VOICE_ERR_CPU, "voice-whisper-cpu");
}
