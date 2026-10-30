use super::*;

// The checks below compare multi-line source text; a checkout with Windows line endings
// must read the same as one with Unix endings.
macro_rules! source {
    ($path:literal) => {
        include_str!($path).replace("\r\n", "\n")
    };
}

#[test]
fn a_very_long_waveform_never_passes_the_canvas_cap() {
    let video = source!("../../../ui/video.js");
    assert!(video.contains("Math.min(window.devicePixelRatio || 1, WAVE_MAX_PX / width)"));
    assert!(!video.contains("Math.max(1, WAVE_MAX_PX / width)"));
}

#[test]
fn a_trim_with_only_an_end_still_limits_the_export_length() {
    assert_eq!(trim_duration_secs(Some(2000), Some(5000)), Some(3.0));
    assert_eq!(trim_duration_secs(None, Some(5000)), Some(5.0));
    assert_eq!(trim_duration_secs(Some(0), Some(5000)), Some(5.0));
    assert_eq!(trim_duration_secs(Some(2000), None), None);
    assert_eq!(trim_duration_secs(Some(5000), Some(5000)), None);
    assert_eq!(trim_duration_secs(Some(6000), Some(5000)), None);
}

#[test]
fn editor_decision_follows_the_editors_answer() {
    use std::sync::mpsc::channel;
    let short = Duration::from_millis(50);
    let (tx, rx) = channel::<String>();
    tx.send("accepted".into()).unwrap();
    assert_eq!(wait_for_editor_decision(&rx, short, short), EditorLoadDecision::Accepted);
    tx.send("refused".into()).unwrap();
    assert_eq!(wait_for_editor_decision(&rx, short, short), EditorLoadDecision::Refused);
    // A page that is still booting never answers.
    assert_eq!(wait_for_editor_decision(&rx, short, short), EditorLoadDecision::NoAnswer);
    // An unsaved-edits question gets the long window; an unanswered one is a refusal.
    tx.send("asking".into()).unwrap();
    tx.send("refused".into()).unwrap();
    assert_eq!(wait_for_editor_decision(&rx, short, short), EditorLoadDecision::Refused);
    tx.send("asking".into()).unwrap();
    assert_eq!(wait_for_editor_decision(&rx, short, short), EditorLoadDecision::Refused);
}

#[test]
fn export_source_is_allowed_only_for_own_recordings_work_copies_and_the_open_editor_file() {
    let root = std::env::temp_dir().join(format!("ps-exportsrc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let rec_dir = root.join("recs");
    let temp_root = root.join("tmp");
    let work = temp_root.join(format!("{}-openedit-1-2", brand::TEMP_PREFIX));
    let elsewhere = root.join("elsewhere");
    for d in [&rec_dir, &work, &elsewhere] {
        fs::create_dir_all(d).unwrap();
    }
    let rec = rec_dir.join("rec-1.mp4");
    let copy = work.join("clip.mp4");
    let opened = elsewhere.join("opened.mp4");
    let foreign = elsewhere.join("private.mp4");
    for f in [&rec, &copy, &opened, &foreign] {
        fs::write(f, b"v").unwrap();
    }
    let dirs = [rec_dir.clone()];
    let editor = vec![String::new(), opened.to_string_lossy().to_string()];
    assert!(export_source_is_allowed(&dirs, &[], &temp_root, &editor, &rec));
    assert!(export_source_is_allowed(&dirs, &[], &temp_root, &[], &copy), "an editor work copy");
    assert!(export_source_is_allowed(&dirs, &[], &temp_root, &editor, &opened), "the file the editor was handed");
    assert!(!export_source_is_allowed(&dirs, &[], &temp_root, &editor, &foreign), "any other readable video");
    assert!(!export_source_is_allowed(&dirs, &[], &temp_root, &[], &opened), "nothing handed to the editor");
    assert!(!export_source_is_allowed(&dirs, &[], &temp_root, &[String::new()], &foreign));
    assert!(!export_source_is_allowed(&dirs, &[], &temp_root, &editor, &elsewhere.join("gone.mp4")));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn export_dest_is_usable_refuses_device_names_bad_characters_and_the_source_itself() {
    let dir = std::env::temp_dir().join(format!("ps-batch-dest-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("clip.mp4");
    std::fs::write(&src, b"x").unwrap();
    assert!(export_dest_is_usable(&dir.join("out.mp4"), &src));
    assert!(!export_dest_is_usable(&dir.join("CON"), &src));
    assert!(!export_dest_is_usable(&dir.join("bad?.mp4"), &src));
    assert!(!export_dest_is_usable(&dir.join("CLIP.MP4"), &src));
    assert!(!export_dest_is_usable(&src, &src));
    let _ = std::fs::remove_dir_all(&dir);
}

pub(super) fn waveform_test_pcm(samples: usize, seed: u64) -> Vec<u8> {
    let mut state = seed;
    let mut out = Vec::with_capacity(samples * 2);
    for i in 0..samples {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let mut v = (state >> 48) as i16;
        // A few exact extremes so the i16::MIN case is part of the data.
        if i % 1_000_003 == 7 {
            v = i16::MIN;
        }
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

pub(super) fn stream_peaks(pcm: &[u8], total_samples: u64, buckets: u32, seed: u64) -> Vec<f32> {
    let mut acc = PeakAccumulator::new(total_samples, buckets);
    let mut state = seed;
    let mut at = 0usize;
    while at < pcm.len() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let n = (1 + (state >> 40) as usize % 70_001).min(pcm.len() - at);
        acc.push(&pcm[at..at + n]);
        at += n;
    }
    acc.finish()
}

#[test]
fn streamed_peaks_equal_the_whole_buffer_peaks_for_one_hour_of_audio() {
    let samples = 3600 * WAVEFORM_SAMPLE_RATE as usize;
    let pcm = waveform_test_pcm(samples, 1);
    for buckets in [4000u32, 1234, 8] {
        let want = pcm_s16le_to_peaks(&pcm, buckets);
        let got = stream_peaks(&pcm, samples as u64, buckets, 99 + buckets as u64);
        assert_eq!(got, want, "{buckets} buckets");
    }
}

#[test]
fn streamed_peaks_match_on_uneven_and_tiny_inputs() {
    for (samples, buckets) in [(100_003usize, 777u32), (5, 8), (1, 8), (0, 8), (9_999, 4000), (65_537, 64)] {
        let pcm = waveform_test_pcm(samples, 7 + samples as u64);
        let want = pcm_s16le_to_peaks(&pcm, buckets);
        let got = stream_peaks(&pcm, samples as u64, buckets, 5);
        assert_eq!(got, want, "{samples} samples, {buckets} buckets");
    }
}

#[test]
fn streamed_peaks_fold_samples_past_the_announced_length_into_the_last_bucket() {
    let mut pcm = waveform_test_pcm(1000, 3);
    // A loud sample well past the announced 800.
    pcm.extend_from_slice(&i16::MAX.to_le_bytes());
    let got = stream_peaks(&pcm, 800, 8, 1);
    assert_eq!(got.len(), 8);
    assert!(got[7] > 0.99, "the overrun sample counts in the last bucket: {got:?}");
}

#[test]
fn the_editor_opens_only_known_files() {
    let root = std::env::temp_dir().join(format!("ps-editopen-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let rec_dir = root.join("recs");
    let elsewhere = root.join("elsewhere");
    for d in [&rec_dir, &elsewhere] {
        fs::create_dir_all(d).unwrap();
    }
    let attached = elsewhere.join("clip.mp4");
    let foreign = elsewhere.join("private.mp4");
    for f in [&attached, &foreign] {
        fs::write(f, b"v").unwrap();
    }
    let mut p = test_prompt("a");
    p.file_path = attached.to_string_lossy().to_string();
    let paths = attached_media_paths(&[p, test_prompt("b")]);
    assert_eq!(paths, vec![attached.to_string_lossy().to_string()], "empty paths are not sources");
    let dirs = [rec_dir];
    let temp_root = root.join("tmp");
    assert!(export_source_is_allowed(&dirs, &[], &temp_root, &paths, &attached));
    assert!(!export_source_is_allowed(&dirs, &[], &temp_root, &paths, &foreign), "an arbitrary readable file");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_media_backfill_only_applies_to_the_entry_state_it_measured() {
    let mut p: Prompt = serde_json::from_str(r#"{"id":"a","name":"N","text":"T"}"#).unwrap();
    assert_eq!(media_backfill_target(&p), None, "no attachment, nothing to measure");
    p.file_path = "D:/old.mp4".into();
    let probed = media_backfill_target(&p).unwrap();
    assert_eq!(probed, "D:/old.mp4");
    // The attachment is replaced and measured anew while the probe ran.
    p.file_path = "D:/new.mp4".into();
    assert_ne!(media_backfill_target(&p).as_deref(), Some(probed.as_str()));
    p.media_duration_ms = 4000;
    assert_eq!(media_backfill_target(&p), None, "a measured entry is never overwritten");
    // Structural: the write step re-asks the same question under the second lock.
    let src = source!("../video_editor.rs");
    assert!(src.contains("media_backfill_target(p).as_deref() == Some(media_path.as_str())"));
}

#[test]
fn closing_the_editor_releases_every_waiting_open_request_as_gone() {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    crate::video_editor::VE_LOAD_ACKS.lock().unwrap().get_or_insert_with(std::collections::HashMap::new).insert(u64::MAX - 7, tx);
    *crate::video_editor::VE_PENDING_REQUEST.lock().unwrap() = Some(("a".into(), "b".into(), "c".into()));
    // The caller has been told "asking": it waits the long window for the person's answer.
    let waiter = std::thread::spawn(move || crate::video_editor::wait_for_editor_decision(&rx, std::time::Duration::from_secs(5), std::time::Duration::from_secs(3600)));
    crate::video_editor::VE_LOAD_ACKS.lock().unwrap().as_ref().unwrap()[&(u64::MAX - 7)].send("asking".into()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    crate::video_editor::release_pending_editor_requests();
    assert_eq!(waiter.join().unwrap(), crate::video_editor::EditorLoadDecision::Gone);
    assert!(crate::video_editor::VE_PENDING_REQUEST.lock().unwrap().is_none(), "a stale offer must not outlive its window");
    let src = source!("../video_editor.rs");
    let hook = src.find("tauri::WindowEvent::Destroyed =>").expect("the editor window must watch its own destruction");
    assert!(src[hook..hook + 300].contains("release_pending_editor_requests"));
}

// The open-while-closing race: a request that met a window on its way out must build a fresh
// one instead of being refused or ignored.
#[test]
fn a_request_that_met_a_closing_window_falls_through_to_building_one() {
    use crate::video_editor::{retarget_outcome, EditorLoadDecision as D, RetargetOutcome as O};
    // Destroyed answered "gone", or the window vanished before it could answer.
    assert_eq!(retarget_outcome(&D::Gone, true), O::BuildNew);
    assert_eq!(retarget_outcome(&D::Gone, false), O::BuildNew);
    assert_eq!(retarget_outcome(&D::NoAnswer, false), O::BuildNew);
    assert_eq!(retarget_outcome(&D::Accepted, false), O::BuildNew);
    // A live window keeps the old behaviour: its refusal is final, its acceptance or silence counts as opened.
    assert_eq!(retarget_outcome(&D::Refused, true), O::Done(false));
    assert_eq!(retarget_outcome(&D::Accepted, true), O::Done(true));
    assert_eq!(retarget_outcome(&D::NoAnswer, true), O::Done(true));
    // The wait itself turns the destroy message into the distinct state.
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    tx.send("gone".into()).unwrap();
    let short = Duration::from_millis(50);
    assert_eq!(wait_for_editor_decision(&rx, short, short), D::Gone);
    // Structural: the open command reaches the build path on that outcome and waits for the label to free up.
    let src = source!("../video_editor.rs");
    assert!(src.contains("RetargetOutcome::BuildNew => carried = Some(request)"));
    assert!(src.contains("carried.unwrap_or((src_path, origin_dir, overlay_path))"));
}

#[test]
fn a_close_the_page_cannot_answer_goes_through_and_any_other_is_held_for_it() {
    use crate::video_editor::{editor_close_action, EditorCloseAction as A};
    // Page not listening yet (still booting): nobody could answer, so the close goes through.
    assert_eq!(editor_close_action(false, false), A::Allow);
    // Alt+F4 / taskbar close on a page that listens: held back and handed to the page.
    assert_eq!(editor_close_action(true, false), A::AskPage);
    // The page asked and the person agreed: the one-shot permission lets it through.
    assert_eq!(editor_close_action(true, true), A::Allow);
    // Structural: the handler holds the close back and only flags and emits (a close handler must never
    // create or destroy a window), and a quit asks the editor before the job question.
    let src = source!("../video_editor.rs");
    let at = src.find("tauri::WindowEvent::CloseRequested { api, .. } =>").expect("the editor window must watch close requests");
    let handler = &src[at..at + 700];
    assert!(handler.contains("api.prevent_close()") && handler.contains("ve-close-requested"));
    assert!(!handler.contains("WebviewWindowBuilder") && !handler.contains(".destroy()") && !handler.contains(".close()"));
    let quit = source!("../bg_ops.rs");
    let at = quit.find("pub(crate) fn request_app_quit").unwrap();
    assert!(quit[at..at + 400].contains("ask_editor_before_quit"));
}

// Structural fix, no red unit test: a panic inside the export task cannot be provoked here.
#[test]
fn a_failed_export_task_still_closes_its_job_row() {
    let src = source!("../video_editor.rs");
    let body = src.split("pub(crate) async fn export_video(").nth(1).unwrap().split("
}
").next().unwrap();
    assert!(body.contains("result.unwrap_or_else(|e| Err(e.to_string()))"));
    assert!(!body.contains("result.map_err(|e| e.to_string())?"));
}

#[test]
fn a_superseded_filmstrip_loop_stops_before_its_first_thumbnail() {
    // Structural fix, no red unit test: the stale check sits inside the per-thumbnail loop behind a
    // real ffprobe call, so the source contrast pins the wiring and the call below pins the signature.
    let src = source!("../video_editor.rs");
    assert!(src.contains("is_stale: impl Fn() -> bool"));
    assert!(src.contains("FILMSTRIP_GEN.fetch_add(1, Ordering::SeqCst);
        }"));
    let r = crate::video_editor::generate_filmstrip_blocking(
        std::path::Path::new("nope-ffmpeg"), std::path::Path::new("nope-ffprobe"), std::path::Path::new("nope.mp4"), 3, || true,
    );
    assert!(r.is_err(), "the probe of a missing file fails first");
}

#[test]
fn a_reserved_device_name_with_trailing_spaces_before_the_dot_is_refused() {
    assert!(!crate::video_editor::export_file_name_is_valid("CON .mp4"));
    assert!(!crate::video_editor::export_file_name_is_valid("nul  .take2.mp4"));
    assert!(crate::video_editor::export_file_name_is_valid("CONE .mp4"));
}

// Two batch items that map to one name (rec.mp4 and rec.mkv, both exported to mp4) and a file
// already in the folder must each end up under their own name, nothing replaced.
#[test]
fn batch_exports_with_one_target_name_never_overwrite_each_other() {
    let dir = std::env::temp_dir().join(format!("ps-v24-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let dst = dir.join("rec.mp4");
    fs::write(&dst, b"already there").unwrap();
    let mut placed = Vec::new();
    for body in [&b"first"[..], &b"second"[..]] {
        let staged = dir.join(format!(".export-{}.mp4", placed.len()));
        fs::write(&staged, body).unwrap();
        placed.push(video_editor::place_staged_export(&staged, &dst).unwrap());
    }
    assert_eq!(fs::read(&dst).unwrap(), b"already there");
    assert_eq!(fs::read(dir.join("rec (2).mp4")).unwrap(), b"first");
    assert_eq!(fs::read(dir.join("rec (3).mp4")).unwrap(), b"second");
    assert_ne!(placed[0], placed[1]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn opening_the_editor_vets_the_source_by_text_first_and_off_the_async_worker() {
    let root = std::env::temp_dir().join(format!("ps-openvet-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let rec_dir = root.join("recs");
    let elsewhere = root.join("elsewhere");
    for d in [&rec_dir, &elsewhere] {
        fs::create_dir_all(d).unwrap();
    }
    let rec = rec_dir.join("rec-1.mp4");
    let foreign = elsewhere.join("private.mp4");
    let overlay = elsewhere.join("cam.webm");
    for f in [&rec, &foreign, &overlay] {
        fs::write(f, b"v").unwrap();
    }
    let dirs = [rec_dir.clone()];
    let temp_root = root.join("tmp");
    let vet = |src: &Path, ov: Option<&str>| vet_editor_open(&dirs, &[], &temp_root, &[], src, ov);
    // A network spelling, as the recording or as the overlay, is refused on its text alone.
    assert_eq!(vet(Path::new(r"\\host\share\a.mp4"), None), Err("source-not-allowed"));
    assert_eq!(vet(&rec, Some(r"\\host\share\cam.webm")), Err("source-not-allowed"));
    assert_eq!(vet(&rec, Some("//host/share/cam.webm")), Err("source-not-allowed"));
    // A file the app does not know is refused; a recording that vanished is simply not found.
    assert_eq!(vet(&foreign, None), Err("source-not-allowed"));
    assert_eq!(vet(&rec_dir.join("rec-gone.mp4"), None), Err("recording file not found"));
    // An own recording passes; an overlay that no longer exists is dropped, not an error.
    assert_eq!(vet(&rec, None), Ok(String::new()));
    assert_eq!(vet(&rec, Some(&overlay.to_string_lossy())), Ok(overlay.to_string_lossy().to_string()));
    assert_eq!(vet(&rec, Some(&elsewhere.join("gone.webm").to_string_lossy())), Ok(String::new()));
    // Attachments that name another machine are never canonicalized for the comparison.
    let editor = vec![r"\\host\share\a.mp4".to_string()];
    assert!(!export_source_is_allowed(&dirs, &[], &temp_root, &editor, Path::new(r"\\host\share\a.mp4")));
    let _ = fs::remove_dir_all(&root);

    // The command vets before any is_file call and does the disk work on a blocking worker.
    let ve = source!("../video_editor.rs");
    let at = ve.find("pub(crate) async fn open_video_editor").unwrap();
    let body = &ve[at..];
    let body = &body[..body.find("\n}\n").unwrap()];
    let vet_at = body.find("vet_editor_open(").expect("vets the source");
    assert!(body[..vet_at].contains("spawn_blocking"), "the vetting runs on a blocking worker");
    assert!(!body[..vet_at].contains(".is_file()"), "no disk call before the vetting");
    assert!(body[..vet_at].contains("is_foreign_network_path("), "network spellings refused on text first");
    let allowed = &ve[ve.find("pub(crate) fn export_source_is_allowed").unwrap()..];
    let allowed = &allowed[..allowed.find("\n}\n").unwrap()];
    assert!(allowed.find("is_foreign_network_path(").is_some_and(|n| n < allowed.find("canonicalize(").unwrap()), "text check before the first canonicalize");
}

// A still taken with the playhead on the clip's very end (it just played through) used to fail:
// seeking to the duration leaves ffmpeg no frame to write. Runs the bundled ffmpeg when the
// checkout has it (it is a build input) and skips otherwise.
#[test]
fn still_frame_at_clip_end_falls_back_to_last_frame() {
    let Some(ffmpeg) = super::rec_conversion::test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-still-end-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let clip = dir.join("clip.mp4");
    let made = std::process::Command::new(&ffmpeg)
        .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=size=64x64:rate=30:duration=1", "-pix_fmt", "yuv420p"])
        .arg(&clip)
        .status()
        .unwrap();
    assert!(made.success());
    for format in ["png", "jpeg", "webp"] {
        for at_ms in [500, 1000, 4000] {
            let bytes = extract_still_frame_blocking(&ffmpeg, &clip, at_ms, format)
                .unwrap_or_else(|e| panic!("{format} at {at_ms} ms: {e}"));
            assert!(!bytes.is_empty(), "{format} at {at_ms} ms");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_export_tile_is_titled_with_the_file_name_without_its_extension() {
    assert_eq!(export_tile_name(Path::new("C:/clips/Summer trip-edit.mp4")), "Summer trip-edit");
    assert_eq!(export_tile_name(Path::new("D:/a/clip.v2.webm")), "clip.v2");
    assert_eq!(export_tile_name(Path::new("D:/a/.hidden")), ".hidden");
}

#[test]
fn an_export_tile_needs_a_free_cell_in_the_active_view() {
    let mut s = Settings::default();
    let mut v = test_view("a");
    v.cols = 2;
    v.rows = 1;
    v.layouts.insert(grid_key(2, 1), HashMap::from([("p1".to_string(), [0, 0])]));
    s.views = vec![v];
    s.active_view = "a".to_string();
    assert!(export_tile_has_room(&s), "one cell is still free");
    s.views[0].layouts.get_mut(&grid_key(2, 1)).unwrap().insert("p2".to_string(), [1, 0]);
    assert!(!export_tile_has_room(&s), "a full grid takes no tile");
    s.views.clear();
    assert!(!export_tile_has_room(&s), "no view, no room");
}

#[test]
fn the_export_tile_command_takes_only_a_known_export_and_stays_off_the_other_windows() {
    let src = source!("../video_editor.rs");
    let body = &src[src.find("pub(crate) async fn video_editor_add_tile").unwrap()..];
    assert!(body.contains("is_known_export_output(&file)"), "only a file this session exported");
    assert!(body.find("export_tile_has_room").unwrap() < body.find("add_prompt(").unwrap(), "the room check comes first");
    let call = &body[body.find("add_prompt(").unwrap()..];
    assert!(call.contains("None,\n        // Shows the clip's player on the tile, as a dropped video does.\n        Some(true),"), "the tile shows the player");
    let caps = source!("../../capabilities/video-editor.json");
    assert!(caps.contains("allow-video-editor-add-tile"));
    assert!(!caps.contains("allow-add-prompt"), "the editor reaches the store only through the one command");
}

#[test]
fn export_request_paths_refuse_a_network_destination_or_media_file_on_text() {
    let none: Vec<PathBuf> = Vec::new();
    assert!(export_request_paths_refused(&none, r"\\host\share\x.mp4", &[]));
    assert!(export_request_paths_refused(&none, r"C:\out\x.mp4", &[r"\\host\share\a.png".to_string()]));
    assert!(export_request_paths_refused(&none, r"C:\out\x.mp4", &["//host/share/a.png".to_string()]));
    assert!(!export_request_paths_refused(&none, r"C:\out\x.mp4", &[r"D:\media\a.png".to_string()]));
    let share = vec![PathBuf::from(r"\\nas\rec")];
    assert!(!export_request_paths_refused(&share, r"\\nas\rec\x.mp4", &[]), "the user's own share folder keeps working");
}

#[test]
fn the_batch_export_loop_gates_an_item_on_text_before_any_file_check() {
    let src = source!("../video_editor.rs");
    let batch = &src[src.find("pub(crate) async fn export_recordings_batch").unwrap()..];
    let loop_at = batch.find("for item in items").unwrap();
    let body = &batch[loop_at..loop_at + 900];
    assert!(!body.contains("!item_src.is_file() ||"), "is_file must not run ahead of the network gate");
    assert!(body.contains("export_request_paths_refused(&own_dirs, &item.dest_path"));
}

#[test]
fn a_rotated_phone_clip_reports_its_upright_size() {
    let side = |rot: &str| serde_json::json!({"width": 1920, "height": 1080, "side_data_list": [{"side_data_type": "Display Matrix", "rotation": rot.parse::<f64>().unwrap()}]});
    assert_eq!(stream_display_size(&side("-90")), (1080, 1920));
    assert_eq!(stream_display_size(&side("90")), (1080, 1920));
    assert_eq!(stream_display_size(&side("270")), (1080, 1920));
    assert_eq!(stream_display_size(&side("180")), (1920, 1080));
    assert_eq!(stream_display_size(&side("0")), (1920, 1080));
    let tag = serde_json::json!({"width": 1920, "height": 1080, "tags": {"rotate": "90"}});
    assert_eq!(stream_display_size(&tag), (1080, 1920));
    let plain = serde_json::json!({"width": 1920, "height": 1080});
    assert_eq!(stream_display_size(&plain), (1920, 1080));
}

#[test]
fn a_rotated_clip_reports_the_quarter_turns_ffmpeg_autorotate_applies() {
    let side = |rot: serde_json::Value| serde_json::json!({"width": 1920, "height": 1080, "side_data_list": [{"side_data_type": "Display Matrix", "rotation": rot}]});
    // ffprobe reports the rotation counter-clockwise: 90 turns the picture left (one quarter),
    // -90 and 270 turn it right (three quarters).
    assert_eq!(stream_rotation_quarters(&side(serde_json::json!(90))), 1);
    assert_eq!(stream_rotation_quarters(&side(serde_json::json!(-90))), 3);
    assert_eq!(stream_rotation_quarters(&side(serde_json::json!(270))), 3);
    assert_eq!(stream_rotation_quarters(&side(serde_json::json!(180))), 2);
    assert_eq!(stream_rotation_quarters(&side(serde_json::json!(-180))), 2);
    assert_eq!(stream_rotation_quarters(&side(serde_json::json!(0))), 0);
    assert_eq!(stream_rotation_quarters(&side(serde_json::json!("-90"))), 3, "string form");
    assert_eq!(stream_rotation_quarters(&side(serde_json::json!(45))), 0, "not a quarter turn");
    let tag = serde_json::json!({"width": 1920, "height": 1080, "tags": {"rotate": "270"}});
    assert_eq!(stream_rotation_quarters(&tag), 3);
    let plain = serde_json::json!({"width": 1920, "height": 1080});
    assert_eq!(stream_rotation_quarters(&plain), 0);
}

#[test]
fn a_re_encoded_mp4_gets_an_even_size_step_after_the_last_output_label() {
    let g = even_final_size("[0:v]trim=start=1,setpts=PTS-STARTPTS[vout]");
    assert_eq!(g, "[0:v]trim=start=1,setpts=PTS-STARTPTS[vevenin];[vevenin]scale=trunc(iw/2)*2:trunc(ih/2)*2[vout]");
    let multi = even_final_size("[0:v]split[a][b];[a][b]overlay[vout];");
    assert!(multi.ends_with("[vevenin]scale=trunc(iw/2)*2:trunc(ih/2)*2[vout]"));
    assert!(multi.contains("overlay[vevenin];"), "only the final label is renamed: {multi}");
    assert_eq!(even_final_size("[0:v]null[other]"), "[0:v]null[other]");
}

#[test]
fn the_export_args_even_the_size_for_mp4_and_avi_only() {
    let src = source!("../video_editor.rs");
    let args = &src[src.find("let needs_even").unwrap()..];
    assert!(args.contains("matches!(req.format.as_str(), \"mp4\" | \"avi\")"));
    assert!(args.contains("if needs_even && req.filter_chain.is_none()"));
    assert!(args.contains("even_final_size(v)"));
}

#[test]
fn a_media_file_on_another_computer_is_not_reported_as_a_refused_destination() {
    let none: Vec<PathBuf> = Vec::new();
    assert_eq!(export_request_refusal(&none, r"\\host\share\x.mp4", &[]), Some("export destination not recognized"));
    assert_eq!(export_request_refusal(&none, r"C:\out\x.mp4", &[r"\\host\share\a.png".to_string()]), Some("source-not-allowed"));
    assert_eq!(export_request_refusal(&none, r"C:\out\x.mp4", &[r"D:\m\a.png".to_string()]), None);
}

#[test]
fn a_cancelled_batch_item_is_neither_a_success_nor_a_failure() {
    let src = source!("../video_editor.rs");
    let at = src.find("pub(crate) async fn await_batch_export").unwrap();
    let body = &src[at..at + 1200];
    let cancelled = body.find("Err(e) if e == \"cancelled\"").expect("cancel is told apart from an error");
    let generic = body.find("Err(_) =>").unwrap();
    assert!(cancelled < generic, "the cancel arm comes before the generic failure arm");
    assert!(body[cancelled..generic].contains("None"), "a cancel is counted nowhere");
}

#[test]
fn the_batch_mic_graph_mixes_the_sidecar_or_the_second_track() {
    let two = batch_mic_graph(2, Path::new(r"C:\nope\rec.mp4")).unwrap();
    assert_eq!(two, "[0:a:0][0:a:1]amix=inputs=2:normalize=0[aout]");
    assert!(batch_mic_graph(1, Path::new(r"C:\nope\rec.mp4")).is_none(), "no sidecar, one track: audio stays as it is");
    assert_eq!(count_audio_lines("  Stream #0:0: Video: h264\n  Stream #0:1[0x2](und): Audio: aac\n  Stream #0:2: Audio: aac\n"), 2);
    let dir = std::env::temp_dir().join(format!("ps-batchmic-graph-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-1.mp4");
    fs::write(dir.join("rec-1.mic.wav"), b"x").unwrap();
    let one = batch_mic_graph(1, &src).unwrap();
    assert!(one.starts_with("amovie='") && one.contains("rec-1.mic.wav'[bmic];[0:a:0][bmic]amix=inputs=2"), "{one}");
    assert!(one.ends_with("[aout]"));
    let none = batch_mic_graph(0, &src).unwrap();
    assert!(none.ends_with("[bmic]anull[aout]"), "{none}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_batch_export_keeps_the_microphone_that_still_sits_in_its_sidecar() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-batchmic-real-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = make_test_mp4(&ffmpeg, &dir, "rec-mic.mp4", 1, "64x48");
    let wav = make_test_wav(&dir, "rec-mic.mic.wav", 1);
    assert_eq!(wav, src.with_extension("mic.wav"));
    let dest = dir.join("out.mp4");
    let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    run_batch_job_blocking(&ffmpeg, &src.to_string_lossy(), &dest.to_string_lossy(), 1000, "mp4", false, Some(2000), my_gen, |_| {})
        .expect("the batch job succeeds");
    assert_eq!(count_audio_streams(&ffmpeg, &dest), 1, "the mic is in the export as one stream");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_odd_sized_source_exports_to_mp4_once_an_edit_forces_a_re_encode() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ps-odd-real-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    // ffv1 can store an odd size; libx264 cannot, so the export has to even it.
    use std::os::windows::process::CommandExt;
    let src = dir.join("odd.mkv");
    let status = std::process::Command::new(&ffmpeg)
        .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=duration=1:size=65x47:rate=15", "-c:v", "ffv1", "-pix_fmt", "yuv420p"])
        .arg(&src)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .unwrap();
    assert!(status.success());
    let dest = dir.join("out.mp4");
    let req = ExportRequest {
        src_path: src.to_string_lossy().to_string(),
        dest_path: dest.to_string_lossy().to_string(),
        filter_chain: Some("[0:v]setpts=PTS-STARTPTS[vout]".to_string()),
        audio_map: None,
        media_paths: Vec::new(),
        lossless_copy: false,
        copy_audio: CopyAudio::All,
        lossless_quality: false,
        format: "mp4".to_string(),
        video_kbps: Some(1000),
        trim_start_ms: None,
        trim_end_ms: None,
        total_ms: 1000,
    };
    let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    export_video_blocking(&ffmpeg, &src, &dest, &req, my_gen, |_| {}).expect("an odd source exports");
    // The no-graph re-encode takes the same fix through -vf.
    let req2 = ExportRequest { filter_chain: None, dest_path: dir.join("out2.mp4").to_string_lossy().to_string(), ..req };
    let my_gen = VIDEO_EXPORT_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    export_video_blocking(&ffmpeg, &src, &dir.join("out2.mp4"), &req2, my_gen, |_| {}).expect("an odd source exports without a graph");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_media_path_with_an_equals_sign_is_a_file_name_not_an_option_key() {
    let dir = std::env::temp_dir().join(format!("ps-graph-eq-{}", std::process::id())).join("a=b");
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec.mp4");
    let badge = dir.join("x=y.png");
    for p in [&src, &badge] {
        fs::write(p, b"x").unwrap();
    }
    let ff = |p: &Path| {
        let c = fs::canonicalize(p).unwrap().to_string_lossy().to_string();
        c.trim_start_matches(r"\\?\").replace('\\', "/").replace(':', r"\:")
    };
    let media = vec![badge.to_string_lossy().to_string()];
    let graph = format!("movie='{}'[i0];[0:v][i0]overlay=1:2[vout]", ff(&badge));
    assert!(validate_export_request_graphs(&src, &media, &[&graph]).is_ok(), "{graph}");
    // A real key stays a key, even framed by blanks or hidden behind an escape at the graph level.
    let other = format!("movie=' filename ={}'[i0]", ff(&badge));
    assert!(validate_export_request_graphs(&src, &media, &[&other]).is_ok());
    assert!(validate_export_request_graphs(&src, &media, &[r"[0:v]drawtext=t\extfile=C\:/x.txt[vout]"]).is_err());
    assert!(validate_export_request_graphs(&src, &media, &["[0:v]drawtext= textfile =x[vout]"]).is_err());
    assert!(validate_export_request_graphs(&src, &media, &["[0:v]drawtext=/text=x[vout]"]).is_err());
    let _ = fs::remove_dir_all(dir.parent().unwrap());
}

#[test]
fn camcorder_and_still_formats_the_picker_offers_pass_the_media_type_check() {
    for name in ["clip.MTS", "a.mxf", "disc.vob", "raw.m2v", "photo.heic", "pic.jxl", "clip.mp4"] {
        assert!(has_export_media_ext(Path::new(name)), "{name}");
    }
    assert!(!has_export_media_ext(Path::new("notes.txt")));
}

#[test]
fn the_editors_export_progress_carries_a_job_tag_a_conversion_does_not() {
    let export = serde_json::to_value(ConvertProgressPayload { ms: 5, job: Some("export") }).unwrap();
    assert_eq!(export, serde_json::json!({"ms": 5, "job": "export"}));
    let convert = serde_json::to_value(ConvertProgressPayload { ms: 5, job: None }).unwrap();
    assert_eq!(convert, serde_json::json!({"ms": 5}), "the conversion payload keeps its old shape");
}

#[test]
fn the_export_disk_probes_skip_a_network_destination_on_text() {
    assert!(check_export_disk_space(r"\\host\share\x.mp4".to_string(), "mp4".to_string(), None, false, 600_000).is_none());
    assert!(check_export_disk_space_batch(r"\\host\share".to_string(), "mp4".to_string(), None, false, vec![600_000]).is_none());
    let src = source!("../rec_cmds.rs");
    for name in ["pub(crate) fn check_export_disk_space(", "pub(crate) fn check_export_disk_space_batch("] {
        let at = src.find(name).unwrap();
        let body = &src[at..at + 900];
        assert!(body.find("is_network_path(").unwrap() < body.find("free_space_bytes(").unwrap(), "{name}: text gate first");
    }
}
