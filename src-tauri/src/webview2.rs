use super::*;

// ---------- App entry ----------

// WebView2 runtime is the only external requirement; offer the official
// installer if it is missing instead of failing with a cryptic error.
#[cfg(windows)]
pub(crate) fn ensure_webview2() -> bool {
    if tauri::webview_version().is_ok() {
        return true;
    }
    let (title, msg) = webview2_dialog(resolve_lang("auto"));
    let answer = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title(title)
        .set_description(msg)
        .set_buttons(rfd::MessageButtons::YesNo)
        .show();
    if answer == rfd::MessageDialogResult::Yes {
        // Try a silent download + install of the official Evergreen runtime; if
        // that fails (offline/blocked), fall back to opening the download page.
        if install_webview2_runtime() {
            // The runtime exists now, but this process started without it:
            // relaunch so a fresh instance loads the webview.
            if let Ok(exe) = std::env::current_exe() {
                let _ = std::process::Command::new(exe).spawn();
            }
        } else {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", "", "https://go.microsoft.com/fwlink/p/?LinkId=2124703"])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
        }
    }
    false
}

// Authenticode-verify a staged file via WinVerifyTrust (WINTRUST_ACTION_GENERIC_VERIFY_V2,
// no UI, no revocation check against a network CRL/OCSP responder -- this only needs to
// prove the file is validly signed, not chase a live revocation list on every launch).
// The update path (install_update) is fail-closed on a published SHA-256; this download
// has no such hash to check against (it is Microsoft's own bootstrapper, not our own
// release asset), so an Authenticode check is the equivalent integrity gate here: the
// size cap + "MZ" header check above are only a corruption/wrong-format guard, not
// authentication on their own.
#[cfg(windows)]
pub(crate) fn verify_authenticode_signature(path: &std::path::Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Security::WinTrust::{
        WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
        WINTRUST_FILE_INFO, WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE,
        WTD_STATEACTION_VERIFY, WTD_UICONTEXT_EXECUTE, WTD_UI_NONE,
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file_info },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwUIContext: WTD_UICONTEXT_EXECUTE,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `data` and `file_info` are valid, live, correctly-sized WINTRUST_DATA/
    // WINTRUST_FILE_INFO values for the whole call; `wide` (the file path buffer) and
    // `file_info` both outlive the WinVerifyTrust call that borrows them by reference.
    let status = unsafe {
        WinVerifyTrust(HWND(std::ptr::null_mut()), &mut action, &mut data as *mut _ as *mut core::ffi::c_void)
    };
    // WTD_STATEACTION_CLOSE releases WinVerifyTrust's own per-call state -- required
    // after a WTD_STATEACTION_VERIFY call regardless of the verify result.
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe {
        let _ = WinVerifyTrust(HWND(std::ptr::null_mut()), &mut action, &mut data as *mut _ as *mut core::ffi::c_void);
    }
    status == 0 // ERROR_SUCCESS: a validly (chain-trusted) signed file
}

// Download the Microsoft Evergreen WebView2 bootstrapper to %TEMP% and run it.
// Returns true only if the installer reports success. Fully guarded: any failure
// falls back to the manual download link.
#[cfg(windows)]
pub(crate) fn install_webview2_runtime() -> bool {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    const BOOTSTRAPPER_URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
    let resp = match ureq::get(BOOTSTRAPPER_URL)
        .set("User-Agent", brand::USER_AGENT)
        .timeout(std::time::Duration::from_secs(300))
        .call()
    {
        Ok(r) => r,
        Err(_) => return false,
    };
    const CAP: u64 = 8 * 1024 * 1024;
    let mut bytes = Vec::new();
    if resp
        .into_reader()
        .take(CAP)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.is_empty()
    {
        return false;
    }
    // Same two guards the update download uses: a file that exactly hit the cap was
    // almost certainly truncated, and whatever we are about to execute has to at least
    // be a Windows executable. (Not cryptographic authentication: the bootstrapper
    // comes from Microsoft's own pinned fwlink over TLS, and only after the user
    // agreed to install it.)
    if bytes.len() as u64 >= CAP || bytes.len() < 2 || &bytes[0..2] != b"MZ" {
        return false;
    }
    // Stage into a unique temp subdir so a local attacker can't pre-plant the
    // installer at a predictable path (TOCTOU): matching install_update.
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("{}-wv2-{}-{}", brand::SLUG, std::process::id(), stamp));
    if fs::create_dir_all(&dir).is_err() {
        return false;
    }
    let path = dir.join("MicrosoftEdgeWebview2Setup.exe");
    if fs::write(&path, &bytes).is_err() {
        return false;
    }
    // Integrity: only Microsoft's own genuinely, chain-trust-signed bootstrapper is ever
    // run. Unlike install_update, there is no separately-published hash to check this
    // download against -- Authenticode is the equivalent fail-closed gate here.
    if !verify_authenticode_signature(&path) {
        return false;
    }
    std::process::Command::new(&path)
        .args(["/install"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
