// Command line: `prompt-saver <command> ...`. A launch whose first argument names one of the
// commands below never opens a window. If the app is already running, the launch hands its
// arguments to that instance over a named pipe and prints the answer; otherwise it opens the
// store itself, runs the command alone, prints the result and exits. Either way only one
// process touches the database at a time.
//
// Layout: `parse()` turns argv into a `Cmd`; `exec()` is the pure core that runs a `Cmd`
// against a `Store` in memory (shared by the standalone path and the running instance, so
// they cannot drift apart); `run_standalone()` and `run_in_process_deferred()` are the two thin
// callers that load and persist the store around it; the pipe framing and the launch-side
// dispatcher sit at the top.

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

#[path = "cli_callers.rs"]
mod callers;
pub(crate) use callers::*;

// ---------- Launch-side entry point ----------

// A launch is a GUI start only when it carries nothing, the autostart flag, or a first word that
// is a path (a file handed over by a shortcut or "Open with"). Everything else is a command: a
// typo, `help` or a flag in front would otherwise open the window (or hand off to the running
// instance and exit 0) and a script would read success for a command that never ran. The parser
// answers an unknown command with exit 1.
fn looks_like_cli(argv: &[String]) -> bool {
    match argv.first().map(String::as_str) {
        None | Some("") | Some("--minimized") => false,
        Some(a) if a.starts_with('-') => true,
        Some(a) => !a.contains(['\\', '/', ':', '.']),
    }
}

// How this launch's raw arguments read. A first word that is not valid Unicode can only be a file
// name, so it is a GUI start; a later one inside a command is refused instead of panicking.
#[derive(Debug, PartialEq)]
enum LaunchArgs {
    Gui,
    Cli(Vec<String>),
    BadArgument,
}

fn read_launch_args(raw: impl IntoIterator<Item = std::ffi::OsString>) -> LaunchArgs {
    let mut argv: Vec<String> = Vec::new();
    let mut non_unicode = false;
    for a in raw {
        match a.into_string() {
            Ok(s) => argv.push(s),
            Err(_) if argv.is_empty() => return LaunchArgs::Gui,
            Err(_) => non_unicode = true,
        }
    }
    if !looks_like_cli(&argv) {
        LaunchArgs::Gui
    } else if non_unicode {
        LaunchArgs::BadArgument
    } else {
        LaunchArgs::Cli(argv)
    }
}

// Called at the very top of run(), before anything Tauri-related exists.
// `Some(code)` means: exit with this code, no window was ever created.
// `None` means: this launch is an ordinary GUI start, carry on as before.
pub(crate) fn dispatch_from_launch_args() -> Option<i32> {
    let launch = read_launch_args(std::env::args_os().skip(1));
    if launch == LaunchArgs::Gui {
        return None;
    }
    #[cfg(windows)]
    attach_parent_console_if_needed();
    let argv = match launch {
        LaunchArgs::Cli(argv) => argv,
        _ => {
            eprintln!("an argument is not valid text");
            return Some(1);
        }
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let rt = tokio::runtime::Builder::new_current_thread().enable_io().enable_time().build();
    if let Ok(rt) = rt {
        if let Some(code) = rt.block_on(try_forward(&argv, &cwd)) {
            return Some(code);
        }
    }
    Some(run_standalone(&argv, &cwd))
}

// A windows_subsystem="windows" release build starts with no console, so a launch typed into
// an existing terminal would print nothing. Attaching to the caller's console (only that; a
// debug build or a launch with a real std handle is left alone) makes the output show there.
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
        // Already has a working handle (debug build or own console): attaching again would only fail.
        if let Ok(h) = GetStdHandle(STD_OUTPUT_HANDLE) {
            if !h.is_invalid() {
                return;
            }
        }
        // No parent console to attach to (Explorer, a shortcut, the Task Scheduler): stay silent.
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
// The single-instance plugin (see run()) forwards a second launch's argv to the running
// instance but has no channel for an answer. This pipe is that channel, and the second
// process uses it for the whole round trip, keeping control of its own exit code instead of
// fighting the plugin's unconditional exit(0).

fn pipe_name() -> String {
    format!(r"\\.\pipe\{}-cli", crate::brand::IDENTIFIER)
}

const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
// Windows' busy-pipe error code (ERROR_PIPE_BUSY), retried rather than read as "nobody is
// listening".
const ERROR_PIPE_BUSY: i32 = 231;
const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_ACCESS_DENIED: i32 = 5;

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

// What opening the pipe failed with, as far as the launch's next step is concerned.
#[derive(Debug, PartialEq)]
enum OpenFailure {
    // Every server instance is serving someone: an instance exists, so the command must not run
    // beside it.
    Busy,
    // No pipe, or one this user may not open (another user's session owns the name): no instance
    // of this user can be reached, so the command runs standalone.
    NoInstance,
    // Anything else says nothing about whether an instance runs; running standalone beside one
    // would have two processes writing the database.
    Unreachable,
}

fn classify_open_failure(code: Option<i32>) -> OpenFailure {
    match code {
        Some(ERROR_PIPE_BUSY) => OpenFailure::Busy,
        Some(ERROR_FILE_NOT_FOUND | ERROR_ACCESS_DENIED) => OpenFailure::NoInstance,
        _ => OpenFailure::Unreachable,
    }
}

// How long a launch waits for a busy pipe before it gives up with exit 3.
const FORWARD_BUSY_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

// Tries the already-running instance. `None` means no instance of this user is listening and
// the caller falls back to standalone. `Some(code)` means this is the launch's exit code: an
// answer that was printed, or exit 3 when an instance may be running but could not be used.
async fn try_forward(argv: &[String], cwd: &Path) -> Option<i32> {
    try_forward_at(&pipe_name(), FORWARD_BUSY_WAIT, argv, cwd).await
}

async fn try_forward_at(name: &str, busy_wait: std::time::Duration, argv: &[String], cwd: &Path) -> Option<i32> {
    use tokio::net::windows::named_pipe::ClientOptions;
    let deadline = std::time::Instant::now() + busy_wait;
    let mut client = loop {
        match ClientOptions::new().open(name) {
            Ok(c) => break c,
            Err(e) => match classify_open_failure(e.raw_os_error()) {
                OpenFailure::Busy => {
                    if std::time::Instant::now() >= deadline {
                        eprintln!("prompt-saver: the running instance is busy and did not accept the command within 5 seconds");
                        return Some(3);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                OpenFailure::NoInstance => return None,
                OpenFailure::Unreachable => {
                    eprintln!("prompt-saver: could not reach the running instance: {e}");
                    return Some(3);
                }
            },
        }
    };
    // The argv (paths, passwords of an import) goes only to a server of this same user; any
    // other owner of the pipe name is not the app, so the command runs standalone instead.
    if !forward_target_trusted(named_pipe_server_pid(&client)) {
        return None;
    }
    let req = PipeRequest { argv: argv.to_vec(), cwd: cwd.to_string_lossy().to_string() };
    let bytes = serde_json::to_vec(&req).ok()?;
    match exchange(&mut client, &bytes, FORWARD_HANDOFF, FORWARD_RESULT_WAIT).await {
        Forwarded::Answer(resp) => {
            if !resp.stdout.is_empty() {
                print!("{}", resp.stdout);
            }
            if !resp.stderr.is_empty() {
                eprint!("{}", resp.stderr);
            }
            Some(resp.code)
        }
        // The running instance never took the request, so nothing ran: the store is unreachable from
        // here for now, like a foreign machine's fresh data-dir lock (exit 3).
        Forwarded::NotHandedOver => {
            eprintln!("prompt-saver: the running instance did not accept the command within 5 seconds");
            Some(3)
        }
        Forwarded::Closed => {
            eprintln!("prompt-saver: the running instance closed the connection without an answer; the command may or may not have run");
            Some(3)
        }
        Forwarded::TimedOut => {
            eprintln!("prompt-saver: no result after 10 minutes; the command may still be running in the running instance");
            Some(3)
        }
    }
}

// How long writing the request into the pipe may take, and how long the finished
// command is waited for afterwards (a backup, restore or export can run long).
const FORWARD_HANDOFF: std::time::Duration = std::time::Duration::from_secs(5);
const FORWARD_RESULT_WAIT: std::time::Duration = std::time::Duration::from_secs(10 * 60);

enum Forwarded {
    Answer(PipeResponse),
    NotHandedOver,
    Closed,
    TimedOut,
}

// One request/answer round over a connected pipe: the hand-off is bounded by
// `handoff`; once the request is written, the answer is awaited for up to `wait`.
async fn exchange<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    pipe: &mut S,
    request: &[u8],
    handoff: std::time::Duration,
    wait: std::time::Duration,
) -> Forwarded {
    match tokio::time::timeout(handoff, write_frame(pipe, request)).await {
        Ok(Ok(())) => {}
        _ => return Forwarded::NotHandedOver,
    }
    match tokio::time::timeout(wait, read_frame(pipe, MAX_FRAME_BYTES)).await {
        Ok(Ok(bytes)) => match serde_json::from_slice::<PipeResponse>(&bytes) {
            Ok(resp) => Forwarded::Answer(resp),
            Err(_) => Forwarded::Closed,
        },
        Ok(Err(_)) => Forwarded::Closed,
        Err(_) => Forwarded::TimedOut,
    }
}

// Started once from run()'s setup(), independent of the single-instance plugin.
// `first_pipe_instance` makes a second app instance's attempt to start a second server fail
// cleanly (relevant with "allow multiple instances" on), so only one copy answers the pipe.
// A tokio named-pipe handle registers with the runtime's reactor when created, so creating
// one on the plain setup() thread would panic the process before a window exists. The first
// server instance is therefore created inside the spawned task, like every later one.
pub(crate) fn install_server(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        serve_pipe(&pipe_name(), SERVER_RETRY, move |connected| {
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move { handle_connection(connected, app2).await });
        })
        .await
    });
}

// How long the server waits before it tries to create a pipe instance again.
const SERVER_RETRY: std::time::Duration = std::time::Duration::from_secs(3);

// The accept loop. It never ends for the life of the process: a pipe nobody serves sends every
// command line call to a standalone run beside the live app. A first instance that cannot be
// created yet (another copy of the app owns the name, and may quit later) is retried, a client
// that hangs up before it is accepted costs only that one instance, and a failed create backs off.
async fn serve_pipe(
    name: &str,
    retry: std::time::Duration,
    mut on_connection: impl FnMut(tokio::net::windows::named_pipe::NamedPipeServer),
) {
    use tokio::net::windows::named_pipe::ServerOptions;
    let mut server = loop {
        match ServerOptions::new().first_pipe_instance(true).create(name) {
            Ok(s) => break s,
            Err(_) => tokio::time::sleep(retry).await,
        }
    };
    loop {
        let (connected, next) = accept_next(server, name, retry).await;
        server = next;
        if let Some(connected) = connected {
            on_connection(connected);
        }
    }
}

// Waits for one client on `server`. Gives back the connected instance (None when the client hung
// up before it was accepted, which is no reason to stop serving) and the fresh instance to wait on
// next. The next instance is queued before the connected one is handed off, so a client that
// arrives while a request runs is accepted at once instead of racing a busy pipe.
async fn accept_next(
    server: tokio::net::windows::named_pipe::NamedPipeServer,
    name: &str,
    retry: std::time::Duration,
) -> (
    Option<tokio::net::windows::named_pipe::NamedPipeServer>,
    tokio::net::windows::named_pipe::NamedPipeServer,
) {
    use tokio::net::windows::named_pipe::ServerOptions;
    let connected = server.connect().await.is_ok();
    let next = loop {
        match ServerOptions::new().create(name) {
            Ok(s) => break s,
            Err(_) => tokio::time::sleep(retry).await,
        }
    };
    (connected.then_some(server), next)
}

async fn handle_connection(mut pipe: tokio::net::windows::named_pipe::NamedPipeServer, app: AppHandle) {
    let Ok(req_bytes) = read_frame(&mut pipe, MAX_FRAME_BYTES).await else { return };
    let Ok(req) = serde_json::from_slice::<PipeRequest>(&req_bytes) else { return };
    if req.argv == [SHOW_PING.to_string()] {
        respond_to_show_ping(&mut pipe, &app).await;
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

// A second GUI launch's proof that this instance is alive, not just accepting connections.
// The answer is sent from inside the main thread's own closure (queuing always succeeds even
// when the main thread is wedged; see the tray-window comment above). It is sent before
// show_main does its work, because raising a window or rebuilding a missing one can outlast
// the launch side's patience, and a slow window is not a hung process. Reaching the closure
// is the liveness proof.
async fn respond_to_show_ping(pipe: &mut tokio::net::windows::named_pipe::NamedPipeServer, app: &AppHandle) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let app2 = app.clone();
    if app
        .run_on_main_thread(move || {
            let _ = tx.send(());
            show_main(&app2);
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
// A second launch that is not a CLI command pings the same pipe instead of trusting the
// single-instance plugin's unconditional forward-and-exit.

const SHOW_PING: &str = "\0show-ping\0"; // never a valid subcommand or path
const SECOND_LAUNCH_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

#[derive(Debug, PartialEq)]
pub(crate) enum LaunchDecision {
    // Nothing is listening: this launch is the primary instance.
    Primary,
    // The running instance answered: this launch is done.
    HandedOff,
    // The running instance did not answer in time and was taken over --
    // this launch continues on as the new primary instance.
    TookOver,
}

// The one-shot runtime the handshake runs on, shared by both branches below.
fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread().enable_io().enable_time().build()
}

pub(crate) fn resolve_second_launch() -> LaunchDecision {
    let Ok(rt) = runtime() else {
        return LaunchDecision::HandedOff; // could not even try -- behave as before
    };
    rt.block_on(resolve_second_launch_async())
}

async fn resolve_second_launch_async() -> LaunchDecision {
    use tokio::net::windows::named_pipe::ClientOptions;
    let name = pipe_name();
    // Only two outcomes end this launch: a clear answer (hand off) or a proven hang that was
    // ended (take over). Anything else (a busy pipe, a broken connection, an unidentifiable
    // process) falls through to Primary and leaves the decision to the single-instance plugin,
    // since exiting on a doubt would leave the user with no window.
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
    // The pid is read off the still-connected handle, before anything can end the server and let
    // Windows hand the number to someone else.
    let server_pid = named_pipe_server_pid(&client);
    let req = PipeRequest { argv: vec![SHOW_PING.to_string()], cwd: String::new() };
    let Ok(bytes) = serde_json::to_vec(&req) else { return LaunchDecision::Primary };
    // Its own full budget: time spent waiting for a busy pipe must not shrink the window the
    // running instance gets to prove it is alive.
    let roundtrip = async {
        write_frame(&mut client, &bytes).await.ok()?;
        read_frame(&mut client, MAX_FRAME_BYTES).await.ok()
    };
    let answered = tokio::time::timeout(SECOND_LAUNCH_TIMEOUT, roundtrip).await.ok().flatten().is_some();
    if answered {
        return LaunchDecision::HandedOff;
    }
    // Connected, but the main thread never reached the answer: a wedged instance (the answer goes
    // out before show_main does any work, so a slow or rebuilding window never looks like a hang).
    let Some(pid) = server_pid else { return LaunchDecision::Primary };
    let (same_user, same_exe) = process_identity_check(pid);
    if should_take_over_hung_instance(false, same_user, same_exe) {
        // Terminate and wait: TerminateProcess only asks. Racing ahead while the old process still
        // holds the single-instance mutex, the data-dir lock and the pipe would leave this launch
        // without a window.
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

// Pure decision, independent of the pipe/timeout machinery: whether a second launch that
// failed to reach the running instance in time may end it and continue as primary. Never
// crosses a user or executable boundary; a hung process of someone else, or of a different
// build, is left alone.
fn should_take_over_hung_instance(answered_in_time: bool, same_user: bool, same_exe_path: bool) -> bool {
    !answered_in_time && same_user && same_exe_path
}

// Whether the process behind the pipe may receive a forwarded command: it must run as the same
// user. An unknown owner counts as not trusted.
fn forward_target_trusted(server_pid: Option<u32>) -> bool {
    server_pid.is_some_and(|pid| process_identity_check(pid).0)
}

// GetNamedPipeServerProcessId (Win32_System_Pipes) is not in this project's enabled `windows`
// crate features, so this one call stays a direct kernel32 binding, like a few other modules'
// user32/kernel32 calls.
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

// (same_user, same_exe_path) for the process behind `pid`. Either half is false on any query
// failure; an unknown answer is never treated as a match.
fn process_identity_check(pid: u32) -> (bool, bool) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::{EqualSid, GetTokenInformation, TokenUser, PSID, TOKEN_QUERY};
    use windows::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // The Sid field of a queried TOKEN_USER points into its info buffer, so the buffer must
    // outlive every use of the pointer; it is returned together rather than as a bare PSID a
    // caller could dangle.
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

// Ends `pid` and waits until it is really gone. `true` only when the process has exited: the
// caller may not touch the mutex, data-dir lock or pipe the old instance owns until then.
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

// A prompt/view identifier: permissive enough for every id this or an older build minted
// (see gen_id), narrow enough to reject a stray path, a flag or control characters.
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

// A relative path argument resolves against the caller's working directory, not this
// process': a second launch forwarded to the running instance has a different one.
fn resolve_path_arg(cwd: &Path, raw: &str) -> PathBuf {
    let p = Path::new(raw);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}

// A tiny cursor over the remaining args, shared by every subcommand's parser. It owns its
// items: taking a flag out replaces the list instead of leaking a copy, which matters now
// that the app parses a command for every pipe request without exiting.
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
        // Machine-facing only: its JSON self-description, not listed in top_level_help().
        // docs/automation.md's command table is generated from it (see command_manifest() below).
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

// Whether the remaining args are exactly a bare `--help`/`-h`, checked before the real parse
// so `--help` works even when required arguments are missing.
fn wants_help(a: &Args) -> bool {
    a.items.get(a.pos..).is_some_and(|rest| rest == ["--help"] || rest == ["-h"])
}

fn take_flag_value(a: &mut Args, name: &str) -> Result<Option<String>, String> {
    let mut i = a.pos;
    while i < a.items.len() {
        if a.items[i] == name {
            let val = a.items.get(i + 1).ok_or_else(|| usage_err(format!("{name} needs a value")))?.clone();
            // Remove the flag and its value so positional parsing never sees them.
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
    // An empty selection (a script whose list came out empty) must not turn into "everything".
    if ids.as_ref().is_some_and(|list| list.is_empty()) {
        return Err(usage_err("--ids needs at least one prompt id"));
    }
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
                return Err(usage_err("--scope must be all, data or settings"));
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

// Every command that writes into the data folder, so a read-only instance refuses it. Wider than
// is_mutating: a backup create needs no confirmation and no store snapshot, but it still adds a
// backup and prunes older ones in the folder another machine owns.
fn writes_data_folder(cmd: &Cmd) -> bool {
    is_mutating(cmd) || matches!(cmd, Cmd::BackupCreate { .. })
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

// What a rejected mutating command would have done, shown instead of doing it, so a script
// that forgot --yes learns what it almost triggered.
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
        .filter(|p| key.as_deref().is_none_or(|k| p.tags.iter().any(|t| tag_key(t) == k)))
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
        return Outcome::err(4, format!("prompt {id} is confidential and can only be read in the app itself\n"));
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
    // Same rule as `get`: a confidential prompt's text is only shown in the app, and the store
    // holds ciphertext for it.
    if p.secret {
        return Outcome::err(4, format!("prompt {id} is confidential and can only be copied in the app itself\n"));
    }
    // Our own copy: the clipboard watcher must not take it for a foreign one and can see the
    // change before the write returns, so note it first.
    crate::note_own_copy(&p.text);
    match arboard::Clipboard::new().and_then(|mut c| c.set_text(p.text.clone())) {
        Ok(()) => Outcome::ok(String::new()),
        Err(e) => Outcome::err(3, format!("clipboard unavailable: {e}\n")),
    }
}

fn cmd_search(store: &Store, query: &str, tag: Option<&str>, json: bool) -> Outcome {
    let q = query.to_lowercase();
    let key = tag.map(tag_key);
    let filtered: Vec<&Prompt> = store
        .prompts
        .iter()
        .filter(|p| key.as_deref().is_none_or(|k| p.tags.iter().any(|t| tag_key(t) == k)))
        .filter(|p| {
            q.is_empty()
                || p.name.to_lowercase().contains(&q)
                // A confidential prompt's stored text is ciphertext: matching it is noise.
                || (!p.secret && p.text.to_lowercase().contains(&q))
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
// A prompt made or changed here gets every field the store needs (id, timestamps, defaults)
// but is never placed into a view's grid layout, like a prompt whose view was deleted (see
// View::layouts: "a member with no cell ... is an overflow entry, not a lost one"). It shows
// in list/get/search at once; arranging it on a grid is the app's job.

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
        name: crate::md_sync::sync_clean_name(name),
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
    // A confidential entry's text is sealed with the vault key, which the command line never
    // holds: plain text written here would sit on disk as is and fail to open in the app.
    if p.secret && text.is_some() {
        return Outcome::err(4, format!("prompt {id} is confidential and its text can only be changed in the app itself\n"));
    }
    if let Some(n) = name {
        p.name = crate::md_sync::sync_clean_name(n);
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
        name: crate::md_sync::sync_clean_name(name),
        cols: 5,
        rows: 4,
        layouts: std::collections::HashMap::new(),
        color: color.unwrap_or("").to_string(),
        members: Vec::new(),
        sync_ids: Vec::new(),
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
    // Also dissolves the view's grid folders, so their members are not left claimed by a folder
    // nobody can reach.
    let prompt_ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
    if let Err(e) = crate::views::remove_view_from_settings(&mut store.settings, id, &prompt_ids) {
        return Outcome::err(4, format!("{e}\n"));
    }
    Outcome::ok(String::new()).mutating_settings()
}

// ---------- Export / import ----------

fn cmd_export(store: &Store, cwd: &Path, path: &str, format: &str, ids: Option<&[String]>, confirmed: bool) -> Outcome {
    let selected: Vec<Prompt> = match ids {
        Some(list) => {
            let set: std::collections::HashSet<&str> = list.iter().map(String::as_str).collect();
            let unknown: Vec<&str> = list.iter().map(String::as_str).filter(|id| !store.prompts.iter().any(|p| p.id == *id)).collect();
            if !unknown.is_empty() {
                return Outcome::err(2, format!("no prompt with id {}\n", unknown.join(", ")));
            }
            store.prompts.iter().filter(|p| set.contains(p.id.as_str())).cloned().collect()
        }
        None => store.prompts.clone(),
    };
    // A confidential entry holds sealed ciphertext in its text fields and never leaves in a file,
    // as with the app's own export.
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
        // Overwriting an existing file is the one export side effect that can destroy something, so
        // it is the only spot that needs confirmation.
        return Outcome::err(4, format!("refused: \"{}\" already exists. Pass --yes to overwrite it.\n", out_path.display()));
    }
    match crate::write_bytes_atomic(&out_path, content.as_bytes()) {
        Ok(()) => Outcome::ok(format!("exported {} prompt(s) to {}\n", resolved.len(), out_path.display())),
        Err(e) => Outcome::err(1, format!("could not write {}: {e}\n", out_path.display())),
    }
}

fn cmd_import(store: &mut Store, cwd: &Path, path: &str) -> Outcome {
    let in_path = resolve_path_arg(cwd, path);
    let bytes = match read_file_capped(&in_path, MAX_IMPORT_BYTES) {
        Ok(b) => b,
        // A bad path is the caller's to fix, not a store problem to retry later.
        Err(e) => return Outcome::err(if in_path.exists() { 1 } else { 2 }, format!("{e}\n")),
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

// Shared by both callers of `backup restore`, which is special-cased outside exec(): the
// live path reuses the production restore_backup command end to end, including its app
// restart. This is the standalone half: the same safety snapshot, staging and swap, minus
// anything that needs a running app.
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
    // A restore that brings settings may have switched the picture-text index off while the
    // prompts that stay still carry its text.
    if scope != "data" {
        clear_ocr_text_after_restore(dir, settings.ui_flags.get("ocrIndex").copied().unwrap_or(false))?;
    }
    Ok(())
}

// Erases the picture text in the restored data when the index was on before and the restored
// settings have it off; writes only when something was cleared.
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
    // Leftover receiver secrets from an earlier version, the vault salt and verifier and the
    // sealed backup password never travel over the command line: they would give anyone who
    // reads them what an offline guess needs.
    if crate::is_remote_secret_key(key) || crate::is_vault_key(key) || key == crate::BACKUP_PW_KEY {
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
    if crate::is_remote_secret_key(key) {
        return Outcome::err(4, format!("{key} is managed by the app and cannot be set\n"));
    }
    // Folder authority, the vault keys, the backup password and the import-denied switches
    // have dedicated writers and are refused for an import too; this generic setter must not
    // bypass them.
    if crate::is_protected_folder_key(key)
        || crate::is_vault_key(key)
        || key == crate::BACKUP_PW_KEY
        || crate::is_import_denied_flag(key)
        || crate::is_import_denied_value(key)
    {
        return Outcome::err(4, format!("{key} can only be changed in the app itself\n"));
    }
    let s = &mut store.settings;
    match key {
        "theme" => s.theme = value.to_string(),
        "language" => s.language = value.to_string(),
        "autostart" => match value.parse::<bool>() {
            Ok(b) => s.autostart = b,
            Err(_) => return Outcome::err(4, "autostart takes true or false\n".to_string()),
        },
        "alwaysOnTop" => match value.parse::<bool>() {
            Ok(b) => s.always_on_top = b,
            Err(_) => return Outcome::err(4, "alwaysOnTop takes true or false\n".to_string()),
        },
        _ => {
            // A key that already holds a value keeps its kind: a text setting "2024" stays text, and a
            // switch never turns into a stray number.
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
                return Outcome::err(4, format!("{key} takes {wants}\n"));
            }
            if let Some(b) = as_flag {
                // Switching the picture-text index off here erases its text, as the switch in the app does.
                if key == "ocrIndex" && !b && s.ui_flags.get(key).copied().unwrap_or(false) {
                    let cleared = crate::clear_ocr_index_text(store);
                    store.settings.ui_flags.insert(key.to_string(), b);
                    let out = Outcome::ok(String::new()).mutating_settings();
                    return if cleared { out.mutating().mutating_trash() } else { out };
                }
                s.ui_flags.insert(key.to_string(), b);
            } else if let Some(n) = as_number {
                // "NaN", "inf" and "1e999" parse as numbers but cannot be stored: they are written as null
                // and the whole settings block fails to load.
                let Some(n) = crate::clamp_ui_value(key, n) else {
                    return Outcome::err(4, format!("{key} needs a finite number
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
        ("backup", None) => "prompt-saver backup create [--keep N] [--json]\nprompt-saver backup list [--json]\nprompt-saver backup restore <name> [--scope all|data|settings] [--keep N] --yes".to_string(),
        ("backup", Some("create")) => "prompt-saver backup create [--keep N] [--json]\n\nSnapshot the store now. Prints the new backup's name.".to_string(),
        ("backup", Some("list")) => "prompt-saver backup list [--json]\n\nList existing backups, newest first.".to_string(),
        ("backup", Some("restore")) => "prompt-saver backup restore <name> [--scope all|data|settings] [--keep N] --yes\n\nReplace the store with a backup. Always takes a safety snapshot first.".to_string(),
        ("settings", None) => "prompt-saver settings get <key> [--json]\nprompt-saver settings set <key> <value> --yes".to_string(),
        ("settings", Some("get")) => "prompt-saver settings get <key> [--json]\n\nRead one setting. Exit 2 if the key is unknown.".to_string(),
        ("settings", Some("set")) => "prompt-saver settings set <key> <value> --yes\n\nWrite one setting. The value is read as a whole number/decimal, true/false, or plain text, in that order, unless the key is one of the app's own fixed settings (theme, language, autostart, alwaysOnTop). Settings that grant access (folders, the vault, the backup password, the clipboard switches, the voice consent and the data folder lock) are refused; change them in the app.".to_string(),
        ("version", _) => "prompt-saver version\n\nPrint the app name and version.".to_string(),
        _ => top_level_help(),
    }
}

// ---------- Machine-readable self-description (`prompt-saver describe`) ----------
//
// docs/automation.md's command table is generated from this, so adding a command here
// without regenerating the doc is caught by tests/automation_doc_check.py (see
// tests/README.md). The (name, subcommand) pairs are exactly the ones parse() recognizes;
// each entry's usage/description is command_help()'s own text split at its first blank line,
// so the two cannot drift apart.
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

// Full names as in the "name" field below (top-level command, or "<command> <subcommand>"):
// the commands that need --yes.
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

// The JSON `prompt-saver describe` prints: enough for a program to drive the tool without
// reading source, with the full command list and the exit codes every command shares (see
// top_level_help()).
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
