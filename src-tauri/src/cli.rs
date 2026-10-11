// Command line: `prompt-saver <command> ...`. A launch whose first argument
// names one of the commands below never opens a window. If the app is already
// running, the launch hands its arguments to that instance over a named pipe
// and prints whatever it answers; otherwise it opens the store itself, runs
// the command alone, prints the result and exits. Either way exactly one
// process ever touches the database at a time.
//
// Layout: `parse()` turns argv into a `Cmd`; `exec()` is the pure core that
// runs a `Cmd` against a `Store` already in memory (used by both the
// standalone path and the already-running instance, so the two can never
// drift apart); `run_standalone()` and `run_in_process()` are the two thin
// callers that load/persist the store around it; the pipe framing and the
// launch-side dispatcher sit at the top.

use crate::{
    acquire_data_dir_lock, append_crash_log, append_imported_prompts, backup_db_file, canonical_tags,
    data_dir, data_dir_headless, data_dir_lock_enabled, db_conn_at, db_load, db_load_trash,
    db_write_prompts, db_write_prompts_and_trash, db_write_settings, do_backup_at, gen_id,
    gfs_from_settings, list_backups_impl_at, lock, migrate_prompts, parse_import_bytes,
    parse_ui_text_map, read_file_capped, resolved_for_export, restore_selective, sanitize_notes,
    sanitize_tags, show_main, stage_and_validate_backup, tag_key, to_csv, to_json, to_txt,
    upgrade_placeholders, upgrade_prompt_placeholders, valid_backup_name, valid_scope, valid_tag_color, valid_tag_name, Db, Prompt,
    Store, WrittenRows, MAX_IMPORT_BYTES,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};

// ---------- Launch-side entry point ----------

const SUBCOMMANDS: &[&str] = &[
    "list", "get", "copy", "add", "edit", "delete", "restore", "tag", "view", "search", "export",
    "import", "backup", "settings", "version", "describe",
];

fn looks_like_cli(argv: &[String]) -> bool {
    match argv.first().map(String::as_str) {
        Some(a) if SUBCOMMANDS.contains(&a) => true,
        Some("--help" | "-h" | "--version") => true,
        _ => false,
    }
}

// Called at the very top of run(), before anything Tauri-related exists.
// `Some(code)` means: exit with this code, no window was ever created.
// `None` means: this launch is an ordinary GUI start, carry on as before.
pub(crate) fn dispatch_from_launch_args() -> Option<i32> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if !looks_like_cli(&argv) {
        return None;
    }
    #[cfg(windows)]
    attach_parent_console_if_needed();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let rt = tokio::runtime::Builder::new_current_thread().enable_io().enable_time().build();
    if let Ok(rt) = rt {
        if let Some(code) = rt.block_on(try_forward(&argv, &cwd)) {
            return Some(code);
        }
    }
    Some(run_standalone(&argv, &cwd))
}

// A windows_subsystem="windows" release build starts with no console at all,
// so a second launch typed into an existing terminal would otherwise print
// nothing. Attaching to the caller's own console (and only that -- a debug
// build, or any launch that already has a real std handle, is left alone)
// is what makes a command's output show up there instead of vanishing.
#[cfg(windows)]
fn attach_parent_console_if_needed() {
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING,
    };
    use windows::Win32::System::Console::{
        AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
        STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    unsafe {
        // Already has a working handle (debug build, or a console of its own) --
        // attaching again would only fail and must never disturb it.
        if let Ok(h) = GetStdHandle(STD_OUTPUT_HANDLE) {
            if !h.is_invalid() {
                return;
            }
        }
        // No parent console to attach to (started from Explorer, a shortcut, or
        // the Task Scheduler) -- stays exactly as silent as it always has.
        if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
            return;
        }
        let open = |name: &str| -> windows::core::Result<windows::Win32::Foundation::HANDLE> {
            let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            CreateFileW(
                windows::core::PCWSTR(wide.as_ptr()),
                (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                Default::default(),
                None,
            )
        };
        if let Ok(h) = open("CONOUT$") {
            let _ = SetStdHandle(STD_OUTPUT_HANDLE, h);
            let _ = SetStdHandle(STD_ERROR_HANDLE, h);
        }
        if let Ok(h) = open("CONIN$") {
            let _ = SetStdHandle(STD_INPUT_HANDLE, h);
        }
    }
}

// ---------- Named pipe transport ----------
//
// The single-instance plugin (see run()) already forwards a second launch's
// raw argv to the running instance, but only one way -- it has no channel for
// an answer. This pipe is that channel, and it is simpler to just use it for
// the whole round trip: the second process connects to it directly instead of
// going through the plugin at all, so it keeps full control of its own exit
// code and never has to fight the plugin's unconditional exit(0).

fn pipe_name() -> String {
    format!(r"\\.\pipe\{}-cli", crate::brand::IDENTIFIER)
}

const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
// Windows' own busy-pipe error code (ERROR_PIPE_BUSY); retried instead of
// treated as "nobody is listening".
const ERROR_PIPE_BUSY: i32 = 231;

#[derive(serde::Serialize, serde::Deserialize)]
struct PipeRequest {
    argv: Vec<String>,
    cwd: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PipeResponse {
    code: i32,
    stdout: String,
    stderr: String,
}

async fn write_frame<W: tokio::io::AsyncWrite + Unpin>(w: &mut W, bytes: &[u8]) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    let len = u32::try_from(bytes.len()).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "frame too large"))?;
    w.write_all(&len.to_le_bytes()).await?;
    w.write_all(bytes).await?;
    w.flush().await
}

async fn read_frame<R: tokio::io::AsyncRead + Unpin>(r: &mut R, max: usize) -> std::io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > max {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await?;
    Ok(buf)
}

// Tries the already-running instance. `None` means nothing answered (no pipe
// at all, or a real error opening it) -- the caller falls back to standalone.
// `Some(code)` means an answer was printed and this IS the launch's exit code.
async fn try_forward(argv: &[String], cwd: &Path) -> Option<i32> {
    use tokio::net::windows::named_pipe::ClientOptions;
    let name = pipe_name();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut client = loop {
        match ClientOptions::new().open(&name) {
            Ok(c) => break c,
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                if std::time::Instant::now() >= deadline {
                    return None;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Err(_) => return None, // nobody is listening -- run standalone instead
        }
    };
    let req = PipeRequest { argv: argv.to_vec(), cwd: cwd.to_string_lossy().to_string() };
    let bytes = serde_json::to_vec(&req).ok()?;
    let roundtrip = async {
        write_frame(&mut client, &bytes).await.ok()?;
        let resp_bytes = read_frame(&mut client, MAX_FRAME_BYTES).await.ok()?;
        serde_json::from_slice::<PipeResponse>(&resp_bytes).ok()
    };
    match tokio::time::timeout(std::time::Duration::from_secs(5), roundtrip).await {
        Ok(Some(resp)) => {
            if !resp.stdout.is_empty() {
                print!("{}", resp.stdout);
            }
            if !resp.stderr.is_empty() {
                eprint!("{}", resp.stderr);
            }
            Some(resp.code)
        }
        // Connected, but no readable answer within the time limit -- the store
        // is effectively unreachable from here right now, same standing as a
        // foreign machine's fresh data-dir lock (exit 3).
        _ => {
            eprintln!("prompt-saver: no answer from the running instance within 5 seconds");
            Some(3)
        }
    }
}

// Started once from run()'s setup(), independent of the single-instance
// plugin: `first_pipe_instance` makes a second app instance's attempt to
// start a second server fail cleanly (relevant only with "allow multiple
// instances" on), so exactly one running copy ever answers the pipe.
// Every tokio named-pipe handle registers itself with the runtime's reactor the
// moment it is created, so creating one on the plain setup() thread panics the
// whole process before a window ever exists. The first server instance is
// therefore created inside the spawned task, exactly like every later one.
pub(crate) fn install_server(app: &AppHandle) {
    use tokio::net::windows::named_pipe::ServerOptions;
    let name = pipe_name();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut server = match ServerOptions::new().first_pipe_instance(true).create(&name) {
            Ok(s) => s,
            Err(_) => return, // another instance already owns the pipe; nothing to do here
        };
        loop {
            if server.connect().await.is_err() {
                return;
            }
            let connected = server;
            // Queue the next instance before handing this one off, so a client
            // that connects while this request is still running is accepted
            // immediately instead of racing the still-busy pipe.
            server = match ServerOptions::new().create(&name) {
                Ok(s) => s,
                Err(_) => return,
            };
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move { handle_connection(connected, app2).await });
        }
    });
}

async fn handle_connection(mut pipe: tokio::net::windows::named_pipe::NamedPipeServer, app: AppHandle) {
    let Ok(req_bytes) = read_frame(&mut pipe, MAX_FRAME_BYTES).await else { return };
    let Ok(req) = serde_json::from_slice::<PipeRequest>(&req_bytes) else { return };
    if req.argv == [SHOW_PING.to_string()] {
        respond_to_show_ping(&mut pipe, &app, true).await;
        return;
    }
    if req.argv == [ALIVE_PING.to_string()] {
        respond_to_show_ping(&mut pipe, &app, false).await;
        return;
    }
    let (code, stdout, stderr, restart) = run_in_process_deferred(&app, req.argv, PathBuf::from(req.cwd)).await;
    let resp = PipeResponse { code, stdout, stderr };
    let answer = async {
        if let Ok(bytes) = serde_json::to_vec(&resp) {
            let _ = write_frame(&mut pipe, &bytes).await;
        }
    };
    answer_then_restart(answer, restart, || app.restart()).await;
}

// A second GUI launch's way of proving this instance is not just accepting
// connections but genuinely alive: the answer is sent from inside the main
// thread's own closure, not as soon as the closure is queued (queuing always
// succeeds even when the main thread itself is wedged -- see the tray-window
// comment above on the same hazard). It is sent BEFORE show_main does its
// work, because raising a window -- and rebuilding a missing one, webview and
// all -- can easily outlast the launch side's patience, and a slow window is
// not a hung process. Reaching the closure at all is the liveness proof. A
// promptsaver:// link pings with `raise` false: the link's own point is to
// copy or open quietly, never to pull a window to the front.
async fn respond_to_show_ping(
    pipe: &mut tokio::net::windows::named_pipe::NamedPipeServer,
    app: &AppHandle,
    raise: bool,
) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let app2 = app.clone();
    if app
        .run_on_main_thread(move || {
            let _ = tx.send(());
            if raise {
                show_main(&app2);
            }
        })
        .is_ok()
    {
        let _ = rx.await; // no timeout here -- the launch-side ping enforces its own
    }
    let resp = PipeResponse { code: 0, stdout: String::new(), stderr: String::new() };
    if let Ok(bytes) = serde_json::to_vec(&resp) {
        let _ = write_frame(pipe, &bytes).await;
    }
}

// ---------- Second-launch handshake (a plain GUI start, not a CLI command) ----------
//
// A second launch that is not a CLI command pings the same pipe a CLI launch
// uses instead of trusting the single-instance plugin's own unconditional
// forward-and-exit -- see run()'s own comment on why. `argv` is the launch's
// raw arguments (already known not to look like a CLI command by the time
// run() calls this). A promptsaver:// link runs the exact same handshake,
// with a ping that never raises a window: the answer still has to reach the
// single-instance plugin (a live instance) or this process's own setup (a
// takeover, or nobody running) -- it never ends the launch here on its own.

const SHOW_PING: &str = "\0show-ping\0"; // never a valid subcommand, path or link
const ALIVE_PING: &str = "\0alive-ping\0"; // liveness only, never raises a window
const SECOND_LAUNCH_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

#[derive(Debug, PartialEq)]
pub(crate) enum LaunchDecision {
    // Nothing is listening -- this launch is the primary instance.
    Primary,
    // The running instance answered (or this is a case left to the
    // single-instance plugin, e.g. a deep link) -- this launch is done.
    HandedOff,
    // The running instance did not answer in time and was taken over --
    // this launch continues on as the new primary instance.
    TookOver,
}

// The one-shot runtime the handshake runs on, shared by both branches below.
fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread().enable_io().enable_time().build()
}

// A link must reach the single-instance plugin (a live instance) or this
// process's own setup (a takeover, or nobody running) -- it never ends here.
fn link_decision(handshake: LaunchDecision) -> LaunchDecision {
    match handshake {
        LaunchDecision::HandedOff => LaunchDecision::Primary,
        other => other,
    }
}

pub(crate) fn resolve_second_launch(argv: &[String]) -> LaunchDecision {
    // A link never ends this launch silently: HandedOff (the running instance
    // is alive) is turned back into Primary, so the caller falls through to
    // the single-instance plugin, which forwards the link and exits this
    // process, or to this process's own first-launch setup.
    if argv.iter().any(|a| a.starts_with("promptsaver://")) {
        let Ok(rt) = runtime() else { return LaunchDecision::Primary };
        return link_decision(rt.block_on(resolve_second_launch_async(ALIVE_PING)));
    }
    let Ok(rt) = runtime() else {
        return LaunchDecision::HandedOff; // could not even try -- behave as before
    };
    rt.block_on(resolve_second_launch_async(SHOW_PING))
}

async fn resolve_second_launch_async(ping: &str) -> LaunchDecision {
    use tokio::net::windows::named_pipe::ClientOptions;
    let name = pipe_name();
    // Only two outcomes end this launch: a clear answer (hand off) or a proven
    // hang that was ended (take over). Everything else -- a busy pipe, a
    // connection that breaks under us, a process that cannot be identified --
    // falls through to Primary and lets the single-instance plugin decide
    // exactly as it did before this handshake existed. Exiting on a doubt is
    // what leaves the user with no window at all.
    let connect_deadline = std::time::Instant::now() + SECOND_LAUNCH_TIMEOUT;
    let mut client = loop {
        match ClientOptions::new().open(&name) {
            Ok(c) => break c,
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                if std::time::Instant::now() >= connect_deadline {
                    return LaunchDecision::Primary; // busy the whole window -- do not guess
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            Err(_) => return LaunchDecision::Primary, // nobody is listening
        }
    };
    // The pid is read off the still-connected handle, before anything can end
    // the server and let Windows hand the number to someone else.
    let server_pid = named_pipe_server_pid(&client);
    let req = PipeRequest { argv: vec![ping.to_string()], cwd: String::new() };
    let Ok(bytes) = serde_json::to_vec(&req) else { return LaunchDecision::Primary };
    // Its own full budget: time already spent waiting for a busy pipe must not
    // shrink the window the running instance gets to prove it is alive.
    let roundtrip = async {
        write_frame(&mut client, &bytes).await.ok()?;
        read_frame(&mut client, MAX_FRAME_BYTES).await.ok()
    };
    let answered = tokio::time::timeout(SECOND_LAUNCH_TIMEOUT, roundtrip).await.ok().flatten().is_some();
    if answered {
        return LaunchDecision::HandedOff;
    }
    // Connected, but the main thread never even reached the answer -- a
    // genuinely wedged instance (the answer goes out before show_main does any
    // work, so a slow or rebuilding window never looks like a hang here).
    let Some(pid) = server_pid else { return LaunchDecision::Primary };
    let (same_user, same_exe) = process_identity_check(pid);
    if should_take_over_hung_instance(false, same_user, same_exe) {
        // Terminate and WAIT: TerminateProcess only asks. Racing ahead while
        // the old process still holds the single-instance mutex, the data-dir
        // lock and the pipe would end this launch without a window after all.
        let ended = terminate_process(pid);
        append_crash_log(&format!(
            "second launch: took over a hung running instance (pid {pid}, ended={ended})"
        ));
        if ended {
            LaunchDecision::TookOver
        } else {
            LaunchDecision::Primary // could not confirm it is gone -- let the plugin decide
        }
    } else {
        LaunchDecision::Primary
    }
}

// Pure decision, independent of the pipe/timeout machinery above: whether a
// second launch that failed to reach the running instance in time may end it
// and continue as the primary instance itself. Never crosses a user or
// executable boundary -- a hung process belonging to someone else, or to a
// different build entirely, is left alone.
fn should_take_over_hung_instance(answered_in_time: bool, same_user: bool, same_exe_path: bool) -> bool {
    !answered_in_time && same_user && same_exe_path
}

// GetNamedPipeServerProcessId (Win32_System_Pipes) is not part of this
// project's enabled `windows` crate feature set, so this one call stays a
// direct kernel32 binding, the same pattern other modules already use for a few
// user32/kernel32 calls they need without pulling in a whole extra feature.
fn named_pipe_server_pid(client: &tokio::net::windows::named_pipe::NamedPipeClient) -> Option<u32> {
    use std::os::windows::io::AsRawHandle;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetNamedPipeServerProcessId(pipe: isize, id: *mut u32) -> i32;
    }
    let mut pid = 0u32;
    let ok = unsafe { GetNamedPipeServerProcessId(client.as_raw_handle() as isize, &mut pid) != 0 };
    (ok && pid != 0).then_some(pid)
}

// (same_user, same_exe_path) for the process behind `pid`. Either half
// defaults to false on any failure to query it -- an unknown answer is never
// treated as a match.
fn process_identity_check(pid: u32) -> (bool, bool) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::{EqualSid, GetTokenInformation, TokenUser, PSID, TOKEN_QUERY};
    use windows::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // The Sid field of a queried TOKEN_USER points into its own info buffer,
    // so the buffer has to outlive every use of the pointer -- returned
    // together rather than as a bare PSID a caller could dangle.
    unsafe fn token_user_sid(token: windows::Win32::Foundation::HANDLE) -> Option<(Vec<u8>, PSID)> {
        let mut needed = 0u32;
        let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut needed) };
        if needed == 0 {
            return None;
        }
        let mut buf = vec![0u8; needed as usize];
        unsafe { GetTokenInformation(token, TokenUser, Some(buf.as_mut_ptr().cast()), needed, &mut needed).ok()? };
        let sid = unsafe { (*buf.as_ptr().cast::<windows::Win32::Security::TOKEN_USER>()).User.Sid };
        Some((buf, sid))
    }

    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return (false, false);
        };

        let mut name_buf = [0u16; 1024];
        let mut name_len = name_buf.len() as u32;
        let same_exe = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, windows::core::PWSTR(name_buf.as_mut_ptr()), &mut name_len).is_ok()
            && std::env::current_exe().is_ok_and(|mine| {
                String::from_utf16_lossy(&name_buf[..name_len as usize]).eq_ignore_ascii_case(&mine.to_string_lossy())
            });

        let same_user = (|| -> Option<bool> {
            let mut their_token = windows::Win32::Foundation::HANDLE::default();
            OpenProcessToken(process, TOKEN_QUERY, &mut their_token).ok()?;
            let their_sid = token_user_sid(their_token);
            let _ = CloseHandle(their_token);
            let (_their_buf, their_sid) = their_sid?;

            let mut my_token = windows::Win32::Foundation::HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut my_token).ok()?;
            let my_sid = token_user_sid(my_token);
            let _ = CloseHandle(my_token);
            let (_my_buf, my_sid) = my_sid?;

            Some(EqualSid(their_sid, my_sid).is_ok())
        })()
        .unwrap_or(false);

        let _ = CloseHandle(process);
        (same_user, same_exe)
    }
}

// Ends `pid` and waits for it to be really gone. `true` only when the process
// has exited: the caller may not touch the mutex, the data-dir lock or the
// pipe the old instance still owns until then.
fn terminate_process(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    };
    const EXIT_WAIT_MS: u32 = 3000;
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid) else {
            return false;
        };
        let asked = TerminateProcess(process, 1).is_ok();
        let gone = asked && WaitForSingleObject(process, EXIT_WAIT_MS) == WAIT_OBJECT_0;
        let _ = CloseHandle(process);
        gone
    }
}

// ---------- Command grammar ----------

enum Parsed {
    Help(String),
    Run(Cmd),
}

enum Cmd {
    Version,
    Describe,
    List { tag: Option<String>, favorite: bool, trash: bool, json: bool },
    Get { id: String, json: bool },
    Copy { id: String },
    Add { name: String, text: String, tags: Vec<String>, favorite: bool, notes: Option<String>, color: Option<String>, confirmed: bool, json: bool },
    Edit { id: String, name: Option<String>, text: Option<String>, tags: Option<Vec<String>>, favorite: Option<bool>, notes: Option<String>, color: Option<String>, confirmed: bool, json: bool },
    Delete { id: String, confirmed: bool },
    Restore { id: String, confirmed: bool, json: bool },
    TagList { json: bool },
    TagAdd { id: String, tag: String, confirmed: bool },
    TagRemove { id: String, tag: String, confirmed: bool },
    ViewList { json: bool },
    ViewAdd { name: String, color: Option<String>, confirmed: bool, json: bool },
    ViewRemove { id: String, confirmed: bool },
    Search { query: String, tag: Option<String>, json: bool },
    Export { path: String, format: String, ids: Option<Vec<String>>, confirmed: bool },
    Import { path: String, confirmed: bool },
    BackupCreate { keep: Option<usize>, json: bool },
    BackupList { json: bool },
    BackupRestore { name: String, keep: Option<usize>, scope: String, confirmed: bool },
    SettingsGet { key: String, json: bool },
    SettingsSet { key: String, value: String, confirmed: bool },
}

fn usage_err(msg: impl Into<String>) -> String {
    let msg = msg.into();
    format!("{msg}\nRun \"prompt-saver --help\" for the full command list.")
}

// A prompt/view identifier: permissive enough to match every id this or an
// older build ever minted (see gen_id), narrow enough to reject anything that
// could not possibly be one -- a stray path, a flag, control characters.
// pub(crate): also the id check for a promptsaver:// link and the remote
// receiver, so all three entry points reject the same malformed ids the
// same way.
pub(crate) fn valid_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

const MAX_NAME_LEN: usize = 500;
const MAX_TEXT_LEN: usize = 500_000;
const MAX_PATH_ARG_LEN: usize = 32_000;

fn valid_name(s: &str) -> Result<String, String> {
    if s.is_empty() {
        return Err("name is empty".to_string());
    }
    if s.chars().count() > MAX_NAME_LEN {
        return Err(format!("name longer than {MAX_NAME_LEN} characters"));
    }
    Ok(s.to_string())
}

fn valid_text(s: &str) -> Result<String, String> {
    if s.chars().count() > MAX_TEXT_LEN {
        return Err(format!("text longer than {MAX_TEXT_LEN} characters"));
    }
    Ok(s.to_string())
}

fn valid_path_arg(s: &str) -> Result<String, String> {
    if s.is_empty() || s.contains('\0') || s.len() > MAX_PATH_ARG_LEN {
        return Err("path is empty, contains a NUL byte, or is too long".to_string());
    }
    Ok(s.to_string())
}

// A relative path argument resolves against the CALLER's working directory,
// never this process' own -- when a second launch was forwarded to the
// already-running instance, those two directories are almost never the same.
fn resolve_path_arg(cwd: &Path, raw: &str) -> PathBuf {
    let p = Path::new(raw);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}

// A tiny cursor over the remaining args, shared by every subcommand's parser.
// It owns its items: taking a flag out replaces the list instead of leaking a
// copy, which mattered once the app parsed a command for every pipe and receiver
// request and never exited between them.
struct Args {
    items: Vec<String>,
    pos: usize,
}

impl Args {
    fn new(items: &[String]) -> Self {
        Args { items: items.to_vec(), pos: 0 }
    }
    fn peek(&self) -> Option<&str> {
        self.items.get(self.pos).map(String::as_str)
    }
    fn next(&mut self) -> Option<String> {
        let v = self.items.get(self.pos).cloned();
        if v.is_some() {
            self.pos += 1;
        }
        v
    }
    fn next_positional(&mut self) -> Option<String> {
        let v = self.next()?;
        if v.starts_with("--") {
            self.pos -= 1;
            return None;
        }
        Some(v)
    }
}

fn parse(argv: &[String]) -> Result<Parsed, String> {
    let mut a = Args::new(argv);
    let head = a.next().ok_or_else(|| usage_err("missing command"))?;
    let head = head.as_str();
    if head == "--help" || head == "-h" {
        return Ok(Parsed::Help(top_level_help()));
    }
    if head == "--version" {
        return Ok(Parsed::Run(Cmd::Version));
    }
    if wants_help(&a) {
        return Ok(Parsed::Help(command_help(head, None)));
    }
    let cmd = match head {
        "version" => Cmd::Version,
        // Machine-facing only: its own JSON self-description, not listed in
        // top_level_help(). docs/automation.md's command table is generated
        // from this, never hand-typed -- see command_manifest() below.
        "describe" => Cmd::Describe,
        "list" => parse_list(&mut a)?,
        "get" => parse_get(&mut a)?,
        "copy" => parse_copy(&mut a)?,
        "add" => parse_add(&mut a)?,
        "edit" => parse_edit(&mut a)?,
        "delete" => parse_delete(&mut a)?,
        "restore" => parse_restore(&mut a)?,
        "tag" => return parse_tag(&mut a),
        "view" => return parse_view(&mut a),
        "search" => parse_search(&mut a)?,
        "export" => parse_export(&mut a)?,
        "import" => parse_import(&mut a)?,
        "backup" => return parse_backup(&mut a),
        "settings" => return parse_settings(&mut a),
        other => return Err(usage_err(format!("unknown command \"{other}\""))),
    };
    Ok(Parsed::Run(cmd))
}

// Whether the remaining args are exactly a bare `--help`/`-h` for this
// (sub)command -- checked before the real parse so `--help` always works
// even when other required arguments are missing.
fn wants_help(a: &Args) -> bool {
    a.items.get(a.pos..).is_some_and(|rest| rest == ["--help"] || rest == ["-h"])
}

fn take_flag_value(a: &mut Args, name: &str) -> Result<Option<String>, String> {
    let mut i = a.pos;
    while i < a.items.len() {
        if a.items[i] == name {
            let val = a.items.get(i + 1).ok_or_else(|| usage_err(format!("{name} needs a value")))?.clone();
            // Remove both the flag and its value so positional parsing after
            // this call never sees them.
            let mut kept: Vec<String> = a.items[a.pos..i].to_vec();
            kept.extend_from_slice(&a.items[i + 2..]);
            a.items = kept;
            a.pos = 0;
            return Ok(Some(val));
        }
        i += 1;
    }
    Ok(None)
}

fn take_flag(a: &mut Args, name: &str) -> bool {
    let mut i = a.pos;
    while i < a.items.len() {
        if a.items[i] == name {
            let mut kept: Vec<String> = a.items[a.pos..i].to_vec();
            kept.extend_from_slice(&a.items[i + 1..]);
            a.items = kept;
            a.pos = 0;
            return true;
        }
        i += 1;
    }
    false
}

fn split_tags(raw: &str) -> Vec<String> {
    raw.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

fn parse_list(a: &mut Args) -> Result<Cmd, String> {
    let tag = take_flag_value(a, "--tag")?;
    let favorite = take_flag(a, "--favorite");
    let trash = take_flag(a, "--trash");
    let json = take_flag(a, "--json");
    let _ = take_flag(a, "--quiet");
    reject_extra(a)?;
    Ok(Cmd::List { tag, favorite, trash, json })
}

fn parse_get(a: &mut Args) -> Result<Cmd, String> {
    let json = take_flag(a, "--json");
    let id = required_id(a)?;
    reject_extra(a)?;
    Ok(Cmd::Get { id, json })
}

fn parse_copy(a: &mut Args) -> Result<Cmd, String> {
    let _ = take_flag(a, "--quiet");
    let id = required_id(a)?;
    reject_extra(a)?;
    Ok(Cmd::Copy { id })
}

fn required_id(a: &mut Args) -> Result<String, String> {
    let id = a.next_positional().ok_or_else(|| usage_err("missing id"))?.to_string();
    if !valid_id(&id) {
        return Err(usage_err("id has an invalid shape"));
    }
    Ok(id)
}

fn parse_add(a: &mut Args) -> Result<Cmd, String> {
    let name = take_flag_value(a, "--name")?.ok_or_else(|| usage_err("--name is required"))?;
    let text = take_flag_value(a, "--text")?.ok_or_else(|| usage_err("--text is required"))?;
    let tags = take_flag_value(a, "--tags")?.map(|s| split_tags(&s)).unwrap_or_default();
    let notes = take_flag_value(a, "--notes")?;
    let color = take_flag_value(a, "--color")?;
    let favorite = take_flag(a, "--favorite");
    let confirmed = take_flag(a, "--yes");
    let json = take_flag(a, "--json");
    reject_extra(a)?;
    let name = valid_name(&name).map_err(usage_err)?;
    let text = valid_text(&text).map_err(usage_err)?;
    Ok(Cmd::Add { name, text, tags, favorite, notes, color, confirmed, json })
}

fn parse_edit(a: &mut Args) -> Result<Cmd, String> {
    let name = take_flag_value(a, "--name")?;
    let text = take_flag_value(a, "--text")?;
    let tags = take_flag_value(a, "--tags")?.map(|s| split_tags(&s));
    let notes = take_flag_value(a, "--notes")?;
    let color = take_flag_value(a, "--color")?;
    let fav_on = take_flag(a, "--favorite");
    let fav_off = take_flag(a, "--no-favorite");
    let confirmed = take_flag(a, "--yes");
    let json = take_flag(a, "--json");
    let id = required_id(a)?;
    reject_extra(a)?;
    let favorite = match (fav_on, fav_off) {
        (true, true) => return Err(usage_err("--favorite and --no-favorite are mutually exclusive")),
        (true, false) => Some(true),
        (false, true) => Some(false),
        (false, false) => None,
    };
    let name = name.map(|n| valid_name(&n)).transpose().map_err(usage_err)?;
    let text = text.map(|t| valid_text(&t)).transpose().map_err(usage_err)?;
    if name.is_none() && text.is_none() && tags.is_none() && favorite.is_none() && notes.is_none() && color.is_none() {
        return Err(usage_err("nothing to change: give at least one of --name/--text/--tags/--favorite/--no-favorite/--notes/--color"));
    }
    Ok(Cmd::Edit { id, name, text, tags, favorite, notes, color, confirmed, json })
}

fn parse_delete(a: &mut Args) -> Result<Cmd, String> {
    let confirmed = take_flag(a, "--yes");
    let id = required_id(a)?;
    reject_extra(a)?;
    Ok(Cmd::Delete { id, confirmed })
}

fn parse_restore(a: &mut Args) -> Result<Cmd, String> {
    let confirmed = take_flag(a, "--yes");
    let json = take_flag(a, "--json");
    let id = required_id(a)?;
    reject_extra(a)?;
    Ok(Cmd::Restore { id, confirmed, json })
}

fn parse_tag(a: &mut Args) -> Result<Parsed, String> {
    if wants_help(a) {
        return Ok(Parsed::Help(command_help("tag", None)));
    }
    let sub = a.next().ok_or_else(|| usage_err("tag needs list|add|remove"))?;
    let sub = sub.as_str();
    if wants_help(a) {
        return Ok(Parsed::Help(command_help("tag", Some(sub))));
    }
    let cmd = match sub {
        "list" => {
            let json = take_flag(a, "--json");
            reject_extra(a)?;
            Cmd::TagList { json }
        }
        "add" => {
            let confirmed = take_flag(a, "--yes");
            let id = required_id(a)?;
            let tag = a.next_positional().ok_or_else(|| usage_err("missing tag name"))?.to_string();
            reject_extra(a)?;
            let tag = valid_tag_name(&tag).map_err(usage_err)?;
            Cmd::TagAdd { id, tag, confirmed }
        }
        "remove" => {
            let confirmed = take_flag(a, "--yes");
            let id = required_id(a)?;
            let tag = a.next_positional().ok_or_else(|| usage_err("missing tag name"))?.to_string();
            reject_extra(a)?;
            let tag = valid_tag_name(&tag).map_err(usage_err)?;
            Cmd::TagRemove { id, tag, confirmed }
        }
        other => return Err(usage_err(format!("unknown tag subcommand \"{other}\""))),
    };
    Ok(Parsed::Run(cmd))
}

fn parse_view(a: &mut Args) -> Result<Parsed, String> {
    if wants_help(a) {
        return Ok(Parsed::Help(command_help("view", None)));
    }
    let sub = a.next().ok_or_else(|| usage_err("view needs list|add|remove"))?;
    let sub = sub.as_str();
    if wants_help(a) {
        return Ok(Parsed::Help(command_help("view", Some(sub))));
    }
    let cmd = match sub {
        "list" => {
            let json = take_flag(a, "--json");
            reject_extra(a)?;
            Cmd::ViewList { json }
        }
        "add" => {
            let color = take_flag_value(a, "--color")?;
            let confirmed = take_flag(a, "--yes");
            let json = take_flag(a, "--json");
            let name = a.next_positional().ok_or_else(|| usage_err("missing view name"))?.to_string();
            reject_extra(a)?;
            let name = valid_name(&name).map_err(usage_err)?;
            let color = color.map(|c| valid_tag_color(&c)).transpose().map_err(usage_err)?;
            Cmd::ViewAdd { name, color, confirmed, json }
        }
        "remove" => {
            let confirmed = take_flag(a, "--yes");
            let id = required_id(a)?;
            reject_extra(a)?;
            Cmd::ViewRemove { id, confirmed }
        }
        other => return Err(usage_err(format!("unknown view subcommand \"{other}\""))),
    };
    Ok(Parsed::Run(cmd))
}

fn parse_search(a: &mut Args) -> Result<Cmd, String> {
    let tag = take_flag_value(a, "--tag")?;
    let json = take_flag(a, "--json");
    let query = a.next_positional().ok_or_else(|| usage_err("missing search query"))?.to_string();
    reject_extra(a)?;
    Ok(Cmd::Search { query, tag, json })
}

fn parse_export(a: &mut Args) -> Result<Cmd, String> {
    let format = take_flag_value(a, "--format")?.unwrap_or_else(|| "json".to_string());
    let ids = take_flag_value(a, "--ids")?.map(|s| split_tags(&s));
    let confirmed = take_flag(a, "--yes");
    let path = a.next_positional().ok_or_else(|| usage_err("missing output path"))?.to_string();
    reject_extra(a)?;
    let path = valid_path_arg(&path).map_err(usage_err)?;
    if !matches!(format.as_str(), "json" | "csv" | "txt") {
        return Err(usage_err("--format must be json, csv or txt"));
    }
    Ok(Cmd::Export { path, format, ids, confirmed })
}

fn parse_import(a: &mut Args) -> Result<Cmd, String> {
    let confirmed = take_flag(a, "--yes");
    let path = a.next_positional().ok_or_else(|| usage_err("missing input path"))?.to_string();
    reject_extra(a)?;
    let path = valid_path_arg(&path).map_err(usage_err)?;
    Ok(Cmd::Import { path, confirmed })
}

fn parse_backup(a: &mut Args) -> Result<Parsed, String> {
    if wants_help(a) {
        return Ok(Parsed::Help(command_help("backup", None)));
    }
    let sub = a.next().ok_or_else(|| usage_err("backup needs create|list|restore"))?;
    let sub = sub.as_str();
    if wants_help(a) {
        return Ok(Parsed::Help(command_help("backup", Some(sub))));
    }
    let cmd = match sub {
        "create" => {
            let keep = parse_keep(a)?;
            let json = take_flag(a, "--json");
            reject_extra(a)?;
            Cmd::BackupCreate { keep, json }
        }
        "list" => {
            let json = take_flag(a, "--json");
            reject_extra(a)?;
            Cmd::BackupList { json }
        }
        "restore" => {
            let keep = parse_keep(a)?;
            let scope = take_flag_value(a, "--scope")?.unwrap_or_else(|| "all".to_string());
            let confirmed = take_flag(a, "--yes");
            let name = a.next_positional().ok_or_else(|| usage_err("missing backup name"))?.to_string();
            reject_extra(a)?;
            if !valid_backup_name(&name) {
                return Err(usage_err("backup name has an invalid shape"));
            }
            if valid_scope(&scope).is_err() {
                return Err(usage_err("--scope must be all, prompts or settings"));
            }
            Cmd::BackupRestore { name, keep, scope, confirmed }
        }
        other => return Err(usage_err(format!("unknown backup subcommand \"{other}\""))),
    };
    Ok(Parsed::Run(cmd))
}

fn parse_keep(a: &mut Args) -> Result<Option<usize>, String> {
    match take_flag_value(a, "--keep")? {
        Some(v) => v.parse::<usize>().map(Some).map_err(|_| usage_err("--keep must be a whole number")),
        None => Ok(None),
    }
}

fn parse_settings(a: &mut Args) -> Result<Parsed, String> {
    if wants_help(a) {
        return Ok(Parsed::Help(command_help("settings", None)));
    }
    let sub = a.next().ok_or_else(|| usage_err("settings needs get|set"))?;
    let sub = sub.as_str();
    if wants_help(a) {
        return Ok(Parsed::Help(command_help("settings", Some(sub))));
    }
    let cmd = match sub {
        "get" => {
            let json = take_flag(a, "--json");
            let key = a.next_positional().ok_or_else(|| usage_err("missing setting key"))?.to_string();
            reject_extra(a)?;
            Cmd::SettingsGet { key, json }
        }
        "set" => {
            let confirmed = take_flag(a, "--yes");
            let key = a.next_positional().ok_or_else(|| usage_err("missing setting key"))?.to_string();
            let value = a.next_positional().ok_or_else(|| usage_err("missing setting value"))?.to_string();
            reject_extra(a)?;
            Cmd::SettingsSet { key, value, confirmed }
        }
        other => return Err(usage_err(format!("unknown settings subcommand \"{other}\""))),
    };
    Ok(Parsed::Run(cmd))
}

fn reject_extra(a: &Args) -> Result<(), String> {
    if let Some(extra) = a.peek() {
        return Err(usage_err(format!("unexpected argument \"{extra}\"")));
    }
    Ok(())
}

// ---------- Outcome ----------

struct Outcome {
    code: i32,
    stdout: String,
    stderr: String,
    mutated_prompts: bool,
    mutated_trash: bool,
    mutated_settings: bool,
}

impl Outcome {
    fn ok(stdout: impl Into<String>) -> Self {
        Outcome { code: 0, stdout: stdout.into(), stderr: String::new(), mutated_prompts: false, mutated_trash: false, mutated_settings: false }
    }
    fn err(code: i32, stderr: impl Into<String>) -> Self {
        Outcome { code, stdout: String::new(), stderr: stderr.into(), mutated_prompts: false, mutated_trash: false, mutated_settings: false }
    }
    fn mutating(mut self) -> Self {
        self.mutated_prompts = true;
        self
    }
    fn mutating_trash(mut self) -> Self {
        self.mutated_trash = true;
        self
    }
    fn mutating_settings(mut self) -> Self {
        self.mutated_settings = true;
        self
    }
}

fn is_mutating(cmd: &Cmd) -> bool {
    matches!(
        cmd,
        Cmd::Add { .. }
            | Cmd::Edit { .. }
            | Cmd::Delete { .. }
            | Cmd::Restore { .. }
            | Cmd::TagAdd { .. }
            | Cmd::TagRemove { .. }
            | Cmd::ViewAdd { .. }
            | Cmd::ViewRemove { .. }
            | Cmd::Import { .. }
            | Cmd::BackupRestore { .. }
            | Cmd::SettingsSet { .. }
    )
}

fn confirmed_of(cmd: &Cmd) -> bool {
    match cmd {
        Cmd::Add { confirmed, .. }
        | Cmd::Edit { confirmed, .. }
        | Cmd::Delete { confirmed, .. }
        | Cmd::Restore { confirmed, .. }
        | Cmd::TagAdd { confirmed, .. }
        | Cmd::TagRemove { confirmed, .. }
        | Cmd::ViewAdd { confirmed, .. }
        | Cmd::ViewRemove { confirmed, .. }
        | Cmd::Import { confirmed, .. }
        | Cmd::BackupRestore { confirmed, .. }
        | Cmd::SettingsSet { confirmed, .. } => *confirmed,
        _ => true,
    }
}

// What a rejected mutating command would have done -- shown instead of doing
// it, so a script that forgot --yes still learns what it almost triggered.
fn dry_run_description(cmd: &Cmd) -> String {
    match cmd {
        Cmd::Add { name, .. } => format!("would add a prompt named \"{name}\""),
        Cmd::Edit { id, .. } => format!("would edit prompt {id}"),
        Cmd::Delete { id, .. } => format!("would move prompt {id} to the trash"),
        Cmd::Restore { id, .. } => format!("would restore prompt {id} from the trash"),
        Cmd::TagAdd { id, tag, .. } => format!("would add tag \"{tag}\" to prompt {id}"),
        Cmd::TagRemove { id, tag, .. } => format!("would remove tag \"{tag}\" from prompt {id}"),
        Cmd::ViewAdd { name, .. } => format!("would add a view named \"{name}\""),
        Cmd::ViewRemove { id, .. } => format!("would remove view {id}"),
        Cmd::Import { path, .. } => format!("would import prompts from \"{path}\""),
        Cmd::BackupRestore { name, .. } => format!("would replace the store with backup \"{name}\""),
        Cmd::SettingsSet { key, value, .. } => format!("would set setting \"{key}\" to \"{value}\""),
        _ => "would make a change".to_string(),
    }
}

// ---------- Pure core: runs one command against a Store already in memory ----------

fn exec(store: &mut Store, dir: &Path, cmd: &Cmd, cwd: &Path) -> Outcome {
    if is_mutating(cmd) && !confirmed_of(cmd) {
        return Outcome::err(4, format!("refused: {}. Pass --yes to actually do it.\n", dry_run_description(cmd)));
    }
    match cmd {
        Cmd::Version => Outcome::ok(String::new()), // handled before exec() is reached
        Cmd::Describe => Outcome::ok(String::new()), // handled before exec() is reached
        Cmd::List { tag, favorite, trash, json } => cmd_list(store, tag.as_deref(), *favorite, *trash, *json),
        Cmd::Get { id, json } => cmd_get(store, id, *json),
        Cmd::Copy { id } => cmd_copy(store, id),
        Cmd::Add { name, text, tags, favorite, notes, color, json, .. } => {
            cmd_add(store, name, text, tags, *favorite, notes.as_deref(), color.as_deref(), *json)
        }
        Cmd::Edit { id, name, text, tags, favorite, notes, color, json, .. } => {
            cmd_edit(store, id, name.as_deref(), text.as_deref(), tags.clone(), *favorite, notes.as_deref(), color.as_deref(), *json)
        }
        Cmd::Delete { id, .. } => cmd_delete(store, id),
        Cmd::Restore { id, json, .. } => cmd_restore(store, id, *json),
        Cmd::TagList { json } => cmd_tag_list(store, *json),
        Cmd::TagAdd { id, tag, .. } => cmd_tag_add(store, id, tag),
        Cmd::TagRemove { id, tag, .. } => cmd_tag_remove(store, id, tag),
        Cmd::ViewList { json } => cmd_view_list(store, *json),
        Cmd::ViewAdd { name, color, json, .. } => cmd_view_add(store, name, color.as_deref(), *json),
        Cmd::ViewRemove { id, .. } => cmd_view_remove(store, id),
        Cmd::Search { query, tag, json } => cmd_search(store, query, tag.as_deref(), *json),
        Cmd::Export { path, format, ids, confirmed } => cmd_export(store, cwd, path, format, ids.as_deref(), *confirmed),
        Cmd::Import { path, .. } => cmd_import(store, cwd, path),
        Cmd::BackupCreate { keep, json } => cmd_backup_create(store, dir, *keep, *json),
        Cmd::BackupList { json } => cmd_backup_list(dir, *json),
        Cmd::BackupRestore { .. } => Outcome::err(1, "backup restore is handled by its own caller\n"), // never reached: special-cased before exec()
        Cmd::SettingsGet { key, json } => cmd_settings_get(store, key, *json),
        Cmd::SettingsSet { key, value, .. } => cmd_settings_set(store, key, value),
    }
}

// ---------- List / get / copy / search ----------

#[derive(serde::Serialize)]
struct PromptSummary<'a> {
    id: &'a str,
    name: &'a str,
    favorite: bool,
    tags: &'a [String],
}

fn summary_line(p: &Prompt) -> String {
    format!("{}\t{}\t{}\t{}", p.id, p.name, if p.favorite { "*" } else { "" }, p.tags.join(","))
}

fn cmd_list(store: &Store, tag: Option<&str>, favorite: bool, trash: bool, json: bool) -> Outcome {
    let key = tag.map(tag_key);
    let source: &[Prompt] = if trash { &store.trash } else { &store.prompts };
    let filtered: Vec<&Prompt> = source
        .iter()
        .filter(|p| !favorite || p.favorite)
        .filter(|p| key.as_deref().map_or(true, |k| p.tags.iter().any(|t| tag_key(t) == k)))
        .collect();
    if json {
        let items: Vec<PromptSummary> = filtered.iter().map(|p| PromptSummary { id: &p.id, name: &p.name, favorite: p.favorite, tags: &p.tags }).collect();
        Outcome::ok(format!("{}\n", serde_json::to_string(&items).unwrap_or_default()))
    } else {
        let lines: String = filtered.iter().map(|p| summary_line(p)).collect::<Vec<_>>().join("\n");
        Outcome::ok(if lines.is_empty() { String::new() } else { format!("{lines}\n") })
    }
}

fn find_prompt<'a>(store: &'a Store, id: &str) -> Option<&'a Prompt> {
    store.prompts.iter().find(|p| p.id == id).or_else(|| store.trash.iter().find(|p| p.id == id))
}

#[derive(serde::Serialize)]
struct PromptDetail<'a> {
    id: &'a str,
    name: &'a str,
    text: &'a str,
    favorite: bool,
    tags: &'a [String],
    notes: &'a str,
    color: &'a str,
    trashed: bool,
}

fn cmd_get(store: &Store, id: &str, json: bool) -> Outcome {
    let Some(p) = find_prompt(store, id) else {
        return Outcome::err(2, format!("no prompt with id {id}\n"));
    };
    if p.secret {
        return Outcome::err(1, format!("prompt {id} is confidential and can only be read in the app itself\n"));
    }
    let trashed = p.trashed_at > 0;
    if json {
        let d = PromptDetail { id: &p.id, name: &p.name, text: &p.text, favorite: p.favorite, tags: &p.tags, notes: &p.notes, color: &p.color, trashed };
        Outcome::ok(format!("{}\n", serde_json::to_string(&d).unwrap_or_default()))
    } else {
        let mut out = String::new();
        out.push_str(&format!("id: {}\n", p.id));
        out.push_str(&format!("name: {}\n", p.name));
        out.push_str(&format!("favorite: {}\n", p.favorite));
        out.push_str(&format!("tags: {}\n", p.tags.join(", ")));
        out.push_str(&format!("color: {}\n", p.color));
        out.push_str(&format!("trashed: {trashed}\n"));
        out.push_str(&format!("notes: {}\n", p.notes));
        out.push_str(&format!("text:\n{}\n", p.text));
        Outcome::ok(out)
    }
}

fn cmd_copy(store: &Store, id: &str) -> Outcome {
    let Some(p) = store.prompts.iter().find(|p| p.id == id) else {
        return Outcome::err(2, format!("no prompt with id {id}\n"));
    };
    // Same rule as `get`: a confidential prompt's text is only ever shown in the
    // app, and what the store holds for it is ciphertext.
    if p.secret {
        return Outcome::err(1, format!("prompt {id} is confidential and can only be copied in the app itself\n"));
    }
    match arboard::Clipboard::new().and_then(|mut c| c.set_text(p.text.clone())) {
        Ok(()) => {
            // Our own copy: the clipboard watcher must not take it for a foreign one.
            crate::note_own_copy(&p.text);
            Outcome::ok(String::new())
        }
        Err(e) => Outcome::err(3, format!("clipboard unavailable: {e}\n")),
    }
}

fn cmd_search(store: &Store, query: &str, tag: Option<&str>, json: bool) -> Outcome {
    let q = query.to_lowercase();
    let key = tag.map(tag_key);
    let filtered: Vec<&Prompt> = store
        .prompts
        .iter()
        .filter(|p| key.as_deref().map_or(true, |k| p.tags.iter().any(|t| tag_key(t) == k)))
        .filter(|p| {
            q.is_empty()
                || p.name.to_lowercase().contains(&q)
                || p.text.to_lowercase().contains(&q)
                || p.notes.to_lowercase().contains(&q)
                || p.tags.iter().any(|t| t.to_lowercase().contains(&q))
        })
        .collect();
    if json {
        let items: Vec<PromptSummary> = filtered.iter().map(|p| PromptSummary { id: &p.id, name: &p.name, favorite: p.favorite, tags: &p.tags }).collect();
        Outcome::ok(format!("{}\n", serde_json::to_string(&items).unwrap_or_default()))
    } else {
        let lines: String = filtered.iter().map(|p| summary_line(p)).collect::<Vec<_>>().join("\n");
        Outcome::ok(if lines.is_empty() { String::new() } else { format!("{lines}\n") })
    }
}

// ---------- Add / edit / delete / restore ----------
//
// A prompt made or changed here gets every field the store itself needs
// (id, timestamps, defaults) but is never placed into a view's grid layout --
// exactly like a prompt whose view was deleted (see View::layouts' own
// comment: "a member with no cell ... is an overflow entry, not a lost one").
// It shows up in list/get/search immediately either way; arranging it on a
// grid is a job for the app itself.

#[allow(clippy::too_many_arguments)]
fn cmd_add(store: &mut Store, name: &str, text: &str, tags: &[String], favorite: bool, notes: Option<&str>, color: Option<&str>, json: bool) -> Outcome {
    let tags = match sanitize_tags(tags.to_vec(), &canonical_tags(&store.prompts)) {
        Ok(t) => t,
        Err(e) => return Outcome::err(4, format!("{e}\n")),
    };
    let notes = match sanitize_notes(notes.unwrap_or("")) {
        Ok(n) => n,
        Err(e) => return Outcome::err(4, format!("{e}\n")),
    };
    let color = match color.map(valid_tag_color).transpose() {
        Ok(c) => c.unwrap_or_default(),
        Err(e) => return Outcome::err(4, format!("{e}\n")),
    };
    let prompt = Prompt {
        id: gen_id(),
        name: name.to_string(),
        text: upgrade_placeholders(text),
        color,
        image: String::new(),
        show_image: false,
        copy_image: false,
        file_path: String::new(),
        pdf_page: 0,
        icon_path: String::new(),
        caption: String::new(),
        caption_size: 0,
        font: String::new(),
        font_size: 0,
        favorite,
        tags,
        notes,
        hotkey: String::new(),
        hotkey_paste: String::new(),
        trashed_at: 0,
        variants: Vec::new(),
        rotate_mode: String::new(),
        rotate_auto: false,
        media_duration_ms: 0,
        media_width: 0,
        media_height: 0,
        media_size_bytes: 0,
        image_text: String::new(),
        secret: false,
    };
    let id = prompt.id.clone();
    store.prompts.push(prompt);
    let body = if json { format!("{{\"id\":\"{id}\"}}\n") } else { format!("{id}\n") };
    Outcome::ok(body).mutating()
}

#[allow(clippy::too_many_arguments)]
fn cmd_edit(store: &mut Store, id: &str, name: Option<&str>, text: Option<&str>, tags: Option<Vec<String>>, favorite: Option<bool>, notes: Option<&str>, color: Option<&str>, json: bool) -> Outcome {
    let tags = match tags {
        Some(list) => match sanitize_tags(list, &canonical_tags(&store.prompts)) {
            Ok(t) => Some(t),
            Err(e) => return Outcome::err(4, format!("{e}\n")),
        },
        None => None,
    };
    let notes = match notes.map(sanitize_notes).transpose() {
        Ok(n) => n,
        Err(e) => return Outcome::err(4, format!("{e}\n")),
    };
    let color = match color.map(valid_tag_color).transpose() {
        Ok(c) => c,
        Err(e) => return Outcome::err(4, format!("{e}\n")),
    };
    let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) else {
        return Outcome::err(2, format!("no prompt with id {id}\n"));
    };
    // A confidential entry stores its text sealed with the vault key, which the
    // command line never holds: plain text written here would sit on disk as is
    // and fail to open in the app.
    if p.secret && text.is_some() {
        return Outcome::err(1, format!("prompt {id} is confidential and its text can only be changed in the app itself\n"));
    }
    if let Some(n) = name {
        p.name = n.to_string();
    }
    if let Some(t) = text {
        p.text = upgrade_placeholders(t);
    }
    if let Some(t) = tags {
        p.tags = t;
    }
    if let Some(f) = favorite {
        p.favorite = f;
    }
    if let Some(n) = notes {
        p.notes = n;
    }
    if let Some(c) = color {
        p.color = c;
    }
    let body = if json { "{\"ok\":true}\n".to_string() } else { String::new() };
    Outcome::ok(body).mutating()
}

fn cmd_delete(store: &mut Store, id: &str) -> Outcome {
    let Some(pos) = store.prompts.iter().position(|p| p.id == id) else {
        return Outcome::err(2, format!("no prompt with id {id}\n"));
    };
    let mut p = store.prompts.remove(pos);
    p.trashed_at = crate::now_secs();
    store.trash.insert(0, p);
    for view in &mut store.settings.views {
        for layout in view.layouts.values_mut() {
            layout.remove(id);
        }
        view.members.retain(|m| m != id);
    }
    store.settings.floating.remove(id);
    store.settings.float_scale.remove(id);
    store.settings.video_prefs.remove(id);
    store.normalize_folders();
    Outcome::ok(String::new()).mutating_trash().mutating_settings()
}

fn cmd_restore(store: &mut Store, id: &str, json: bool) -> Outcome {
    let Some(pos) = store.trash.iter().position(|p| p.id == id) else {
        return Outcome::err(2, format!("no prompt with id {id} in the trash\n"));
    };
    let mut p = store.trash.remove(pos);
    p.trashed_at = 0;
    upgrade_prompt_placeholders(&mut p);
    // Same seat the app's own restore gives: on the active view, or its overflow row.
    crate::seat_restored_in_active_view(&mut store.settings, &p.id);
    store.prompts.push(p);
    let body = if json { "{\"ok\":true}\n".to_string() } else { String::new() };
    Outcome::ok(body).mutating_trash().mutating_settings()
}

// ---------- Tags ----------

#[derive(serde::Serialize)]
struct TagCount {
    name: String,
    count: usize,
}

fn cmd_tag_list(store: &Store, json: bool) -> Outcome {
    let mut counts: std::collections::HashMap<String, (String, usize)> = std::collections::HashMap::new();
    for p in &store.prompts {
        for t in &p.tags {
            let entry = counts.entry(tag_key(t)).or_insert_with(|| (t.clone(), 0));
            entry.1 += 1;
        }
    }
    let mut rows: Vec<TagCount> = counts.into_values().map(|(name, count)| TagCount { name, count }).collect();
    rows.sort_by_key(|r| r.name.to_lowercase());
    if json {
        Outcome::ok(format!("{}\n", serde_json::to_string(&rows).unwrap_or_default()))
    } else {
        let lines: String = rows.iter().map(|r| format!("{}\t{}", r.name, r.count)).collect::<Vec<_>>().join("\n");
        Outcome::ok(if lines.is_empty() { String::new() } else { format!("{lines}\n") })
    }
}

fn cmd_tag_add(store: &mut Store, id: &str, tag: &str) -> Outcome {
    let known = canonical_tags(&store.prompts);
    let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) else {
        return Outcome::err(2, format!("no prompt with id {id}\n"));
    };
    let mut tags = p.tags.clone();
    tags.push(tag.to_string());
    match sanitize_tags(tags, &known) {
        Ok(t) => {
            p.tags = t;
            Outcome::ok(String::new()).mutating()
        }
        Err(e) => Outcome::err(4, format!("{e}\n")),
    }
}

fn cmd_tag_remove(store: &mut Store, id: &str, tag: &str) -> Outcome {
    let key = tag_key(tag);
    let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) else {
        return Outcome::err(2, format!("no prompt with id {id}\n"));
    };
    let before = p.tags.len();
    p.tags.retain(|t| tag_key(t) != key);
    if p.tags.len() == before {
        return Outcome::err(2, format!("prompt {id} does not carry tag \"{tag}\"\n"));
    }
    Outcome::ok(String::new()).mutating()
}

// ---------- Views ----------

#[derive(serde::Serialize)]
struct ViewSummary<'a> {
    id: &'a str,
    name: &'a str,
    members: usize,
}

fn cmd_view_list(store: &Store, json: bool) -> Outcome {
    if json {
        let items: Vec<ViewSummary> = store.settings.views.iter().map(|v| ViewSummary { id: &v.id, name: &v.name, members: v.members.len() }).collect();
        Outcome::ok(format!("{}\n", serde_json::to_string(&items).unwrap_or_default()))
    } else {
        let lines: String = store.settings.views.iter().map(|v| format!("{}\t{}\t{}", v.id, v.name, v.members.len())).collect::<Vec<_>>().join("\n");
        Outcome::ok(if lines.is_empty() { String::new() } else { format!("{lines}\n") })
    }
}

const MAX_VIEWS_CLI: usize = 200; // mirrors MAX_VIEWS -- kept independent so the CLI's own cap never silently drifts if the GUI's changes

fn cmd_view_add(store: &mut Store, name: &str, color: Option<&str>, json: bool) -> Outcome {
    if store.settings.views.len() >= MAX_VIEWS_CLI {
        return Outcome::err(4, format!("max {MAX_VIEWS_CLI} views\n"));
    }
    let view = crate::View {
        id: gen_id(),
        name: name.to_string(),
        cols: 5,
        rows: 4,
        layouts: std::collections::HashMap::new(),
        color: color.unwrap_or("").to_string(),
        members: Vec::new(),
    };
    let id = view.id.clone();
    store.settings.views.push(view);
    let body = if json { format!("{{\"id\":\"{id}\"}}\n") } else { format!("{id}\n") };
    Outcome::ok(body).mutating_settings()
}

fn cmd_view_remove(store: &mut Store, id: &str) -> Outcome {
    if !store.settings.views.iter().any(|v| v.id == id) {
        return Outcome::err(2, format!("no view with id {id}\n"));
    }
    if store.settings.views.len() <= 1 {
        return Outcome::err(4, "cannot remove the last view\n".to_string());
    }
    store.settings.views.retain(|v| v.id != id);
    if store.settings.active_view == id {
        if let Some(first) = store.settings.views.first() {
            store.settings.active_view = first.id.clone();
        }
    }
    Outcome::ok(String::new()).mutating_settings()
}

// ---------- Export / import ----------

fn cmd_export(store: &Store, cwd: &Path, path: &str, format: &str, ids: Option<&[String]>, confirmed: bool) -> Outcome {
    let selected: Vec<Prompt> = match ids {
        Some(list) if !list.is_empty() => {
            let set: std::collections::HashSet<&str> = list.iter().map(String::as_str).collect();
            store.prompts.iter().filter(|p| set.contains(p.id.as_str())).cloned().collect()
        }
        _ => store.prompts.clone(),
    };
    // A confidential entry holds sealed ciphertext in its text fields: it never
    // leaves in a file, the same default the app's own export applies.
    let selected = crate::filter_export_secrets(selected, false);
    let remembered = if store.settings.ui_flags.get("varDefaults") != Some(&false) { parse_ui_text_map(&store.settings, "varDefaults") } else { std::collections::HashMap::new() };
    let fixed = if store.settings.ui_flags.get("varFixed") == Some(&true) { parse_ui_text_map(&store.settings, "varFixed") } else { std::collections::HashMap::new() };
    let resolved: Vec<Prompt> = selected.iter().map(|p| resolved_for_export(p, &remembered, &fixed)).collect();
    let content = match format {
        "csv" => to_csv(&resolved),
        "txt" => to_txt(&resolved),
        _ => to_json(&resolved),
    };
    let out_path = resolve_path_arg(cwd, path);
    if out_path.exists() && !confirmed {
        // Overwriting an existing file is the one export side effect that can
        // destroy something -- everything else about export only adds a new
        // file, so this is the only spot that needs confirmation.
        return Outcome::err(4, format!("refused: \"{}\" already exists. Pass --yes to overwrite it.\n", out_path.display()));
    }
    match std::fs::write(&out_path, content) {
        Ok(()) => Outcome::ok(format!("exported {} prompt(s) to {}\n", resolved.len(), out_path.display())),
        Err(e) => Outcome::err(3, format!("could not write {}: {e}\n", out_path.display())),
    }
}

fn cmd_import(store: &mut Store, cwd: &Path, path: &str) -> Outcome {
    let in_path = resolve_path_arg(cwd, path);
    let bytes = match read_file_capped(&in_path, MAX_IMPORT_BYTES) {
        Ok(b) => b,
        Err(e) => return Outcome::err(3, format!("{e}\n")),
    };
    let data = match parse_import_bytes(&bytes, &in_path) {
        Ok(d) => d,
        Err(e) => return Outcome::err(4, format!("{e}\n")),
    };
    let count = append_imported_prompts(store, data.prompts);
    Outcome::ok(format!("imported {count} prompt(s)\n")).mutating()
}

// ---------- Backups ----------

fn default_keep(settings: &crate::Settings) -> usize {
    settings.ui_values.get("backupKeep").copied().unwrap_or(10.0).clamp(1.0, 50.0) as usize
}

#[derive(serde::Serialize)]
struct BackupSummary {
    name: String,
    size: u64,
    prompts: Option<usize>,
}

fn cmd_backup_create(store: &Store, dir: &Path, keep: Option<usize>, json: bool) -> Outcome {
    let keep = keep.unwrap_or_else(|| default_keep(&store.settings));
    let gfs = gfs_from_settings(&store.settings);
    match do_backup_at(dir, &dir.join("backups"), keep, gfs) {
        Ok(name) => {
            let body = if json { format!("{{\"name\":\"{name}\"}}\n") } else { format!("{name}\n") };
            Outcome::ok(body)
        }
        Err(e) => Outcome::err(3, format!("{e}\n")),
    }
}

fn cmd_backup_list(dir: &Path, json: bool) -> Outcome {
    let items = list_backups_impl_at(&dir.join("backups"));
    if json {
        let rows: Vec<BackupSummary> = items.iter().map(|b| BackupSummary { name: b.name.clone(), size: b.size, prompts: b.prompts }).collect();
        Outcome::ok(format!("{}\n", serde_json::to_string(&rows).unwrap_or_default()))
    } else {
        let lines: String = items
            .iter()
            .map(|b| format!("{}\t{}\t{}", b.name, b.size, b.prompts.map(|n| n.to_string()).unwrap_or_else(|| "?".to_string())))
            .collect::<Vec<_>>()
            .join("\n");
        Outcome::ok(if lines.is_empty() { String::new() } else { format!("{lines}\n") })
    }
}

// Shared by both callers of `backup restore` (special-cased outside exec()
// because the live path reuses the production restore_backup command
// end-to-end, including its app restart -- there is nothing for exec() itself
// to do there). This is the standalone half: same safety snapshot, staging
// and swap, minus anything that only makes sense with a running app.
fn restore_backup_standalone(dir: &Path, name: &str, keep: usize, scope: &str, settings: &crate::Settings) -> Result<(), String> {
    let backups_root = dir.join("backups");
    let backup_db = backup_db_file(&backups_root.join(name)).ok_or_else(|| "missing".to_string())?;
    let gfs = gfs_from_settings(settings);
    crate::do_backup_protecting_at(dir, &backups_root, keep, gfs, name)
        .map_err(|e| format!("safety-snapshot-failed: {e}"))?;
    let target = dir.join("data.db");
    let staged = stage_and_validate_backup(&backup_db, dir)?;
    if scope == "all" {
        if let Err(e) = crate::keep_machine_in_staged(&target, &staged) {
            let _ = std::fs::remove_file(&staged);
            return Err(e);
        }
        if let Err(e) = std::fs::rename(&staged, &target) {
            let _ = std::fs::remove_file(&staged);
            return Err(e.to_string());
        }
    } else {
        let result = restore_selective(&target, &staged, scope);
        let _ = std::fs::remove_file(&staged);
        result?;
    }
    let _ = std::fs::remove_file(dir.join("data.db-wal"));
    let _ = std::fs::remove_file(dir.join("data.db-shm"));
    // A restore that brings settings with it may have switched the picture-text
    // index off while the prompts that stay still carry its text.
    if scope != "data" {
        clear_ocr_text_after_restore(dir, settings.ui_flags.get("ocrIndex").copied().unwrap_or(false))?;
    }
    Ok(())
}

// Erases the picture text in the restored data when the index was on before and
// the restored settings have it off; writes only when something was cleared.
fn clear_ocr_text_after_restore(dir: &Path, was_on: bool) -> Result<(), String> {
    if !was_on {
        return Ok(());
    }
    let mut fresh = load_store_headless(dir);
    if fresh.prompts_locked || !crate::clear_ocr_text_if_switched_off(true, &mut fresh) {
        return Ok(());
    }
    let mut conn = db_conn_at(dir).ok_or_else(|| "could not open the restored store".to_string())?;
    let ok = db_write_prompts_and_trash(&mut conn, &fresh.prompts, &fresh.trash, &mut fresh.written.borrow_mut()).is_ok();
    if ok { Ok(()) } else { Err("could not write the restored store".to_string()) }
}

fn map_restore_err_code(e: &str) -> i32 {
    if e == "missing" {
        2
    } else {
        3
    }
}

// ---------- Settings ----------

fn cmd_settings_get(store: &Store, key: &str, json: bool) -> Outcome {
    // The receiver's certificate values belong to this machine and never
    // travel over the command line, least of all the remote one.
    // The vault salt and verifier and the sealed backup password would give
    // anyone with the receiver password what an offline guess needs.
    if crate::remote::is_remote_secret_key(key) || crate::is_vault_key(key) || key == crate::BACKUP_PW_KEY {
        return Outcome::err(2, format!("no setting named \"{key}\"\n"));
    }
    let s = &store.settings;
    let value: serde_json::Value = match key {
        "theme" => serde_json::Value::String(s.theme.clone()),
        "language" => serde_json::Value::String(s.language.clone()),
        "autostart" => serde_json::Value::Bool(s.autostart),
        "alwaysOnTop" => serde_json::Value::Bool(s.always_on_top),
        _ => {
            if let Some(v) = s.ui_flags.get(key) {
                serde_json::Value::Bool(*v)
            } else if let Some(v) = s.ui_values.get(key) {
                serde_json::json!(v)
            } else if let Some(v) = s.ui_texts.get(key) {
                serde_json::Value::String(v.clone())
            } else {
                return Outcome::err(2, format!("no setting named \"{key}\"\n"));
            }
        }
    };
    if json {
        Outcome::ok(format!("{value}\n"))
    } else {
        let text = match &value {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        Outcome::ok(format!("{text}\n"))
    }
}

fn cmd_settings_set(store: &mut Store, key: &str, value: &str) -> Outcome {
    if crate::remote::is_remote_secret_key(key) {
        return Outcome::err(1, format!("{key} is managed by the app and cannot be set\n"));
    }
    // Folder authority, the vault keys, the backup password, the import-denied
    // switches and every receiver setting have their own dedicated writers and
    // are refused for an import too; this generic setter is reachable from the
    // remote receiver, so it must not be a way around them.
    if crate::is_protected_folder_key(key)
        || crate::is_vault_key(key)
        || key == crate::BACKUP_PW_KEY
        || crate::is_import_denied_flag(key)
        || crate::is_import_denied_value(key)
    {
        return Outcome::err(1, format!("{key} can only be changed in the app itself\n"));
    }
    let s = &mut store.settings;
    match key {
        "theme" => s.theme = value.to_string(),
        "language" => s.language = value.to_string(),
        "autostart" => match value.parse::<bool>() {
            Ok(b) => s.autostart = b,
            Err(_) => return Outcome::err(1, "autostart takes true or false\n".to_string()),
        },
        "alwaysOnTop" => match value.parse::<bool>() {
            Ok(b) => s.always_on_top = b,
            Err(_) => return Outcome::err(1, "alwaysOnTop takes true or false\n".to_string()),
        },
        _ => {
            // A key that already holds a value keeps its kind: a text setting set to
            // "2024" stays text, and a switch never turns into a stray number.
            let held = if s.ui_flags.contains_key(key) {
                "flag"
            } else if s.ui_values.contains_key(key) {
                "number"
            } else if s.ui_texts.contains_key(key) {
                "text"
            } else {
                ""
            };
            let as_flag = if held == "flag" || held.is_empty() { value.parse::<bool>().ok() } else { None };
            let as_number = if held == "number" || held.is_empty() { value.parse::<f64>().ok() } else { None };
            if as_flag.is_none() && as_number.is_none() && !(held.is_empty() || held == "text") {
                let wants = if held == "flag" { "true or false" } else { "a number" };
                return Outcome::err(1, format!("{key} takes {wants}\n"));
            }
            if let Some(b) = as_flag {
                // Switching the picture-text index off here erases its text, the same
                // as the switch in the app does: that text was read out of pictures.
                if key == "ocrIndex" && !b && s.ui_flags.get(key).copied().unwrap_or(false) {
                    let cleared = crate::clear_ocr_index_text(store);
                    store.settings.ui_flags.insert(key.to_string(), b);
                    let out = Outcome::ok(String::new()).mutating_settings();
                    return if cleared { out.mutating().mutating_trash() } else { out };
                }
                s.ui_flags.insert(key.to_string(), b);
            } else if let Some(n) = as_number {
                // "NaN", "inf" and "1e999" parse as numbers but cannot be stored: they
                // are written as null and the whole settings block fails to load.
                let Some(n) = crate::clamp_ui_value(key, n) else {
                    return Outcome::err(1, format!("{key} needs a finite number
"));
                };
                s.ui_values.insert(key.to_string(), n);
            } else {
                s.ui_texts.insert(key.to_string(), value.to_string());
            }
        }
    }
    Outcome::ok(String::new()).mutating_settings()
}

// ---------- Standalone caller (nothing is running) ----------

fn load_store_headless(dir: &Path) -> Store {
    if let Some(conn) = db_conn_at(dir) {
        let mut written = WrittenRows::new();
        let (mut prompts, settings_opt, _needs_mig, prompt_fails, settings_unreadable) = db_load(&conn, &mut written);
        let (trash, trash_fails) = db_load_trash(&conn, &mut written);
        let decrypt_fails = prompt_fails + trash_fails;
        let mut settings = settings_opt.unwrap_or_default();
        settings.migrate();
        migrate_prompts(&mut prompts);
        Store { prompts, trash, settings, prompts_locked: decrypt_fails > 0 || settings_unreadable, written: std::cell::RefCell::new(written) }
    } else {
        Store { prompts: Vec::new(), trash: Vec::new(), settings: crate::Settings::default(), prompts_locked: true, written: std::cell::RefCell::new(WrittenRows::new()) }
    }
}

pub(crate) fn run_standalone(argv: &[String], cwd: &Path) -> i32 {
    run_standalone_with(data_dir_headless, argv, cwd)
}

// run_standalone with the folder supplied by the caller (asked only once the
// command needs a store), so a test can point it at a folder of its own.
pub(crate) fn run_standalone_with(resolve_dir: impl FnOnce() -> PathBuf, argv: &[String], cwd: &Path) -> i32 {
    let parsed = match parse(argv) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let cmd = match parsed {
        Parsed::Help(text) => {
            println!("{text}");
            return 0;
        }
        Parsed::Run(cmd) => cmd,
    };
    if matches!(cmd, Cmd::Version) {
        print_version();
        return 0;
    }
    if matches!(cmd, Cmd::Describe) {
        println!("{}", describe_json());
        return 0;
    }
    let dir = resolve_dir();
    let store = load_store_headless(&dir);
    acquire_data_dir_lock(&dir, data_dir_lock_enabled(&store.settings));

    // Before the restore arm: a restore replaces the whole database, so a store
    // another machine holds locked must be refused for it like for every other
    // change (it used to return before this check was ever reached).
    if is_mutating(&cmd) && crate::DATA_DIR_READONLY.load(Ordering::Relaxed) {
        eprintln!("the data folder is locked by another machine right now\n");
        return 3;
    }

    if let Cmd::BackupRestore { name, keep, scope, confirmed } = &cmd {
        if !confirmed {
            eprintln!("refused: {}. Pass --yes to actually do it.", dry_run_description(&cmd));
            return 4;
        }
        let keep = keep.unwrap_or_else(|| default_keep(&store.settings));
        return match restore_backup_standalone(&dir, name, keep, scope, &store.settings) {
            Ok(()) => {
                println!("restored backup {name}");
                0
            }
            Err(e) => {
                eprintln!("{e}");
                map_restore_err_code(&e)
            }
        };
    }

    if is_mutating(&cmd) && store.prompts_locked {
        eprintln!("the store has rows that failed to decrypt; writes are refused until that is resolved\n");
        return 3;
    }

    let mut store = store;
    let outcome = exec(&mut store, &dir, &cmd, cwd);
    if outcome.mutated_prompts || outcome.mutated_trash || outcome.mutated_settings {
        let Some(mut conn) = db_conn_at(&dir) else {
            eprintln!("could not open the store to save the change\n");
            return 3;
        };
        // Reuses the map load_store_headless() already populated from the table,
        // not a fresh empty one -- write_rows only deletes a row that is missing
        // from the new list AND already known to this map, exactly so a process
        // that never saw a row does not erase it. Starting empty here would make
        // every deletion a no-op on disk: the row leaves store.prompts in memory
        // but write_rows, finding no prior knowledge of it, never drops it from
        // the table.
        let write_ok = if outcome.mutated_trash {
            db_write_prompts_and_trash(&mut conn, &store.prompts, &store.trash, &mut store.written.borrow_mut()).is_ok()
        } else if outcome.mutated_prompts {
            db_write_prompts(&mut conn, &store.prompts, &mut store.written.borrow_mut()).is_ok()
        } else {
            true
        };
        let settings_ok = if outcome.mutated_settings { db_write_settings(&conn, &store.settings).is_ok() } else { true };
        if !write_ok || !settings_ok {
            eprintln!("the write did not reach the database\n");
            return 3;
        }
        // Nothing is running to do it, so the Run entry is brought in line here.
        if let Some(("autostart", Ok(on))) = os_backed_toggle(&cmd) {
            if outcome.code == 0 {
                if let Err(e) = crate::apply_autostart(on, store.settings.start_minimized) {
                    eprintln!("the setting was saved but the Windows entry could not be changed: {e}\n");
                    return 3;
                }
            }
        }
    }
    if !outcome.stdout.is_empty() {
        print!("{}", outcome.stdout);
    }
    if !outcome.stderr.is_empty() {
        eprint!("{}", outcome.stderr);
    }
    outcome.code
}

fn print_version() {
    println!("{} {}", crate::brand::DISPLAY_NAME, env!("CARGO_PKG_VERSION"));
}

// ---------- Already-running caller ----------

pub(crate) async fn run_in_process(app: &AppHandle, argv: Vec<String>, cwd: PathBuf) -> (i32, String, String) {
    let (code, stdout, stderr, restart) = run_in_process_deferred(app, argv, cwd).await;
    if restart {
        app.restart();
    }
    (code, stdout, stderr)
}

// Same as run_in_process, but a restore does not relaunch the app itself: the
// last element says a relaunch is due, and the caller does it once its answer is
// written. A relaunch inside the call drops the connection before any answer goes
// out, which a script reads as a failed restore.
pub(crate) async fn run_in_process_deferred(app: &AppHandle, argv: Vec<String>, cwd: PathBuf) -> (i32, String, String, bool) {
    let mut restart = false;
    let (code, stdout, stderr) = run_in_process_inner(app, argv, cwd, &mut restart).await;
    (code, stdout, stderr, restart)
}

// Runs `after` once `answer` has finished: the answer must be on the wire before
// the relaunch tears the connection down.
pub(crate) async fn answer_then_restart(answer: impl std::future::Future<Output = ()>, restart_due: bool, restart: impl FnOnce()) {
    answer.await;
    if restart_due {
        restart();
    }
}

// A confirmed `settings set autostart|alwaysOnTop` with its value read as a switch.
fn os_backed_toggle(cmd: &Cmd) -> Option<(&'static str, Result<bool, ()>)> {
    let Cmd::SettingsSet { key, value, confirmed: true } = cmd else { return None };
    let key = match key.as_str() {
        "autostart" => "autostart",
        "alwaysOnTop" => "alwaysOnTop",
        _ => return None,
    };
    Some((key, value.parse::<bool>().map_err(|_| ())))
}

async fn run_in_process_inner(app: &AppHandle, argv: Vec<String>, cwd: PathBuf, restart: &mut bool) -> (i32, String, String) {
    let parsed = match parse(&argv) {
        Ok(p) => p,
        Err(e) => return (1, String::new(), format!("{e}\n")),
    };
    let cmd = match parsed {
        Parsed::Help(text) => return (0, format!("{text}\n"), String::new()),
        Parsed::Run(cmd) => cmd,
    };
    if matches!(cmd, Cmd::Version) {
        return (0, format!("{} {}\n", crate::brand::DISPLAY_NAME, env!("CARGO_PKG_VERSION")), String::new());
    }
    if matches!(cmd, Cmd::Describe) {
        return (0, format!("{}\n", describe_json()), String::new());
    }

    // Backup restore reuses the real, already-tested restore end to end (safety
    // snapshot, staged swap) instead of a second reimplementation that would have
    // to fake what only a running app can really do; the relaunch is left to the
    // caller (see run_in_process_deferred).
    if let Cmd::BackupRestore { name, keep, scope, confirmed } = &cmd {
        if !confirmed {
            return (4, String::new(), format!("refused: {}. Pass --yes to actually do it.\n", dry_run_description(&cmd)));
        }
        let state: tauri::State<Db> = app.state();
        let keep = keep.unwrap_or_else(|| default_keep(&lock(&state).settings));
        return match crate::restore_backup_core(app.clone(), state, name.clone(), keep, scope.clone()) {
            Ok(()) => {
                *restart = true;
                (0, format!("restored backup {name}\n"), String::new())
            }
            Err(e) => {
                let code = if e == "missing" { 2 } else { 3 };
                (code, String::new(), format!("{e}\n"))
            }
        };
    }

    // Autostart and always-on-top live partly outside the store (the Run entry, the
    // window flag): they go through the app's own handlers, which change both.
    if let Some((key, parsed)) = os_backed_toggle(&cmd) {
        let Ok(on) = parsed else {
            return (1, String::new(), format!("{key} takes true or false\n"));
        };
        if crate::DATA_DIR_READONLY.load(Ordering::Relaxed) || crate::SETTINGS_LOCKED.load(Ordering::Relaxed) {
            return (3, String::new(), "the store refused the write (locked or unavailable)\n".to_string());
        }
        let state: tauri::State<Db> = app.state();
        let done = if key == "autostart" {
            crate::set_autostart(app.clone(), state, on).map(|_| ())
        } else {
            crate::set_always_on_top(app.clone(), state, on);
            Ok(())
        };
        return match done {
            Ok(()) => {
                let _ = tauri::Emitter::emit_to(app, "main", "store-synced", ());
                (0, String::new(), String::new())
            }
            Err(e) => (3, String::new(), format!("{e}\n")),
        };
    }

    let state: tauri::State<Db> = app.state();
    let dir = data_dir(app);
    let mut store = lock(&state);
    // (This check also refuses a prompt-only command while only the settings
    // are locked; the standalone caller does not look at SETTINGS_LOCKED.)
    // The same refusal the standalone caller gives, up front: a store that is
    // read-only, locked after a failed decrypt or settings-locked must not even
    // be changed in memory, or the next save that succeeds would write it.
    if is_mutating(&cmd)
        && (crate::DATA_DIR_READONLY.load(Ordering::Relaxed) || store.prompts_locked || crate::SETTINGS_LOCKED.load(Ordering::Relaxed))
    {
        return (3, String::new(), "the store refused the write (locked or unavailable)
".to_string());
    }
    let outcome = exec_persisted(&mut store, &dir, &cmd, &cwd, |st, o| {
        let persisted = if o.mutated_trash {
            crate::save_prompts_and_trash(app, st)
        } else if o.mutated_prompts {
            crate::save_prompts(app, st)
        } else {
            true
        };
        let settings_persisted = if o.mutated_settings { crate::save_settings(app, &st.settings) } else { true };
        persisted && settings_persisted
    });
    drop(store);
    if outcome.code == 0 && (outcome.mutated_prompts || outcome.mutated_trash || outcome.mutated_settings) {
        // The windows, the armed hotkeys and the tray still hold what they read
        // before this command: tell them, or a stale grid keeps a deleted tile and
        // its next layout write puts the cell back.
        let _ = tauri::Emitter::emit_to(app, "main", "store-synced", ());
        crate::rearm_hotkeys(app);
        crate::rebuild_tray(app);
        crate::sync_float_windows(app);
        crate::notify_float_settings(app);
    }
    (outcome.code, outcome.stdout, outcome.stderr)
}

// Runs a command on the live store and keeps its change only when `persist`
// accepts it. A refused write puts prompts, trash and settings back exactly as
// they were, so the caller's "refused" answer is true of the running app too.
fn exec_persisted(store: &mut Store, dir: &Path, cmd: &Cmd, cwd: &Path, persist: impl FnOnce(&Store, &Outcome) -> bool) -> Outcome {
    // Only a command that can change something needs the copy; a read over a
    // large library stays free of it.
    let before = is_mutating(cmd).then(|| (store.prompts.clone(), store.trash.clone(), store.settings.clone()));
    let outcome = exec(store, dir, cmd, cwd);
    if !(outcome.mutated_prompts || outcome.mutated_trash || outcome.mutated_settings) {
        return outcome;
    }
    if persist(store, &outcome) {
        return outcome;
    }
    if let Some(before) = before {
        (store.prompts, store.trash, store.settings) = before;
    }
    // When the prompts write went through and only the settings write failed,
    // memory is back but the disk keeps the prompt change until the next save,
    // which rewrites everything (see the line below).
    // The row bookkeeping may have moved with the failed write: start it over so
    // nothing is skipped as already written on the next save.
    store.forget_written_rows();
    Outcome::err(3, "the store refused the write (locked or unavailable)\n")
}

// ---------- Remote-caller path confinement ----------
//
// A command line launch resolves `export`/`import` against the CALLER's own
// working directory (see resolve_path_arg) because a trusted local user is
// the one typing it. The remote receiver and a promptsaver:// link have no
// such trusted caller behind them, so the same two commands are confined to
// the data folder there: parsed here, in the one module that already owns
// `Cmd`/`parse`, rather than duplicating a second copy of the command
// grammar next to it.
pub(crate) fn confine_path_arg(argv: &[String], base_dir: &Path) -> Result<(), String> {
    let cmd = match parse(argv) {
        Ok(Parsed::Run(cmd)) => cmd,
        // A bad or --help invocation is reported by the real dispatch right
        // after this call; nothing here needs to duplicate that message.
        _ => return Ok(()),
    };
    let raw = match &cmd {
        Cmd::Export { path, .. } | Cmd::Import { path, .. } => path.clone(),
        _ => return Ok(()),
    };
    let resolved = resolve_path_arg(base_dir, &raw);
    // Export's target file usually does not exist yet -- confine its parent
    // directory instead, the same way a shell would resolve a not-yet-created
    // path. An import's source must already exist, so this equally covers it.
    let check_dir = if resolved.exists() {
        resolved.clone()
    } else {
        match resolved.parent() {
            Some(p) => p.to_path_buf(),
            None => return Err("refused: the path escapes the data folder".to_string()),
        }
    };
    let canon_target = check_dir
        .canonicalize()
        .map_err(|_| "refused: the path does not resolve inside the data folder".to_string())?;
    let canon_base = base_dir
        .canonicalize()
        .map_err(|_| "the data folder could not be resolved".to_string())?;
    if !canon_target.starts_with(&canon_base) {
        return Err("refused: the path escapes the data folder".to_string());
    }
    // The data folder also holds the live store and the snapshots, which an export
    // would overwrite with JSON and no safety copy: only the folder's other files
    // are valid targets.
    if matches!(cmd, Cmd::Export { .. }) {
        let target = if resolved.exists() {
            canon_target
        } else {
            match resolved.file_name() {
                Some(name) => canon_target.join(name),
                None => return Err("refused: the path has no file name".to_string()),
            }
        };
        if is_store_path(&target, &canon_base) {
            return Err("refused: that path belongs to the stored data".to_string());
        }
    }
    Ok(())
}

// True for a path that is part of the stored data inside the data folder: the
// database with its write-ahead files, the legacy json files, the folder lock,
// and anything under the snapshots and attachment folders. Both arguments are
// canonical. File names compare case-insensitively (Windows).
fn is_store_path(target: &Path, base: &Path) -> bool {
    let Ok(rel) = target.strip_prefix(base) else {
        return false;
    };
    let mut parts = rel.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase());
    let Some(first) = parts.next() else {
        return true; // the folder itself
    };
    if parts.next().is_some() {
        return matches!(first.as_str(), "backups" | "files");
    }
    matches!(first.as_str(), "backups" | "files" | "data.db-wal" | "data.db-shm")
        || crate::STORE_FILES.iter().any(|f| f.eq_ignore_ascii_case(&first))
        || first.eq_ignore_ascii_case(crate::DATA_DIR_LOCK_FILE)
}

// ---------- Help text ----------

fn top_level_help() -> String {
    "Prompt Saver command line\n\n\
     Usage: prompt-saver <command> [arguments] [--json] [--yes]\n\n\
     Commands:\n\
     \x20 list      list prompts\n\
     \x20 get       show one prompt\n\
     \x20 copy      copy a prompt's text to the clipboard\n\
     \x20 add       add a prompt\n\
     \x20 edit      change a prompt\n\
     \x20 delete    move a prompt to the trash\n\
     \x20 restore   restore a prompt from the trash\n\
     \x20 tag       list|add|remove tags\n\
     \x20 view      list|add|remove views\n\
     \x20 search    search prompts\n\
     \x20 export    export prompts to a file\n\
     \x20 import    import prompts from a file\n\
     \x20 backup    create|list|restore backups\n\
     \x20 settings  get|set a setting\n\
     \x20 version   print the app version\n\n\
     Every command takes --help. Every command that changes something needs\n\
     --yes, or it only prints what it would have done. --json switches a\n\
     command's output to machine-readable JSON. Exit codes: 0 ok, 1 bad\n\
     invocation, 2 not found, 3 store locked/unavailable, 4 refused."
        .to_string()
}

fn command_help(cmd: &str, sub: Option<&str>) -> String {
    match (cmd, sub) {
        ("list", _) => "prompt-saver list [--tag NAME] [--favorite] [--trash] [--json]\n\nList prompts, one per line as id, name, favorite mark, tags.".to_string(),
        ("get", _) => "prompt-saver get <id> [--json]\n\nShow one prompt's fields. Exit 2 if the id does not exist.".to_string(),
        ("copy", _) => "prompt-saver copy <id>\n\nCopy a prompt's text to the clipboard. Exit 2 if the id does not exist.".to_string(),
        ("add", _) => "prompt-saver add --name NAME --text TEXT [--tags a,b] [--favorite] [--notes TEXT] [--color #rrggbb] --yes [--json]\n\nAdd a new prompt. Prints its new id.".to_string(),
        ("edit", _) => "prompt-saver edit <id> [--name NAME] [--text TEXT] [--tags a,b] [--favorite|--no-favorite] [--notes TEXT] [--color #rrggbb] --yes [--json]\n\nChange one or more fields of an existing prompt.".to_string(),
        ("delete", _) => "prompt-saver delete <id> --yes\n\nMove a prompt to the trash.".to_string(),
        ("restore", _) => "prompt-saver restore <id> --yes [--json]\n\nRestore a prompt out of the trash.".to_string(),
        ("tag", None) => "prompt-saver tag list [--json]\nprompt-saver tag add <id> <tag> --yes\nprompt-saver tag remove <id> <tag> --yes".to_string(),
        ("tag", Some("list")) => "prompt-saver tag list [--json]\n\nList every tag in use with its prompt count.".to_string(),
        ("tag", Some("add")) => "prompt-saver tag add <id> <tag> --yes\n\nAdd one tag to one prompt.".to_string(),
        ("tag", Some("remove")) => "prompt-saver tag remove <id> <tag> --yes\n\nRemove one tag from one prompt.".to_string(),
        ("view", None) => "prompt-saver view list [--json]\nprompt-saver view add <name> [--color #rrggbb] --yes\nprompt-saver view remove <id> --yes".to_string(),
        ("view", Some("list")) => "prompt-saver view list [--json]\n\nList every view.".to_string(),
        ("view", Some("add")) => "prompt-saver view add <name> [--color #rrggbb] --yes\n\nAdd a new, empty view. Prints its new id.".to_string(),
        ("view", Some("remove")) => "prompt-saver view remove <id> --yes\n\nRemove a view (refused for the last remaining one).".to_string(),
        ("search", _) => "prompt-saver search <query> [--tag NAME] [--json]\n\nCase-insensitive search over name, text, notes and tags.".to_string(),
        ("export", _) => "prompt-saver export <path> [--format json|csv|txt] [--ids a,b,c] [--yes]\n\nWrite prompts to a file. --yes is only needed to overwrite an existing file.".to_string(),
        ("import", _) => "prompt-saver import <path> --yes\n\nAppend prompts read from a file this app exported.".to_string(),
        ("backup", None) => "prompt-saver backup create [--keep N] [--json]\nprompt-saver backup list [--json]\nprompt-saver backup restore <name> [--scope all|prompts|settings] [--keep N] --yes".to_string(),
        ("backup", Some("create")) => "prompt-saver backup create [--keep N] [--json]\n\nSnapshot the store now. Prints the new backup's name.".to_string(),
        ("backup", Some("list")) => "prompt-saver backup list [--json]\n\nList existing backups, newest first.".to_string(),
        ("backup", Some("restore")) => "prompt-saver backup restore <name> [--scope all|prompts|settings] [--keep N] --yes\n\nReplace the store with a backup. Always takes a safety snapshot first.".to_string(),
        ("settings", None) => "prompt-saver settings get <key> [--json]\nprompt-saver settings set <key> <value> --yes".to_string(),
        ("settings", Some("get")) => "prompt-saver settings get <key> [--json]\n\nRead one setting. Exit 2 if the key is unknown.".to_string(),
        ("settings", Some("set")) => "prompt-saver settings set <key> <value> --yes\n\nWrite one setting. The value is read as a whole number/decimal, true/false, or plain text, in that order, unless the key is one of the app's own fixed settings (theme, language, autostart, alwaysOnTop). Settings that grant access (folders, the vault, the backup password, the remote access switches and values, the link and clipboard switches, the voice consent and the data folder lock) are refused; change them in the app.".to_string(),
        ("version", _) => "prompt-saver version\n\nPrint the app name and version.".to_string(),
        _ => top_level_help(),
    }
}

// ---------- Machine-readable self-description (`prompt-saver describe`) ----------
//
// docs/automation.md's command table is generated from this, never typed by
// hand a second time: adding a command here without regenerating the doc is
// caught by tests/automation_doc_check.py (see tests/README.md). The
// (name, subcommand) pairs below are exactly the ones parse() recognizes;
// each entry's usage/description text is command_help()'s own text, split at
// its first blank line, so the two can never drift apart.
const MANIFEST_ENTRIES: &[(&str, Option<&str>)] = &[
    ("list", None),
    ("get", None),
    ("copy", None),
    ("add", None),
    ("edit", None),
    ("delete", None),
    ("restore", None),
    ("tag", Some("list")),
    ("tag", Some("add")),
    ("tag", Some("remove")),
    ("view", Some("list")),
    ("view", Some("add")),
    ("view", Some("remove")),
    ("search", None),
    ("export", None),
    ("import", None),
    ("backup", Some("create")),
    ("backup", Some("list")),
    ("backup", Some("restore")),
    ("settings", Some("get")),
    ("settings", Some("set")),
    ("version", None),
];

// Full names, exactly as they appear in the "name" field below (top-level
// command, or "<command> <subcommand>") -- the commands that need --yes.
const MUTATING_COMMAND_NAMES: &[&str] =
    &["add", "edit", "delete", "restore", "tag add", "tag remove", "view add", "view remove", "import", "backup restore", "settings set"];

#[derive(serde::Serialize)]
struct CommandSpec {
    name: String,
    usage: String,
    description: String,
    mutating: bool,
}

fn command_manifest() -> Vec<CommandSpec> {
    MANIFEST_ENTRIES
        .iter()
        .map(|(name, sub)| {
            let full_name = match sub {
                Some(s) => format!("{name} {s}"),
                None => (*name).to_string(),
            };
            let help = command_help(name, *sub);
            let (usage, description) = match help.split_once("\n\n") {
                Some((u, d)) => (u.to_string(), d.trim().to_string()),
                None => (help, String::new()),
            };
            let mutating = MUTATING_COMMAND_NAMES.contains(&full_name.as_str());
            CommandSpec { name: full_name, usage, description, mutating }
        })
        .collect()
}

// The JSON `prompt-saver describe` prints: enough for a program to drive the
// tool without reading source -- the full command list, and the exit codes
// every command shares (see top_level_help()).
fn describe_json() -> String {
    let doc = serde_json::json!({
        "app": crate::brand::DISPLAY_NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "commands": command_manifest(),
        "exit_codes": {
            "0": "ok",
            "1": "bad invocation",
            "2": "not found",
            "3": "store locked or unavailable",
            "4": "refused",
        },
    });
    serde_json::to_string_pretty(&doc).unwrap_or_default()
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
