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

// Authenticode-verify a staged file via WinVerifyTrust (WINTRUST_ACTION_GENERIC_VERIFY_V2, no
// UI, no network revocation check: it only has to prove a valid signature). The update path is
// fail-closed on a published SHA-256; this download is Microsoft's own bootstrapper with no such
// hash, so Authenticode is the equivalent integrity gate (the size cap and "MZ" check are only
// a corruption guard).
// The one publisher whose bootstrapper may be run: a valid chain alone would admit any signed
// program served at the download address.
pub(crate) fn is_microsoft_signer(name: &str) -> bool {
    name == "Microsoft Corporation"
}

// True only when the file is validly signed AND the signer is Microsoft.
#[cfg(windows)]
pub(crate) fn signed_by_microsoft(path: &std::path::Path) -> bool {
    match authenticode_check(path) {
        (true, Some(name)) => is_microsoft_signer(&name),
        _ => false,
    }
}

// (chain-trusted, display name of the signing certificate when it could be read).
#[cfg(windows)]
fn authenticode_check(path: &std::path::Path) -> (bool, Option<String>) {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Security::Cryptography::{CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE};
    use windows::Win32::Security::WinTrust::{
        WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
        WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
        WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY,
        WTD_UICONTEXT_EXECUTE, WTD_UI_NONE,
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
    // The signing certificate is read while the verify state is still open.
    let mut signer_name = None;
    if status == 0 {
        // SAFETY: after a successful VERIFY the state handle names a provider data
        // block that stays valid until the CLOSE call below; every pointer is
        // checked for null before it is followed.
        unsafe {
            let prov = WTHelperProvDataFromStateData(data.hWVTStateData);
            if !prov.is_null() {
                let signer = WTHelperGetProvSignerFromChain(prov, 0, false, 0);
                if !signer.is_null() && (*signer).csCertChain > 0 && !(*signer).pasCertChain.is_null() {
                    let cert = (*(*signer).pasCertChain).pCert;
                    if !cert.is_null() {
                        let mut buf = [0u16; 256];
                        let n = CertGetNameStringW(cert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, Some(&mut buf)) as usize;
                        if n > 1 {
                            signer_name = Some(String::from_utf16_lossy(&buf[..n - 1]));
                        }
                    }
                }
            }
        }
    }
    // WTD_STATEACTION_CLOSE releases WinVerifyTrust's own per-call state: required
    // after a WTD_STATEACTION_VERIFY call regardless of the verify result.
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe {
        let _ = WinVerifyTrust(HWND(std::ptr::null_mut()), &mut action, &mut data as *mut _ as *mut core::ffi::c_void);
    }
    // ERROR_SUCCESS: a validly (chain-trusted) signed file
    (status == 0, signer_name)
}

// Download the Microsoft Evergreen WebView2 bootstrapper to %TEMP% and run it.
// Returns true only if the installer reports success. Fully guarded: any failure
// falls back to the manual download link.
#[cfg(windows)]
pub(crate) fn install_webview2_runtime() -> bool {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    const BOOTSTRAPPER_URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
    let resp = match download_agent().get(BOOTSTRAPPER_URL)
        .header("User-Agent", brand::USER_AGENT)
        .call()
    {
        Ok(r) => r,
        Err(_) => return false,
    };
    const CAP: u64 = 8 * 1024 * 1024;
    let mut bytes = Vec::new();
    if resp
        .into_body()
        .into_reader()
        .take(CAP)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.is_empty()
    {
        return false;
    }
    // Same two guards as the update download: a file that exactly hit the cap was probably
    // truncated, and whatever we execute has to be a Windows executable. These only catch a
    // corrupt download; the Authenticode check below is the gate.
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
    // create_dir, not create_dir_all: a folder that already exists is refused, not adopted.
    if fs::create_dir(&dir).is_err() {
        return false;
    }
    // The staging folder goes on every way out of here, installer run or not.
    struct RemoveDir(PathBuf);
    impl Drop for RemoveDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = RemoveDir(dir.clone());
    let path = dir.join("MicrosoftEdgeWebview2Setup.exe");
    if fs::write(&path, &bytes).is_err() {
        return false;
    }
    // Integrity: only Microsoft's own chain-trust-signed bootstrapper is run. There is no
    // published hash to check it against, so Authenticode with the signer pinned to Microsoft is
    // the fail-closed gate.
    if !signed_by_microsoft(&path) {
        return false;
    }
    std::process::Command::new(&path)
        .args(["/install"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
