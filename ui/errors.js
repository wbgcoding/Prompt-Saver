// Maps the plain English error strings the backend returns to translated texts. Shared by
// the main window (core.js errText) and every satellite window (video, snip, ...), each
// with its own t(). No imports: a window that only needs this must not pull in core.js.
// The backend returns Err(String) with a plain English sentence. Known strings are mapped
// to a translated key here; anything not in the map (an unexpected error, or a plain code
// like "canceled" a caller checks itself) shows the generic errUnexpected text and keeps
// the raw string in the error log. tests/error_msg_check.py keeps this map and the Rust
// error strings in step.
export const ERROR_MSG_KEYS = {
  "cannot delete the last view": "errLastView",
  "theme name is empty": "errThemeNameEmpty",
  "theme name contains a control character": "errThemeNameControlChar",
  "group name is empty": "errGroupNameEmpty",
  "a group needs at least one view": "errGroupNeedsView",
  "tag name is empty": "errTagNameEmpty",
  "tag name contains a comma or a control character": "errTagNameInvalid",
  "installer too large / truncated": "errUpdaterTooLarge",
  "downloaded file is not a valid installer": "errUpdaterInvalidInstaller",
  "update hash mismatch: download rejected": "errUpdaterHashMismatch",
  "update signature invalid: download rejected": "errUpdaterSignature",
  "backup expands to more than 512 MB": "errBackupTooLarge",
  "store-too-large-for-backup": "errStoreTooLargeForBackup",
  "backup-schema-mismatch": "errBackupSchemaMismatch",
  "not in a folder": "errNotInFolder",
  "not a member of this folder": "errNotFolderMember",
  "color picker is off": "errColorPickerOff",
  "autostart is only supported on Windows": "errAutostartWindowsOnly",
  "delete-incomplete": "errDeleteIncomplete",
  "unsupported-export-format": "exportUnsupportedFormat",
  "recording-target-switch-not-supported": "recTargetSwitchUnsupported",
  "recording-target-switch-different-monitor": "recTargetSwitchDifferentMonitor",
  "recording-target-switch-invalid-window": "recTargetSwitchInvalidWindow",
  "recording-target-switch-timeout": "recTargetSwitchTimeout",
  "file-missing": "openFileMissing",
  "ffmpeg-missing": "recFfmpegMissingHint",
  "demo-mode-multiple-instances": "demoModeMultipleInstances",
  "dir-not-picked": "syncDirNotApplied",
  "recording-busy": "recBusy",
  "portable-update-manual": "errUpdatePortableManual",
  "recording-active": "errUpdateRecordingActive",
  "restore-recording-active": "errRestoreRecordingActive",
  "jobs-running": "errUpdateJobsRunning",
  "ffmpeg-busy": "ffmpegDownloadBusy",
  "save-failed": "persistFailed",
  "remote-off-not-saved": "remoteOffNotSaved",
  "remote-tls-short-lived-has-own-command": "remoteTlsShortLivedOwnCommand",
  "remote-network-off-not-saved": "remoteNetworkOffNotSaved",
  "port-unavailable": "remotePortUnavailable",
  "the certificate key could not be protected": "remoteTlsKeyNotProtected",
  "corrupt backup": "errCorruptBackup",
  "unknown backup format": "errUnknownBackupFormat",
  "this folder holds the app's own data": "errFolderIsAppData",
  "the folder is no longer there": "errFolderGone",
  "the prompts could not be saved to the folder being left": "errSyncLeavePrompts",
  "the settings could not be saved to the folder being left": "errSyncLeaveSettings",
  "the data folder is still in use": "errDataDirBusy",
  "this library already has prompts": "errTakeoverHasPrompts",
  "not a text attachment": "errNotTextAttachment",
  "this release is not newer than the installed version": "errUpdateNotNewer",
  "update check failed": "errUpdateCheckFailed",
  "download too large / truncated": "errDownloadTooLarge",
  "downloaded file is not a valid archive": "errDownloadInvalidArchive",
  "ffmpeg download hash mismatch, rejected": "errFfmpegHashMismatch",
  "extracted file is not a valid executable": "errFfmpegInvalidExe",
  "release notes too large / truncated": "errReleaseNotesTooLarge",
  "password too long": "errPasswordTooLong",
  "backup-name-taken": "errBackupNameTaken",
  "folder not found": "errFolderNotFound",
  "recording file not found": "errRecordingFileNotFound",
  "color must be #rrggbb": "errColorInvalid",
  "not a hex color": "errColorInvalid",
  "a folder needs at least two {}": "errFolderNeedsTwo",
  "max {} folders": "errMaxFolders",
  "max {} views": "errMaxViews",
  "max {} custom themes": "errMaxThemes",
  "more than {} tags on one prompt": "errTooManyTags",
  "tag name longer than {} characters": "errTagTooLong",
  "theme name longer than {} characters": "errThemeNameTooLong",
  "note longer than {} characters": "errNoteTooLong",
  "folder name longer than {} characters": "errFolderNameTooLong",
  "no free cell": "errNoFreeCell",
  "unknown view": "errGone",
  "unknown folder": "errGone",
  "unknown {}": "errGone",
  "theme not found": "errGone",
  "not-found": "errGone",
  "invalid theme file": "errThemeFileInvalid",
  "unknown base theme": "errThemeFileInvalid",
  "unknown theme variable '{}'": "errThemeFileInvalid",
  "backup encryption failed": "errBackupEncryptFailed",
  "bad-name": "errBackupBadName",
  "missing": "errBackupMissing",
  "readonly": "errDataFolderLocked",
  "bad-file": "errSettingsFileInvalid",
  "no-store": "errNoStoreToBackup",
  "wrong-password": "errWrongPassword",
  "invalid-password": "errPasswordRequired",
  "password-required": "errPasswordRequired",
  "locked": "errVaultLocked",
  "secret-locked": "errVaultLocked",
  "secret-wrong-key": "errVaultWrongKey",
  "corrupt-secret": "errVaultCorrupt",
  "corrupt-vault": "errVaultCorrupt",
  "safety-snapshot-failed: {e}": "errSafetySnapshot",
  "copy store: {}": "errCopyStore",
  "no {} found": "errNothingToImport",
  "the file is not valid JSON": "errImportFileInvalid",
  "the file is not valid UTF-8 text": "errImportFileInvalid",
  "the new folder could not be written ({write_failures} files), so nothing was deleted": "errSyncWriteFolder",
  "a camera recording is already active": "errCameraActive",
  "no active camera recording": "errNoCameraRecording",
  "cameraRecord is off": "errCameraOff",
  "capture failed": "errCaptureFailed",
  "encode failed": "errCaptureFailed",
  "empty image": "errCaptureFailed",
  "blocked": "errCaptureBlocked",
  "clipboard write failed": "errClipboardFailed",
  "empty region": "errRegionEmpty",
  "region has no size": "errRegionEmpty",
  "could not decode image": "errImageDecode",
  "image is too large ({} MB); the limit is {} MB": "errImageTooLarge",
  "file is too large ({} MB); the limit is {} MB": "errImageTooLarge",
  "window is no longer valid": "errWindowGone",
  "window has no visible size": "errWindowGone",
  "could not read monitor bounds": "errScreenLayout",
  "could not resolve a monitor for this region": "errScreenLayout",
  "could not read the combined display size": "errScreenLayout",
  "no active recording": "errNoRecording",
  "not recording": "errNoRecording",
  "not paused": "errNoRecording",
  "nothing to stop": "errNoRecording",
  "nothing to cancel": "errNoRecording",
  "{source} audio is not active this recording": "errAudioNotActive",
  "export destination not recognized": "errExportDest",
  "could not read the recording": "errRecordingUnreadable",
  "ffprobe failed to read the recording": "errRecordingUnreadable",
  "ffprobe output: {e}": "errRecordingUnreadable",
  "could not start ffmpeg: {e}": "errToolStart",
  "could not start ffprobe: {e}": "errToolStart",
  "ffmpeg-timeout": "errFfmpegTimeout",
  "mic track mux failed: {}": "errMicMux",
  "no audio on track {track}": "errNoAudioTrack",
  "still-frame extraction failed": "errThumbFailed",
  "thumbnail extraction failed": "errThumbFailed",
  "thumbnail cache: {e}": "errThumbFailed",
  "unsupported": "errUnsupportedFormat",
  "unsupported container": "errUnsupportedFormat",
  "unsupported format": "errUnsupportedFormat",
  "unsupported image type": "errUnsupportedFormat",
  "nothing to convert": "errNothingToProcess",
  "nothing to crop": "errNothingToProcess",
  "download: {}": "errDownloadFailed",
  "read: {}": "errDownloadFailed",
  "write: {}": "errUnpackFailed",
  "zip: {}": "errUnpackFailed",
  "extract: {}": "errUnpackFailed",
  "extract: panicked": "errUnpackFailed",
  "target dir: {}": "errUnpackFailed",
  "ffmpeg.exe not found inside the downloaded archive": "errUnpackFailed",
  "stage dir: {}": "errUnpackFailed",
  "save installer: {}": "errUnpackFailed",
  "save script: {}": "errUnpackFailed",
  "start installer: {}": "errInstallerStart",
  "could not start the elevated process (ShellExecuteW code {})": "errInstallerStart",
  "no usable checksum published for this update, download rejected": "errUpdateNoChecksum",
  "not a file": "errNotAFile",
  "no certificate": "errNoCertificate",
  "no-certificate": "errNoCertificate",
  "remote access is off": "errRemoteOff",
  "open-failed": "errOpenFailed",
  "recording or mic sidecar not found": "errRecordingFileNotFound",
  "not a {} backup": "errUnknownBackupFormat",
  "snapshot-failed": "persistFailed",
  "history-failed": "persistFailed",
};
// Entries whose key holds a "{...}" part (the Rust side formats a value into the
// sentence) match any text in that spot; the captured values fill {n}, then {m}, in
// the translated text. Longest key first, so the most specific template wins.
const ERROR_TEMPLATES = Object.entries(ERROR_MSG_KEYS)
  .filter(([k]) => k.includes("{"))
  .sort((a, b) => b[0].length - a[0].length)
  .map(([k, key]) => [
    new RegExp("^" + k.split(/\{[^}]*\}/).map((x) => x.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("(.+?)") + "$", "s"),
    key,
  ]);

// The text a person should see for a backend error: the translated message for a known
// string, the generic errUnexpected text for anything else, never the raw string.
// `t(key)` is the calling window's own lookup.
export function errMessage(err, t) {
  return resolveErrMessage(err, t).msg;
}

function resolveErrMessage(err, t) {
  const s = String(err);
  let key = ERROR_MSG_KEYS[s];
  let args = [];
  if (!key) {
    for (const [re, k] of ERROR_TEMPLATES) {
      const m = re.exec(s);
      if (m) { key = k; args = m.slice(1); break; }
    }
  }
  let msg = t(key || "errUnexpected");
  if (args.length) msg = msg.replace("{n}", () => args[0]); // a function: `$&` in a path is no pattern
  if (args.length > 1) msg = msg.replace("{m}", () => args[1]);
  return { msg, known: !!key };
}

// Same text as errMessage. When the error is not in the map, the person is told the
// details are in the error log, so the raw string is written there too (the same
// log_error payload the main window's errText sends). For the satellite windows that
// show a message without going through the main window's errToast. Fire-and-forget: a
// failure to log must never raise another error.
export function errMessageLogged(err, t, origin) {
  const { msg, known } = resolveErrMessage(err, t);
  if (!known) {
    try {
      const invoke = window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke;
      if (invoke) invoke("log_error", { text: msg, origin: origin || "main", raw: String(err) }).catch(() => {});
    } catch {
      // logging is best effort
    }
  }
  return msg;
}
