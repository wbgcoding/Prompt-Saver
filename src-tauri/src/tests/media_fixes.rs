use super::*;

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
    let src = include_str!("../ffmpeg.rs");
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
    let src = include_str!("../snip.rs");
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
    let src = include_str!("../ffmpeg.rs");
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
    let src = include_str!("../video_editor.rs");
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
    let src = include_str!("../video_editor.rs");
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
    let src = include_str!("../video_editor.rs");
    let hook = src.find("tauri::WindowEvent::Destroyed").expect("the editor window must watch its own destruction");
    assert!(src[hook..hook + 200].contains("refuse_pending_editor_requests"));
}

// Structural fix, no red unit test: the rect comes from a live window enumeration, which a unit
// test cannot fake. The pure choice (frame over client) is covered by snip_window_rect_prefers_the_frame.
#[test]
fn the_window_fallback_crops_the_frame_the_overlay_showed() {
    let src = include_str!("../capture.rs");
    let at = src.find("Target's global physical rect").expect("rect resolution block");
    assert!(src[at..at + 500].contains("snip_window_rect(window_frame_rect(id)"));
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
    let js = include_str!("../../../ui/camera.js");
    assert!(js.contains("invoke(\"camera_permission_ready\")"), "the page must query the flag after listening");
    assert!(!js.contains("}, 1500)"), "no timer may count the permission as given");
    let lib = include_str!("../lib.rs");
    assert!(lib.contains("camera::camera_permission_ready"));
    camera::camera_permission_ready();
}

// Structural fix, no red unit test: a refused thread spawn cannot be provoked in a test.
#[test]
fn a_refused_audio_thread_is_a_start_error_not_a_panic() {
    let audio = include_str!("../audio.rs");
    assert!(audio.contains("-> std::io::Result<thread::JoinHandle<()>>"));
    assert!(!audio.contains("expect(\"spawn rec-audio thread\")"));
    let rec = include_str!("../recording.rs");
    assert_eq!(rec.matches("audio_stop.clone().unwrap(),\n        ).map_err(|e| e.to_string())?").count(), 2);
}

// Structural fix, no red unit test: the encoder needs a real media session to fail on demand.
#[test]
fn the_feeder_stops_feeding_after_the_first_encoder_error() {
    let rec = include_str!("../recording.rs");
    let drain = rec.split("fn drain_pending_frames(").nth(1).unwrap().split("
}
").next().unwrap();
    assert!(drain.contains("shared.request_stop();") && drain.contains("return;"), "the drain must leave at the first error");
    assert!(rec.contains("!clock.is_paused() && shared.peek_encode_error().is_none()"), "hold and audio sends must stop after an error");
}

// Structural fix, no red unit test: the old shape could not fail at run time, which is the finding.
#[test]
fn stopping_and_cancelling_a_recording_return_plain_values() {
    let rec = include_str!("../recording.rs");
    assert!(rec.contains("pub fn stop(session: RecSession) -> StopResult {"));
    assert!(rec.contains("pub fn cancel(session: RecSession) {"));
    assert!(rec.contains("fn finish_and_stop(mut session: RecSession) -> (u64, Option<String>) {"));
    let cmds = include_str!("../rec_cmds.rs");
    assert!(!cmds.contains("recording::cancel(session)?"));
    assert!(!cmds.contains("= stopped?;"));
}

// Structural fix, no red unit test: only comments and an attribute moved.
#[test]
fn the_follow_comment_and_cfg_sit_on_the_follow_function() {
    let bar = include_str!("../rec_bar.rs");
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
    let src = include_str!("../ffmpeg.rs");
    let after = src.split("let status = match child.wait() {").nth(1).unwrap();
    let err_arm = after.split("let stderr_text").next().unwrap();
    assert!(err_arm.contains("forget_staged();") && err_arm.contains("fs::remove_file(&staged)"));
}

// Structural fix, no red unit test: a panic inside the export task cannot be provoked here.
#[test]
fn a_failed_export_task_still_closes_its_job_row() {
    let src = include_str!("../video_editor.rs");
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
    let src = include_str!("../video_editor.rs");
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
    let src = include_str!("../capture.rs");
    let body = &src[src.find("pub(crate) async fn capture_window(").unwrap()..src.find("pub(crate) struct WindowProbe").unwrap()];
    assert_eq!(body.matches("xcap::Window::all()").count(), 0, "capture_window must go through probe_window");
    let missing = crate::capture::probe_window(u32::MAX, true);
    assert!(missing.app_name.is_none() && missing.direct.is_none() && missing.rect.is_none());
}

// Structural fix, no red unit test: the cost is a 7 MB read per call, not a visible result.
#[test]
fn the_verified_pdfium_copy_is_remembered_after_the_first_check() {
    let src = include_str!("../images.rs");
    let body = &src[src.find("pub(crate) fn pdfium_lib_path").unwrap()..];
    let lookup = body.find("PDFIUM_VERIFIED.get()").expect("lookup");
    let read = body.find("fs::read(&cached)").expect("content check");
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
    let src = include_str!("../media_files.rs");
    assert!(!src.contains("with_extension(\"keys.json\")"));
    assert_eq!(crate::media_files::sidecar_path(Path::new("a/rec.mp4"), crate::media_files::KEYS_SIDECAR_EXT), Path::new("a/rec.keys.json"));
}

// Structural fix, no red unit test: a write that fails half-way cannot be provoked on demand.
#[test]
fn the_ocr_temp_guard_exists_before_the_png_is_written() {
    let src = include_str!("../ocr.rs");
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
    let src = include_str!("../ocr_cmds.rs");
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
    let src = include_str!("../ocr_cmds.rs");
    let body = src.split("pub(crate) async fn ocr_index_backfill").nth(1).unwrap().split("pub(crate) async fn ocr_capabilities(").next().unwrap();
    let at = body.find("Err(e) => {").expect("join error arm");
    let arm = &body[at..at + 200];
    assert!(arm.contains("push_bg_op(&app, \"ocr-index\", \"ocr-index\", \"error\""), "{arm}");
}

// Structural fix, no red unit test: install_permission_handler cannot be made to fail in a test.
#[test]
fn a_camera_window_without_its_permission_handler_is_destroyed() {
    let src = include_str!("../camera.rs");
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
