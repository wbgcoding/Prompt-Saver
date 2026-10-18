use super::*;

// The checks below compare multi-line source text; a checkout with Windows line endings
// must read the same as one with Unix endings.
macro_rules! source {
    ($path:literal) => {
        include_str!($path).replace("\r\n", "\n")
    };
}

// ---- recording, media and clipboard fixes ----

#[cfg(windows)]
#[test]
fn a_locked_vault_delivers_no_confidential_text() {
    assert_eq!(clipwatch::deliverable_text(true, false, "secret"), "");
    assert_eq!(clipwatch::deliverable_text(true, true, "secret"), "secret");
    assert_eq!(clipwatch::deliverable_text(false, false, "plain"), "plain");
}

#[test]
fn a_shutdown_during_voice_prepare_cancels_the_pending_start() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let (claiming, cancel) = (AtomicBool::new(true), AtomicBool::new(false));
    voice_cmds::mark_pending_start_cancelled(&claiming, &cancel);
    assert!(cancel.load(Ordering::SeqCst), "a start that is still preparing must be cancelled");
    let (idle, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
    voice_cmds::mark_pending_start_cancelled(&idle, &cancel);
    assert!(!cancel.load(Ordering::SeqCst), "no pending start, nothing to cancel");
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
    assert!(export_source_is_allowed(&dirs, &temp_root, &paths, &attached));
    assert!(!export_source_is_allowed(&dirs, &temp_root, &paths, &foreign), "an arbitrary readable file");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn only_clip_export_files_go_to_the_clipboard() {
    let root = std::env::temp_dir().join(format!("ps-clipexp-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let own = root.join("own");
    let other = root.join("other");
    for d in [&own, &other] {
        fs::create_dir_all(d).unwrap();
    }
    let name = format!("{}-clipexport-1.mp4", brand::TEMP_PREFIX);
    let good = own.join(&name);
    let wrong_dir = other.join(&name);
    let wrong_name = own.join("data.db");
    for f in [&good, &wrong_dir, &wrong_name] {
        fs::write(f, b"x").unwrap();
    }
    assert!(is_clip_export_file(&own, &good));
    assert!(!is_clip_export_file(&own, &wrong_dir), "right name in another folder");
    assert!(!is_clip_export_file(&own, &wrong_name), "some other file in the temp folder");
    assert!(!is_clip_export_file(&own, &own.join(format!("..{}other{}{}", std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR, name))));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn native_helpers_live_in_the_per_user_app_folder() {
    let base = std::ffi::OsString::from("C:/Users/x/AppData/Local");
    let dir = local_app_dir_in(Some(base)).unwrap();
    assert!(dir.starts_with("C:/Users/x/AppData/Local") && dir.ends_with(brand::DATA_FOLDER));
    assert!(local_app_dir_in(None).is_none());
    assert!(local_app_dir_in(Some(std::ffi::OsString::new())).is_none());
    if let Some(p) = pdfium_cache_path() {
        assert!(!p.starts_with(std::env::temp_dir()), "the shared temp folder is not trusted for a loaded DLL");
    }
}

#[test]
fn media_tools_are_searched_only_in_app_owned_folders() {
    let exe = PathBuf::from("C:/app");
    let local = PathBuf::from("C:/local/app");
    assert_eq!(media_tool_dirs(Some(exe.clone()), Some(local.clone())), vec![exe.clone(), local.clone()]);
    assert_eq!(media_tool_dirs(None, Some(local.clone())), vec![local]);
    assert!(media_tool_dirs(None, None).is_empty());
    // The data folder is not a parameter any more: a planted copy there cannot be found.
    let src = source!("../ffmpeg.rs");
    let discovery = src.split("pub(crate) fn find_media_tool").nth(1).unwrap().split("pub(crate) fn ffmpeg_search_path").next().unwrap();
    assert!(!discovery.contains("data_dir"));
}

#[test]
fn a_network_path_is_refused_before_any_file_check() {
    let root = std::env::temp_dir().join(format!("ps-recloc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let allowed = [root.clone()];
    let missing = root.join("rec-gone.mp4");
    assert!(is_recording_source_location(&allowed, &missing), "the location check never needs the file");
    assert!(!is_own_recording_source(&allowed, &missing));
    assert!(!is_recording_source_location(&allowed, Path::new(r"\host\share\rec-1.mp4")));
    assert!(!is_recording_source_location(&allowed, Path::new("//host/share/rec-1.mp4")));
    let _ = fs::remove_dir_all(&root);
}

// Structural fix (the overlay window cannot be built without a running app), so the check
// reads the source: a failed overlay build must end the whole session, not just clear a flag.
#[test]
fn a_failed_overlay_build_ends_the_whole_snip_session() {
    let src = source!("../snip.rs");
    let arm = src.split("match build_snip_window(&app) {").nth(1).unwrap().split("Ok(w) => w,").nth(1).unwrap();
    let arm = arm.split("return Err(e);").next().unwrap();
    assert!(arm.contains("close_all_snip(&app)"));
    assert!(!arm.contains("SNIP_ACTIVE.store(false"), "clearing one flag leaves the rest of the session behind");
}

#[test]
fn a_new_recording_never_takes_an_existing_name() {
    let root = std::env::temp_dir().join(format!("ps-recname-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let name = rec_filename(2026, 10, 18, 12, 0, 0);
    assert_eq!(free_rec_path(&root, &name), root.join(&name));
    fs::write(root.join(&name), b"x").unwrap();
    assert_eq!(free_rec_path(&root, &name), root.join("rec-2026-10-18_120000-2.mp4"));
    fs::write(root.join("rec-2026-10-18_120000-2.mp4"), b"x").unwrap();
    assert_eq!(free_rec_path(&root, &name), root.join("rec-2026-10-18_120000-3.mp4"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_mic_sidecar_that_cannot_be_created_marks_the_mic_lost() {
    let queue = Arc::new(AudioQueueShared::new());
    let bad = std::env::temp_dir().join(format!("ps-nodir-{}", std::process::id())).join("x").join("rec.mic.wav");
    crate::recording::mic_wav_writer_loop(
        queue.clone(),
        Arc::new(RecordClock::new()),
        Arc::new(RecFeederShared::new()),
        Arc::new(AtomicBool::new(false)),
        bad,
    );
    assert!(queue.is_lost(), "the bar must hear about a mic track that is not being written");
}

#[test]
fn only_a_silent_ffmpeg_is_declared_stalled() {
    assert!(!ffmpeg_stalled(0));
    assert!(!ffmpeg_stalled(FFMPEG_STALL_SECS));
    assert!(ffmpeg_stalled(FFMPEG_STALL_SECS + 1));
    // Structural: the check must be fed by the time since the last progress tick, never by the
    // total run time, and every tick must restart that clock.
    let src = source!("../ffmpeg.rs");
    assert!(src.contains("ffmpeg_stalled(last_progress.elapsed().as_secs())"));
    assert!(src.contains("last_progress = std::time::Instant::now();"));
    assert!(!src.contains("start.elapsed().as_secs() >"));
}

#[test]
fn the_export_filter_graph_travels_in_a_swept_file() {
    let dst = Path::new(r"C:\clips\talk-edit.mp4");
    let script = filter_script_path(dst);
    assert_eq!(script.parent(), dst.parent());
    assert!(staged_export_pid(&script) == Some(std::process::id()), "the startup sweep must recognise the file");
    let long = "drawbox=x=1:y=2:w=3:h=4:enable='between(t,1,2)'".repeat(2000);
    let root = std::env::temp_dir().join(format!("ps-graph-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let written = write_filter_script(&root.join("out.mp4"), &long).unwrap();
    assert_eq!(fs::read_to_string(&written).unwrap(), long);
    assert!(long.len() > 32767, "this graph is longer than a command line may be");
    remove_filter_script(&written);
    assert!(!written.exists());
    let _ = fs::remove_dir_all(&root);
    // Contrast: the old inline argument is gone from the export path.
    let src = source!("../video_editor.rs");
    assert!(!src.contains("push(\"-filter_complex\".to_string())"));
    assert!(src.contains("\"-/filter_complex\""));
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
fn closing_the_editor_refuses_every_waiting_open_request() {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    crate::video_editor::VE_LOAD_ACKS.lock().unwrap().get_or_insert_with(std::collections::HashMap::new).insert(u64::MAX - 7, tx);
    *crate::video_editor::VE_PENDING_REQUEST.lock().unwrap() = Some(("a".into(), "b".into(), "c".into()));
    // The caller has been told "asking": it waits the long window for the person's answer.
    let waiter = std::thread::spawn(move || crate::video_editor::wait_for_editor_decision(&rx, std::time::Duration::from_secs(5), std::time::Duration::from_secs(3600)));
    crate::video_editor::VE_LOAD_ACKS.lock().unwrap().as_ref().unwrap()[&(u64::MAX - 7)].send("asking".into()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    crate::video_editor::refuse_pending_editor_requests();
    assert_eq!(waiter.join().unwrap(), crate::video_editor::EditorLoadDecision::Refused);
    assert!(crate::video_editor::VE_PENDING_REQUEST.lock().unwrap().is_none(), "a stale offer must not outlive its window");
    let src = source!("../video_editor.rs");
    let hook = src.find("tauri::WindowEvent::Destroyed").expect("the editor window must watch its own destruction");
    assert!(src[hook..hook + 200].contains("refuse_pending_editor_requests"));
}

// Structural fix, no red unit test: the rect comes from a live window enumeration, which a unit
// test cannot fake. The pure choice (frame over client) is covered by snip_window_rect_prefers_the_frame.
#[test]
fn the_window_fallback_crops_the_frame_the_overlay_showed() {
    let src = source!("../capture.rs");
    // The rect comes from the one window probe, which crops the visible frame the overlay showed.
    let at = src.find("pub(crate) fn probe_window(").expect("probe");
    assert!(src[at..at + 600].contains("snip_window_rect(window_frame_rect(id)"));
    assert!(src.contains("match probe.rect"));
}

#[test]
fn an_attached_photo_previews_upright() {
    let dir = std::env::temp_dir().join(format!("ps-photo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("sideways.jpg");
    // 2x1 pixels with an orientation flag of 6: shown upright it is 1x2.
    std::fs::write(&file, shots::jpeg_with_orientation(6)).unwrap();
    let url = image_file_preview(&file.to_string_lossy()).expect("previews");
    let img = decode_image_mem(&data_url_bytes(&url)).expect("the preview decodes");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!((img.width(), img.height()), (1, 2));
}

#[test]
fn a_tiff_reaches_the_editor_as_a_png() {
    let dir = std::env::temp_dir().join(format!("ps-tiff-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("scan.tif");
    image::RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255])).save_with_format(&file, image::ImageFormat::Tiff).unwrap();
    let (kind, fmt) = detect_media_kind(&file, None).expect("a tiff is an image");
    assert_eq!(kind, "image");
    let url = editor_image_data_url(&file, fmt).expect("converts");
    let png = file.with_file_name("plain.png");
    image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 255])).save(&png).unwrap();
    let plain = editor_image_data_url(&png, Some(image::ImageFormat::Png)).expect("passes through");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(url.starts_with("data:image/png;base64,"), "the webview cannot decode tiff");
    assert_eq!(decode_image_mem(&data_url_bytes(&url)).map(|i| (i.width(), i.height())), Some((3, 2)));
    assert!(plain.starts_with("data:image/png;base64,"));
}

#[cfg(windows)]
#[test]
fn a_hung_powershell_query_is_killed_at_its_deadline() {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", "Start-Sleep -Seconds 60"]).creation_flags(0x0800_0000);
    let t = std::time::Instant::now();
    let res = ocr::run_with_deadline(cmd, std::time::Duration::from_millis(1500));
    assert_eq!(res, Err("timeout".to_string()));
    assert!(t.elapsed() < std::time::Duration::from_secs(20), "the child must not be waited out");
    let mut quick = std::process::Command::new("powershell.exe");
    quick.args(["-NoProfile", "-NonInteractive", "-Command", "Write-Output ok"]).creation_flags(0x0800_0000);
    assert_eq!(ocr::run_with_deadline(quick, std::time::Duration::from_secs(60)).map(|s| s.trim().to_string()), Ok("ok".to_string()));
}

#[test]
fn the_camera_page_asks_for_the_permission_state_and_has_no_timer_fallback() {
    let js = source!("../../../ui/camera.js");
    assert!(js.contains("invoke(\"camera_permission_ready\")"), "the page must query the flag after listening");
    assert!(!js.contains("}, 1500)"), "no timer may count the permission as given");
    let lib = source!("../lib.rs");
    assert!(lib.contains("camera::camera_permission_ready"));
    camera::camera_permission_ready();
}

// Structural fix, no red unit test: a refused thread spawn cannot be provoked in a test.
#[test]
fn a_refused_audio_thread_is_a_start_error_not_a_panic() {
    let audio = source!("../audio.rs");
    assert!(audio.contains("-> std::io::Result<thread::JoinHandle<()>>"));
    assert!(!audio.contains("expect(\"spawn rec-audio thread\")"));
    let rec = source!("../recording.rs");
    assert_eq!(rec.matches("audio_stop.clone().unwrap(),\n        ).map_err(|e| e.to_string())?").count(), 2);
}

// Structural fix, no red unit test: the encoder needs a real media session to fail on demand.
#[test]
fn the_feeder_stops_feeding_after_the_first_encoder_error() {
    let rec = source!("../recording.rs");
    let drain = rec.split("fn drain_pending_frames(").nth(1).unwrap().split("
}
").next().unwrap();
    assert!(drain.contains("shared.request_stop();") && drain.contains("return;"), "the drain must leave at the first error");
    assert!(rec.contains("!clock.is_paused() && shared.peek_encode_error().is_none()"), "hold and audio sends must stop after an error");
}

// Structural fix, no red unit test: the old shape could not fail at run time, which is the finding.
#[test]
fn stopping_and_cancelling_a_recording_return_plain_values() {
    let rec = source!("../recording.rs");
    assert!(rec.contains("pub fn stop(session: RecSession) -> StopResult {"));
    assert!(rec.contains("pub fn cancel(session: RecSession) {"));
    assert!(rec.contains("fn finish_and_stop(mut session: RecSession) -> (u64, Option<String>) {"));
    let cmds = source!("../rec_cmds.rs");
    assert!(!cmds.contains("recording::cancel(session)?"));
    assert!(!cmds.contains("= stopped?;"));
}

// Structural fix, no red unit test: only comments and an attribute moved.
#[test]
fn the_follow_comment_and_cfg_sit_on_the_follow_function() {
    let bar = source!("../rec_bar.rs");
    assert!(bar.contains("// periodic jobs.
#[cfg(windows)]
pub(crate) fn spawn_rec_bar_follow("));
    assert!(!bar.contains("// periodic jobs.
#[cfg(windows)]
// Bumped"));
}

#[test]
fn the_ffmpeg_log_buffer_keeps_only_its_newest_bytes() {
    let mut buf = Vec::new();
    for i in 0..1000u32 {
        push_bounded_tail(&mut buf, format!("warning {i}
").as_bytes(), 100);
        assert!(buf.len() <= 100);
    }
    let text = String::from_utf8(buf).unwrap();
    assert!(text.ends_with("warning 999
"));
    assert!(!text.contains("warning 1
"));
    let mut small = b"abc".to_vec();
    push_bounded_tail(&mut small, b"de", 100);
    assert_eq!(small, b"abcde");
}

// Structural fix, no red unit test: child.wait() cannot be made to fail on a real child.
#[test]
fn a_failed_wait_removes_the_staged_export() {
    let src = source!("../ffmpeg.rs");
    let after = src.split("let status = match child.wait() {").nth(1).unwrap();
    let err_arm = after.split("let stderr_text").next().unwrap();
    assert!(err_arm.contains("forget_staged();") && err_arm.contains("fs::remove_file(&staged)"));
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
fn every_folder_dialog_title_is_translated_in_every_language() {
    let purposes = ["screenshot", "videoExport", "recording", "sync", "data"];
    let en: Vec<&str> = purposes.iter().map(|p| crate::snip::folder_picker_title(p, "en").unwrap()).collect();
    for lang in crate::model::LANG_CODES {
        let titles: Vec<&str> = purposes.iter().map(|p| crate::snip::folder_picker_title(p, lang).unwrap()).collect();
        for (i, t) in titles.iter().enumerate() {
            assert_ne!(*t, en[i], "{lang} still shows the English title for {}", purposes[i]);
            assert!(!t.is_empty());
        }
        let mut sorted = titles.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), purposes.len(), "{lang} reuses one title for two purposes");
    }
}

// Structural fix, no red unit test: counting real xcap listings needs a live desktop.
#[test]
fn a_window_click_lists_the_windows_once() {
    let src = source!("../capture.rs");
    let body = &src[src.find("pub(crate) async fn capture_window(").unwrap()..src.find("pub(crate) struct WindowProbe").unwrap()];
    assert_eq!(body.matches("xcap::Window::all()").count(), 0, "capture_window must go through probe_window");
    let missing = crate::capture::probe_window(u32::MAX, true);
    assert!(missing.app_name.is_none() && missing.direct.is_none() && missing.rect.is_none());
}

// Structural fix, no red unit test: the cost is a 7 MB read per call, not a visible result.
#[test]
fn the_verified_pdfium_copy_is_remembered_after_the_first_check() {
    let src = source!("../images.rs");
    let body = &src[src.find("pub(crate) fn pdfium_lib_path").unwrap()..];
    let lookup = body.find("PDFIUM_VERIFIED.get()").expect("lookup");
    let read = body.find("open_verified_pdfium(&cached").expect("content check");
    assert!(lookup < read, "the remembered path is consulted before the content check");
    assert_eq!(body.matches("PDFIUM_VERIFIED.set(").count(), 2);
}

#[test]
fn saving_the_parts_does_not_hold_the_shared_list_lock() {
    let part = |n: &str| RecordingPartInfo { path: n.to_string(), mic_wav_path: None };
    let shared = std::sync::Mutex::new(vec![part("a"), part("b")]);
    let seen = with_parts_outside_lock(&shared, |mine| {
        assert!(shared.try_lock().is_ok(), "the lock is free while the parts are being moved");
        assert_eq!(mine.len(), 2);
        // "a" moved, "b" could not be: it must come back, ahead of a part added meanwhile.
        mine.retain(|p| p.path == "b");
        shared.lock().unwrap().push(part("c"));
        7
    });
    assert_eq!(seen, 7);
    let left: Vec<String> = shared.lock().unwrap().iter().map(|p| p.path.clone()).collect();
    assert_eq!(left, vec!["b".to_string(), "c".to_string()]);
}

// Structural fix, no red unit test: the old spelling produced the same path as the constant today.
#[test]
fn the_key_sidecar_is_always_named_through_the_shared_builder() {
    let src = source!("../media_files.rs");
    assert!(!src.contains("with_extension(\"keys.json\")"));
    assert_eq!(crate::media_files::sidecar_path(Path::new("a/rec.mp4"), crate::media_files::KEYS_SIDECAR_EXT), Path::new("a/rec.keys.json"));
}

// Structural fix, no red unit test: a write that fails half-way cannot be provoked on demand.
#[test]
fn the_ocr_temp_guard_exists_before_the_png_is_written() {
    let src = source!("../ocr.rs");
    for save in ["im.save(&out)", "crop.save(&path)"] {
        let at = src.find(save).expect(save);
        let guard = src[..at].rfind("TempFileGuard(Some(").expect("guard");
        assert!(at - guard < 400, "the guard sits right before {save}");
    }
}

// Structural fix, no red unit test: where the work runs cannot be observed from a unit test.
#[cfg(windows)]
#[test]
fn the_ocr_data_url_is_decoded_on_the_blocking_pool() {
    let src = source!("../ocr_cmds.rs");
    let cmd = src.split("pub(crate) async fn ocr_image_data").nth(1).unwrap().split("fn decode_ocr_data_url").next().unwrap();
    let spawn = cmd.find("spawn_blocking").expect("blocking pool");
    let decode = cmd.find("decode_ocr_data_url(&data_url)").expect("decode call");
    assert!(decode > spawn, "decoding must happen inside the blocking closure");
    assert!(!cmd.contains("data_url_bytes("), "no decode before the pool");
    assert!(crate::ocr_cmds::decode_ocr_data_url("data:image/png;base64,AAAA").is_err());
}

// Structural fix, no red unit test: a panicking decoder inside the blocking pool cannot be
// provoked without the real engine.
#[test]
fn a_failed_ocr_index_worker_marks_the_job_as_error() {
    let src = source!("../ocr_cmds.rs");
    let body = src.split("pub(crate) async fn ocr_index_backfill").nth(1).unwrap().split("pub(crate) async fn ocr_capabilities(").next().unwrap();
    let at = body.find("Err(e) => {").expect("join error arm");
    let arm = &body[at..at + 200];
    assert!(arm.contains("push_bg_op(&app, \"ocr-index\", \"ocr-index\", \"error\""), "{arm}");
}

// Structural fix, no red unit test: install_permission_handler cannot be made to fail in a test.
#[test]
fn a_camera_window_without_its_permission_handler_is_destroyed() {
    let src = source!("../camera.rs");
    let at = src.find("if let Err(e) = install_permission_handler(&app, &win) {").expect("error arm");
    let arm = &src[at..at + 120];
    assert!(arm.contains("win.destroy()"), "{arm}");
    assert!(!src.contains("install_permission_handler(&app, &win)?;"));
}

#[test]
fn a_failed_camera_sidecar_write_is_reported() {
    let root = std::env::temp_dir().join(format!("ps-camside-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let session = |dir: &std::path::Path| crate::camera::CamSession {
        output_path: dir.join("cam-1.webm"),
        companion_path: None,
        finishing: None,
    };
    // The folder does not exist, so the sidecar cannot be written.
    assert!(crate::camera::finalize_session(&session(&root.join("missing")), 5, None).is_err());
    std::fs::create_dir_all(&root).unwrap();
    assert!(crate::camera::finalize_session(&session(&root), 5, Some(7)).is_ok());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_failed_key_sidecar_write_is_an_error_without_the_path() {
    let root = std::env::temp_dir().join(format!("ps-keyside-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let out = root.join("missing").join("rec-1.mp4");
    let err = crate::keylog::try_write_sidecar(&out, &Default::default()).unwrap_err();
    assert!(err.starts_with("write failed"), "{err}");
    assert!(!err.contains("missing"), "the reason must not carry the path: {err}");
    std::fs::create_dir_all(root.join("ok")).unwrap();
    assert!(crate::keylog::try_write_sidecar(&root.join("ok").join("rec-1.mp4"), &Default::default()).is_ok());
    let _ = std::fs::remove_dir_all(&root);
}

// Structural fix, no red unit test: a refused clipboard promise or listener cannot be provoked
// without owning the system clipboard.
#[test]
fn a_refused_clipboard_promise_ends_the_session() {
    let src = source!("../clipwatch.rs");
    assert!(src.contains("promised = SetClipboardData(CF_UNICODETEXT, 0) != 0;"));
    assert!(src.contains("end_session_with_plain_write(is_first, true);"));
    assert!(src.contains("if AddClipboardFormatListener(hwnd) == 0 {"));
}

#[test]
fn a_reserved_device_name_with_trailing_spaces_before_the_dot_is_refused() {
    assert!(!crate::video_editor::export_file_name_is_valid("CON .mp4"));
    assert!(!crate::video_editor::export_file_name_is_valid("nul  .take2.mp4"));
    assert!(crate::video_editor::export_file_name_is_valid("CONE .mp4"));
}

// Structural fix, no red unit test: the sweep results stay the same, only the directory reads drop.
#[test]
fn the_startup_sweep_reads_the_app_temp_folder_once() {
    let src = source!("../snip.rs");
    let body = src.split("pub(crate) fn sweep_stale_temp_files_in").nth(1).unwrap().split("pub(crate) fn remove_own_temp_leftovers_in").next().unwrap();
    assert_eq!(body.matches("fs::read_dir(&ocr_dir)").count(), 1);
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

// Structural fix, no red unit test: a modal dialog cannot be opened in a test.
#[test]
fn the_media_dialogs_wait_on_a_blocking_thread() {
    for (name, src, anchors) in [
        ("images", source!("../images.rs"), &["fn pick_file_path", "fn pick_media_file_path"][..]),
        ("media_files", source!("../media_files.rs"), &["fn save_recording_as", "fn pick_export_folder"][..]),
        ("snip", source!("../snip.rs"), &["fn save_screenshot_as", "fn pick_folder"][..]),
    ] {
        for anchor in anchors {
            let at = src.find(anchor).unwrap_or_else(|| panic!("{name}: {anchor}"));
            let body = &src[at..];
            let end = body.find("
}
").unwrap();
            let body = &body[..end];
            assert!(body.contains("spawn_blocking"), "{name} {anchor} must open its dialog on a blocking thread");
        }
    }
}

#[test]
fn an_ffmpeg_older_than_the_export_graph_syntax_is_skipped_on_path() {
    assert_eq!(ffmpeg_major_version("ffmpeg version 7.1-essentials_build-www.gyan.dev Copyright (c) 2000-2024"), Some(7));
    assert_eq!(ffmpeg_major_version("ffmpeg version n6.1.1 Copyright (c) 2000-2023"), Some(6));
    assert_eq!(ffmpeg_major_version("ffmpeg version 9.0.1-essentials_build"), Some(9));
    assert_eq!(ffmpeg_major_version("ffmpeg version N-113000-gabcdef Copyright"), None);
    assert_eq!(ffmpeg_major_version("not an ffmpeg banner"), None);
    assert_eq!(ffmpeg_major_version(""), None);

    let root = std::env::temp_dir().join(format!("ps-pathff-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (old, new) = (root.join("old"), root.join("new"));
    fs::create_dir_all(&old).unwrap();
    fs::create_dir_all(&new).unwrap();
    fs::write(old.join("ffmpeg.exe"), b"x").unwrap();
    fs::write(new.join("ffmpeg.exe"), b"x").unwrap();
    let paths = std::env::join_paths([&old, &new]).unwrap();
    let only_new = |p: &Path| p.starts_with(&new);
    assert_eq!(first_usable_on_path(&paths, "ffmpeg.exe", only_new), Some(new.join("ffmpeg.exe")));
    assert_eq!(first_usable_on_path(&paths, "ffmpeg.exe", |_| false), None);
    assert_eq!(first_usable_on_path(&paths, "missing.exe", |_| true), None);
    let _ = fs::remove_dir_all(&root);

    // The real lookup applies that check to ffmpeg only; ffprobe has no version floor.
    let src = source!("../ffmpeg.rs");
    let finder = src.split("pub(crate) fn find_media_tool").nth(1).unwrap().split("pub(crate) fn ffmpeg_search_path").next().unwrap();
    assert!(finder.contains("path_ffmpeg_is_current"));
}

#[test]
fn an_old_data_folder_ffmpeg_moves_only_when_it_is_the_pinned_build() {
    use sha2::{Digest, Sha256};
    let root = std::env::temp_dir().join(format!("ps-oldtool-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (data, local) = (root.join("data"), root.join("local"));
    fs::create_dir_all(&data).unwrap();
    let bytes = b"pinned build bytes";
    let want: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();

    // The pinned copy moves, and the old place is empty afterwards.
    fs::write(data.join("ffmpeg.exe"), bytes).unwrap();
    assert_eq!(migrate_old_media_tool(&data.join("ffmpeg.exe"), &local, &want), OldToolFate::Moved);
    assert_eq!(fs::read(local.join("ffmpeg.exe")).unwrap(), bytes);
    assert!(!data.join("ffmpeg.exe").exists());

    // A planted or damaged file under the same name is deleted, never moved.
    fs::write(data.join("ffprobe.exe"), b"something else").unwrap();
    assert_eq!(migrate_old_media_tool(&data.join("ffprobe.exe"), &local, &want), OldToolFate::Dropped);
    assert!(!data.join("ffprobe.exe").exists());
    assert!(!local.join("ffprobe.exe").exists());

    // A copy already at the new place wins; the old one is removed.
    fs::write(data.join("ffmpeg.exe"), bytes).unwrap();
    assert_eq!(migrate_old_media_tool(&data.join("ffmpeg.exe"), &local, &want), OldToolFate::Dropped);
    assert!(!data.join("ffmpeg.exe").exists());

    // Nothing there, nothing to do.
    assert_eq!(migrate_old_media_tool(&data.join("ffmpeg.exe"), &local, &want), OldToolFate::Absent);

    // The pass leaves a data folder that is also the exe folder (portable) alone.
    fs::write(data.join("ffmpeg.exe"), b"portable copy").unwrap();
    migrate_old_media_tools(&data, Some(&data), Some(&local));
    assert!(data.join("ffmpeg.exe").exists());

    // The pinned hashes are the shipped executables' hashes.
    assert_eq!(FFMPEG_EXE_SHA256.len(), 64);
    assert_eq!(FFPROBE_EXE_SHA256.len(), 64);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn start_up_moves_old_media_tools_off_the_ui_thread() {
    let lib = source!("../lib.rs");
    assert!(lib.contains("migrate_old_media_tools"));
    let at = lib.find("migrate_old_media_tools").unwrap();
    assert!(lib[..at].rfind("std::thread::spawn").is_some_and(|t| at - t < 400), "hashing two 100 MB files must not block setup");
}

#[test]
fn every_progress_block_keeps_a_silent_looking_export_alive() {
    // GIF palettegen prints out_time_ms=N/A until the whole input is read; the block's closing
    // "progress=continue" line is the heartbeat that still arrives.
    assert_eq!(progress_tick("out_time_ms=2500000"), ProgressTick::Time(2500));
    assert_eq!(progress_tick("out_time_ms=-9223372036854775807"), ProgressTick::Time(0));
    assert_eq!(progress_tick("out_time_ms=N/A"), ProgressTick::Alive);
    assert_eq!(progress_tick("progress=continue"), ProgressTick::Alive);
    assert_eq!(progress_tick("progress=end"), ProgressTick::Alive);
    assert_eq!(progress_tick("frame=12"), ProgressTick::None);
    assert_eq!(progress_tick(""), ProgressTick::None);
    let src = source!("../ffmpeg.rs");
    let run = src.split("pub(crate) fn run_ffmpeg_to_staged").nth(1).unwrap();
    assert!(run.contains("progress_tick(&line)"), "the reader thread classifies every line");
}

#[test]
fn a_running_parts_save_is_marked_in_flight_and_quit_waits_for_it() {
    let part = |n: &str| RecordingPartInfo { path: n.to_string(), mic_wav_path: None };
    let shared = std::sync::Mutex::new(vec![part("ps-q6-inflight.mp4")]);
    let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    std::thread::scope(|s| {
        let shared = &shared;
        s.spawn(move || {
            with_parts_outside_lock(shared, |parts| {
                started_tx.send(()).unwrap();
                go_rx.recv().unwrap();
                parts.clear();
            })
        });
        started_rx.recv().unwrap();
        // The list is empty while the copy runs, yet the sweep must still see the part as taken.
        assert!(shared.lock().unwrap().is_empty());
        assert!(protected_take_paths().contains(&PathBuf::from("ps-q6-inflight.mp4")));
        // Quit gives up only after its bounded wait, and goes on as soon as the save is done.
        assert!(!wait_for_part_saves(std::time::Duration::from_millis(100)), "a save is still running");
        go_tx.send(()).unwrap();
        assert!(wait_for_part_saves(std::time::Duration::from_secs(20)));
    });
    // Both quit routes wait: the app's exit event and the take saved on quit.
    let lib = source!("../lib.rs");
    assert!(lib.contains("wait_for_part_saves(REC_PARTS_QUIT_WAIT)"));
    let media = source!("../media_files.rs");
    let quit = media.split("pub(crate) fn save_take_on_quit").nth(1).unwrap().split("pub(crate) async fn save_recording_parts").next().unwrap();
    assert!(quit.contains("wait_for_part_saves(REC_PARTS_QUIT_WAIT)"));
}

// No behavioural red: the old text check and a failing canonicalize both end in `false`; the
// difference is that canonicalize contacts the host first. So the contrast is on the source.
#[test]
fn the_recording_source_check_refuses_every_network_spelling_by_text() {
    let src = source!("../media_files.rs");
    let body = src.split("pub(crate) fn is_recording_source_location").nth(1).unwrap().split("\n}\n").next().unwrap();
    assert!(body.contains("is_network_path(&raw)"), "the one shared gate decides");
    assert!(!body.contains("starts_with(r\"\\\\\")"), "no private two-prefix check");
    let allowed = vec![std::env::temp_dir()];
    for p in [r"/\host\share\rec-1.mp4", r"\??\UNC\host\share\rec-1.mp4", r"\?\UNC\host\share\rec-1.mp4", r"\.\GLOBALROOT\Device\x\rec-1.mp4"] {
        assert!(!is_recording_source_location(&allowed, Path::new(p)), "{p}");
    }
}

#[test]
fn the_verified_pdfium_copy_is_held_open_so_it_cannot_be_swapped() {
    let dir = std::env::temp_dir().join(format!("ps-pdfhold-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pdfium-test.dll");
    fs::write(&path, b"expected bytes").unwrap();
    // Content that differs is refused, and the refused open keeps nothing locked.
    assert!(open_verified_pdfium(&path, b"other bytes").is_none());
    assert!(fs::write(&path, b"expected bytes").is_ok());
    let held = open_verified_pdfium(&path, b"expected bytes").expect("matches");
    assert!(fs::write(&path, b"swapped").is_err(), "no write while the handle is held");
    assert!(fs::remove_file(&path).is_err(), "no delete while the handle is held");
    assert!(fs::rename(&path, dir.join("moved.dll")).is_err(), "no rename while the handle is held");
    assert_eq!(fs::read(&path).unwrap(), b"expected bytes", "readers still get the file");
    drop(held);
    assert!(fs::remove_file(&path).is_ok());
    let _ = fs::remove_dir_all(&dir);
    // The lookup keeps the handle for the whole run, and never re-reads the path to verify.
    let src = source!("../images.rs");
    let body = &src[src.find("pub(crate) fn pdfium_lib_path").unwrap()..];
    assert!(body.contains("PDFIUM_HOLD.set("));
    assert!(!body.contains("fs::read(&cached)"));
}

#[test]
fn old_pdfium_copies_in_the_shared_temp_folder_are_removed_once_at_start() {
    let root = std::env::temp_dir().join(format!("ps-oldpdf-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let dir = root.join(brand::TEMP_DIR);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("pdfium-1.2.3.dll"), b"old").unwrap();
    fs::write(dir.join("pdfium-1.2.4.dll"), b"old").unwrap();
    fs::write(dir.join("pdfium-notes.txt"), b"keep").unwrap();
    fs::write(dir.join(format!("{}-ocr-1.png", brand::TEMP_PREFIX)), b"keep").unwrap();
    remove_old_temp_pdfium_in(&root);
    assert!(!dir.join("pdfium-1.2.3.dll").exists());
    assert!(!dir.join("pdfium-1.2.4.dll").exists());
    assert!(dir.join("pdfium-notes.txt").exists(), "only the dll copies go");
    assert!(dir.join(format!("{}-ocr-1.png", brand::TEMP_PREFIX)).exists(), "other scratch files are not this pass's business");
    // A missing folder is not an error.
    remove_old_temp_pdfium_in(&root.join("absent"));
    let _ = fs::remove_dir_all(&root);
    // The start-up setup runs the pass, and the snip sweep comment names where the copy lives now.
    let lib = source!("../lib.rs");
    assert!(lib.contains("remove_old_temp_pdfium()"));
    let snip = source!("../snip.rs");
    assert!(!snip.contains("next to the versioned pdfium cache"), "the comment no longer places the cache in TEMP");
}

#[test]
fn an_attachment_path_naming_the_store_or_a_network_share_is_refused() {
    let root = std::env::temp_dir().join(format!("ps-attach-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let data = root.join("data");
    fs::create_dir_all(data.join("files")).unwrap();
    fs::write(data.join("data.db"), b"db").unwrap();
    fs::write(data.join("data.db-wal"), b"wal").unwrap();
    fs::write(data.join("settings.json"), b"{}").unwrap();
    fs::write(data.join("files").join("copy.pdf"), b"pdf").unwrap();
    fs::write(root.join("mine.txt"), b"x").unwrap();
    // The store's own files directly in the data folder never feed a copy or an editor.
    assert!(attachment_path_refused(&data, &data.join("data.db").to_string_lossy()));
    assert!(attachment_path_refused(&data, &data.join("data.db-wal").to_string_lossy()));
    assert!(attachment_path_refused(&data, &data.join("settings.json").to_string_lossy()));
    // A dot-dot hop and a different spelling of the same file are the same file.
    let hop = format!("{}{}files{}..{}data.db", data.display(), std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR);
    assert!(attachment_path_refused(&data, &hop));
    // The app's own attachment copies and the user's own files stay fine.
    assert!(!attachment_path_refused(&data, &data.join("files").join("copy.pdf").to_string_lossy()));
    assert!(!attachment_path_refused(&data, &root.join("mine.txt").to_string_lossy()));
    // A missing file is not this gate's business (a moved file must stay re-savable), and empty means none.
    assert!(!attachment_path_refused(&data, &root.join("gone.txt").to_string_lossy()));
    assert!(!attachment_path_refused(&data, ""));
    // A network spelling is refused by text, before the disk is asked anything.
    assert!(attachment_path_refused(&data, r"\\host\share\a.mp4"));
    assert!(attachment_path_refused(&data, "//host/share/a.mp4"));
    let _ = fs::remove_dir_all(&root);

    // Every path that stores or uses an attachment asks the gate before touching it.
    let prompts = source!("../prompts.rs");
    for f in ["pub(crate) async fn update_prompt", "pub(crate) async fn add_prompt"] {
        let at = prompts.find(f).unwrap_or_else(|| panic!("{f}"));
        let body = &prompts[at..];
        let body = &body[..body.find("\n}\n").unwrap()];
        let gate = body.find("attachment_path_refused(").unwrap_or_else(|| panic!("{f} asks the gate"));
        let stage = body.find("staged_copies.stage(").expect("stage");
        assert!(gate < stage, "{f} checks before a copy is made");
    }
    let copy = source!("../copy.rs");
    let at = copy.find("pub(crate) async fn copy_prompt").unwrap();
    assert!(copy[at..].contains("attachment_path_refused("), "the file clipboard path asks again at use");
    let images = source!("../images.rs");
    let at = images.find("pub(crate) fn set_clipboard_file(path: &str)").unwrap();
    let head = &images[at..at + 300];
    assert!(head.find("is_network_path(").is_some_and(|n| n < head.find(".exists()").unwrap()), "text check before exists()");
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
    let vet = |src: &Path, ov: Option<&str>| vet_editor_open(&dirs, &temp_root, &[], src, ov);
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
    assert!(!export_source_is_allowed(&dirs, &temp_root, &editor, Path::new(r"\\host\share\a.mp4")));
    let _ = fs::remove_dir_all(&root);

    // The command vets before any is_file call and does the disk work on a blocking worker.
    let ve = source!("../video_editor.rs");
    let at = ve.find("pub(crate) async fn open_video_editor").unwrap();
    let body = &ve[at..];
    let body = &body[..body.find("\n}\n").unwrap()];
    let vet_at = body.find("vet_editor_open(").expect("vets the source");
    assert!(body[..vet_at].contains("spawn_blocking"), "the vetting runs on a blocking worker");
    assert!(!body[..vet_at].contains(".is_file()"), "no disk call before the vetting");
    assert!(body[..vet_at].contains("is_network_path("), "network spellings refused on text first");
    let allowed = &ve[ve.find("pub(crate) fn export_source_is_allowed").unwrap()..];
    let allowed = &allowed[..allowed.find("\n}\n").unwrap()];
    assert!(allowed.find("is_network_path(").is_some_and(|n| n < allowed.find("canonicalize(").unwrap()), "text check before the first canonicalize");
}
