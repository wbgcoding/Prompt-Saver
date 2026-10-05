// The unit-test process's own environment. A constructor that runs before the
// test harness starts any thread points the roaming and local app-data folders
// at a per-process sandbox, so no code path a test reaches can resolve into the
// owner's real data folder (the error log, the store fallbacks), and the temp
// folder there too, so two test runs at the same time, or leftovers an earlier
// run did not clean up, can never meet in a shared folder. The guard test in
// `tests.rs` proves it.

use std::ffi::OsString;
use std::path::PathBuf;

// Name of the variable the sandbox keeps the untouched roaming-folder path in.
pub(crate) const REAL_APPDATA_ENV: &str = "PROMPT_SAVER_TEST_REAL_APPDATA";

// The roaming folder this machine really has, whatever the sandbox points the
// APPDATA variable at.
pub(crate) fn real_appdata() -> Option<OsString> {
    std::env::var_os(REAL_APPDATA_ENV).or_else(|| std::env::var_os("APPDATA"))
}

// Name of the variable holding the sandbox folder once it exists (the temp
// folder itself moves into it, so it cannot be derived from `temp_dir()` again).
const SANDBOX_ENV: &str = "PROMPT_SAVER_TEST_SANDBOX";

// The per-process sandbox folder, named after the pid so two test runs at the
// same time never share one.
fn sandbox_root() -> PathBuf {
    match std::env::var_os(SANDBOX_ENV) {
        Some(root) => PathBuf::from(root),
        None => std::env::temp_dir().join(format!("ps-testenv-{}", std::process::id())),
    }
}

extern "C" fn remove_sandbox() {
    let _ = std::fs::remove_dir_all(sandbox_root());
}

extern "C" fn enter_sandbox() {
    // A pid the sandbox is created for may have been used by an earlier run
    // that died before it could clean up: start from nothing.
    let root = sandbox_root();
    let _ = std::fs::remove_dir_all(&root);
    let roaming = root.join("Roaming");
    let local = root.join("Local");
    let temp = root.join("Temp");
    if [&roaming, &local, &temp].iter().any(|d| std::fs::create_dir_all(d).is_err()) {
        return;
    }
    // Sandboxes of runs that were killed outright are not removed by their own
    // exit hook: drop the ones a day old, never a young one another run may use.
    if let Ok(entries) = std::fs::read_dir(root.parent().unwrap_or(&root)) {
        for e in entries.flatten() {
            let old = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age.as_secs() > 24 * 3600);
            if old && e.file_name().to_string_lossy().starts_with("ps-testenv-") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    if let Some(real) = std::env::var_os("APPDATA") {
        // SAFETY: runs before main, so nothing else is reading the environment.
        unsafe { std::env::set_var(REAL_APPDATA_ENV, real) };
    }
    unsafe {
        std::env::set_var("APPDATA", &roaming);
        std::env::set_var("LOCALAPPDATA", &local);
        std::env::set_var("TEMP", &temp);
        std::env::set_var("TMP", &temp);
        std::env::set_var(SANDBOX_ENV, &root);
    }
    extern "C" {
        fn atexit(cb: extern "C" fn()) -> i32;
    }
    unsafe { atexit(remove_sandbox) };
}

// MSVC runs the pointers in the `.CRT$XCU` section as C initialisers before
// `main`, which is the earliest point the process environment can be changed.
#[used]
#[link_section = ".CRT$XCU"]
static ENTER_SANDBOX: extern "C" fn() = enter_sandbox;
