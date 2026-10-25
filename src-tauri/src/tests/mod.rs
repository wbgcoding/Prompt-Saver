use super::*;
use crate::scheduler::Scheduler;
use std::time::Instant;

// The text-recognition epoch is one global counter: tests that read it or bump it run one
// at a time, or a parallel bump makes a pass look stale.
pub(super) static OCR_EPOCH_TEST_LOCK: Mutex<()> = Mutex::new(());

mod shots;
mod asset_scope;
mod ffmpeg_jobs;
mod rec_conversion;
mod video;
mod recording_audio;
mod prompts_voice;
mod md_sync_1;
mod md_sync_2;
mod storage_1;
mod storage_2;
mod diff_writes;
mod data_lock;
mod backup_import_1;
mod backup_import_2;
mod rotation_settings;
mod hotkeys_1;
mod hotkeys_2;
mod tray_pills;
mod timer;
mod placeholders;
#[cfg(windows)]
mod win_windows;
mod http_client;
mod data_dir_cleanup;
mod store_tests;
mod trash_tests;
mod paths_tests;
mod vault_tests;
mod parallel_tests;
#[cfg(windows)]
mod registry_tests;
mod crypto_fixtures;
mod media_files_tests;
mod video_editor_tests;
mod views_tests;
mod clipwatch_tests;
mod logging_tests;
mod stats_tests;
mod update_tests;
mod os_integration;

#[allow(unused_imports)]
pub(super) use shots::*;
#[allow(unused_imports)]
pub(super) use ffmpeg_jobs::*;
#[allow(unused_imports)]
pub(super) use rec_conversion::*;
#[allow(unused_imports)]
pub(super) use video::*;
#[allow(unused_imports)]
pub(super) use recording_audio::*;
#[allow(unused_imports)]
pub(super) use prompts_voice::*;
#[allow(unused_imports)]
pub(super) use md_sync_1::*;
#[allow(unused_imports)]
pub(super) use md_sync_2::*;
#[allow(unused_imports)]
pub(super) use storage_1::*;
#[allow(unused_imports)]
pub(super) use storage_2::*;
#[allow(unused_imports)]
pub(super) use diff_writes::*;
#[allow(unused_imports)]
pub(super) use data_lock::*;
#[allow(unused_imports)]
pub(super) use backup_import_1::*;
#[allow(unused_imports)]
pub(super) use backup_import_2::*;
#[allow(unused_imports)]
pub(super) use rotation_settings::*;
#[allow(unused_imports)]
pub(super) use hotkeys_1::*;
#[allow(unused_imports)]
pub(super) use hotkeys_2::*;
#[allow(unused_imports)]
pub(super) use tray_pills::*;
#[allow(unused_imports)]
pub(super) use timer::*;
#[allow(unused_imports)]
pub(super) use placeholders::*;
#[allow(unused_imports)]
pub(super) use data_dir_cleanup::*;
#[cfg(windows)]
#[allow(unused_imports)]
pub(super) use win_windows::*;
#[allow(unused_imports)]
pub(super) use store_tests::*;
#[allow(unused_imports)]
pub(super) use trash_tests::*;
#[allow(unused_imports)]
pub(super) use paths_tests::*;
#[allow(unused_imports)]
pub(super) use vault_tests::*;
#[allow(unused_imports)]
pub(super) use media_files_tests::*;
#[allow(unused_imports)]
pub(super) use video_editor_tests::*;
#[allow(unused_imports)]
pub(super) use views_tests::*;
#[allow(unused_imports)]
pub(super) use clipwatch_tests::*;
#[allow(unused_imports)]
pub(super) use logging_tests::*;
#[allow(unused_imports)]
pub(super) use stats_tests::*;
#[allow(unused_imports)]
pub(super) use update_tests::*;
#[allow(unused_imports)]
pub(super) use os_integration::*;
