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
    valid_backup_name, valid_scope, valid_tag_color, valid_tag_name, Db, Prompt, Store,
    WrittenRows, MAX_IMPORT_BYTES,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};

// ---------- Launch-side entry point ----------

const SUBCOMMANDS: &[&str] = &[
    "list", "get", "copy", "add", "edit", "delete", "restore", "tag", "view", "search", "export",
    "import", "backup", "settings", "version",
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
        respond_to_show_ping(&mut pipe, &app).await;
        return;
    }
    let (code, stdout, stderr) = run_in_process(&app, req.argv, PathBuf::from(req.cwd)).await;
    let resp = PipeResponse { code, stdout, stderr };
    if let Ok(bytes) = serde_json::to_vec(&resp) {
        let _ = write_frame(&mut pipe, &bytes).await;
    }
}

// A second GUI launch's way of proving this instance is not just accepting
// connections but genuinely alive: the response only goes out once show_main
// has actually run on the main thread, not as soon as the closure is queued
// (queuing always succeeds even when the main thread itself is wedged -- see
// the tray-window comment above on the same hazard).
async fn respond_to_show_ping(pipe: &mut tokio::net::windows::named_pipe::NamedPipeServer, app: &AppHandle) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let app2 = app.clone();
    if app
        .run_on_main_thread(move || {
            show_main(&app2);
            let _ = tx.send(());
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
// A second launch that is not a CLI command and not a promptsaver:// link
// pings the same pipe a CLI launch uses instead of trusting the
// single-instance plugin's own unconditional forward-and-exit -- see run()'s
// own comment on why. `argv` is the launch's raw arguments (already known not
// to look like a CLI command by the time run() calls this) so a deep link
// stays on the plugin's existing path unchanged.

const SHOW_PING: &str = "\0show-ping\0"; // never a valid subcommand, path or link
const SECOND_LAUNCH_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

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

pub(crate) fn resolve_second_launch(argv: &[String]) -> LaunchDecision {
    // A deep link still needs the plugin's own forwarding (it hands the URL
    // itself to remote::handle_launch_url, which this ping knows nothing
    // about), so it is deliberately left on the existing path.
    if argv.iter().any(|a| a.starts_with("promptsaver://")) {
        return LaunchDecision::HandedOff;
    }
    let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_io().enable_time().build() else {
        return LaunchDecision::HandedOff; // could not even try -- behave as before
    };
    rt.block_on(resolve_second_launch_async())
}

async fn resolve_second_launch_async() -> LaunchDecision {
    use tokio::net::windows::named_pipe::ClientOptions;
    let name = pipe_name();
    let deadline = std::time::Instant::now() + SECOND_LAUNCH_TIMEOUT;
    let mut client = loop {
        match ClientOptions::new().open(&name) {
            Ok(c) => break c,
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                if std::time::Instant::now() >= deadline {
                    return LaunchDecision::HandedOff; // busy the whole window -- do not guess further
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            Err(_) => return LaunchDecision::Primary, // nobody is listening
        }
    };
    let req = PipeRequest { argv: vec![SHOW_PING.to_string()], cwd: String::new() };
    let Ok(bytes) = serde_json::to_vec(&req) else { return LaunchDecision::HandedOff };
    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
    let roundtrip = async {
        write_frame(&mut client, &bytes).await.ok()?;
        read_frame(&mut client, MAX_FRAME_BYTES).await.ok()
    };
    let answered = tokio::time::timeout(remaining, roundtrip).await.ok().flatten().is_some();
    if answered {
        return LaunchDecision::HandedOff;
    }
    // Connected, but never answered -- a genuinely hung instance (show_main
    // rebuilding a merely-missing window is handled entirely on the other
    // side; getting here means the main thread itself never ran the ping).
    let Some(pid) = named_pipe_server_pid(&client) else { return LaunchDecision::HandedOff };
    let (same_user, same_exe) = process_identity_check(pid);
    if should_take_over_hung_instance(false, same_user, same_exe) {
        terminate_process(pid);
        append_crash_log(&format!("second launch: took over a hung running instance (pid {pid})"));
        LaunchDecision::TookOver
    } else {
        LaunchDecision::HandedOff
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
// direct kernel32 binding, the same pattern lib.rs already uses for a few
// user32/kernel32 calls it needs without pulling in a whole extra feature.
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

fn terminate_process(pid: u32) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    unsafe {
        if let Ok(process) = OpenProcess(PROCESS_TERMINATE, false, pid) {
            let _ = TerminateProcess(process, 1);
            let _ = CloseHandle(process);
        }
    }
}

// ---------- Command grammar ----------

enum Parsed {
    Help(String),
    Run(Cmd),
}

enum Cmd {
    Version,
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
struct Args<'a> {
    items: &'a [String],
    pos: usize,
}

impl<'a> Args<'a> {
    fn new(items: &'a [String]) -> Self {
        Args { items, pos: 0 }
    }
    fn peek(&self) -> Option<&'a str> {
        self.items.get(self.pos).map(String::as_str)
    }
    fn next(&mut self) -> Option<&'a str> {
        let v = self.peek();
        if v.is_some() {
            self.pos += 1;
        }
        v
    }
    fn next_positional(&mut self) -> Option<&'a str> {
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
            a.items = Box::leak(kept.into_boxed_slice());
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
            a.items = Box::leak(kept.into_boxed_slice());
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
        text: text.to_string(),
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
    if let Some(n) = name {
        p.name = n.to_string();
    }
    if let Some(t) = text {
        p.text = t.to_string();
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
    store.prompts.push(p);
    let body = if json { "{\"ok\":true}\n".to_string() } else { String::new() };
    Outcome::ok(body).mutating_trash()
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
    do_backup_at(dir, &backups_root, keep, gfs).map_err(|e| format!("safety-snapshot-failed: {e}"))?;
    let target = dir.join("data.db");
    let staged = stage_and_validate_backup(&backup_db, dir)?;
    if scope == "all" {
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
    Ok(())
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
            if let Ok(b) = value.parse::<bool>() {
                s.ui_flags.insert(key.to_string(), b);
            } else if let Ok(n) = value.parse::<f64>() {
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
    let dir = data_dir_headless();
    let store = load_store_headless(&dir);
    acquire_data_dir_lock(&dir, data_dir_lock_enabled(&store.settings));

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

    if is_mutating(&cmd) && crate::DATA_DIR_READONLY.load(Ordering::Relaxed) {
        eprintln!("the data folder is locked by another machine right now\n");
        return 3;
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

    // Backup restore reuses the real, already-tested command end to end (safety
    // snapshot, staged swap, app restart) instead of a second reimplementation
    // that would have to fake what only a running app can really do.
    if let Cmd::BackupRestore { name, keep, scope, confirmed } = &cmd {
        if !confirmed {
            return (4, String::new(), format!("refused: {}. Pass --yes to actually do it.\n", dry_run_description(&cmd)));
        }
        let state: tauri::State<Db> = app.state();
        let keep = keep.unwrap_or_else(|| default_keep(&lock(&state).settings));
        return match crate::restore_backup(app.clone(), state, name.clone(), keep, scope.clone()).await {
            Ok(()) => (0, format!("restored backup {name}\n"), String::new()),
            Err(e) => {
                let code = if e == "missing" { 2 } else { 3 };
                (code, String::new(), format!("{e}\n"))
            }
        };
    }

    let state: tauri::State<Db> = app.state();
    let dir = data_dir(app);
    let outcome = {
        let mut store = lock(&state);
        exec(&mut store, &dir, &cmd, &cwd)
    };
    if outcome.mutated_prompts || outcome.mutated_trash || outcome.mutated_settings {
        let store = lock(&state);
        let persisted = if outcome.mutated_trash {
            crate::save_prompts_and_trash(app, &store)
        } else if outcome.mutated_prompts {
            crate::save_prompts(app, &store)
        } else {
            true
        };
        let settings_persisted = if outcome.mutated_settings { crate::save_settings(app, &store.settings) } else { true };
        drop(store);
        if !persisted || !settings_persisted {
            return (3, String::new(), "the store refused the write (locked or unavailable)\n".to_string());
        }
    }
    (outcome.code, outcome.stdout, outcome.stderr)
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
    if canon_target.starts_with(&canon_base) {
        Ok(())
    } else {
        Err("refused: the path escapes the data folder".to_string())
    }
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
        ("settings", Some("set")) => "prompt-saver settings set <key> <value> --yes\n\nWrite one setting. The value is read as a whole number/decimal, true/false, or plain text, in that order, unless the key is one of the app's own fixed settings (theme, language, autostart, alwaysOnTop).".to_string(),
        ("version", _) => "prompt-saver version\n\nPrint the app name and version.".to_string(),
        _ => top_level_help(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- the pipe server is created inside the runtime, never beside it ----

    // Pins the platform fact install_server's shape rests on: a tokio named
    // pipe registers with the runtime reactor as it is created, so creating
    // the first server instance on the plain setup thread takes the whole
    // process down before any window exists. Creating it inside the spawned
    // task is what keeps the app startable.
    #[test]
    fn creating_a_pipe_server_outside_a_runtime_panics_but_works_inside_one() {
        use tokio::net::windows::named_pipe::ServerOptions;
        let name = format!(r"\\.\pipe\prompt-saver-reactor-probe-{}", std::process::id());

        let outside = {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(|_| {})); // the panic here is the expected result
            let name = name.clone();
            let result = std::panic::catch_unwind(move || {
                let _ = ServerOptions::new().first_pipe_instance(true).create(&name);
            });
            std::panic::set_hook(previous);
            result
        };
        assert!(outside.is_err(), "a pipe server created off the runtime must panic");

        let rt = tokio::runtime::Builder::new_current_thread().enable_io().build().unwrap();
        rt.block_on(async {
            assert!(
                ServerOptions::new().first_pipe_instance(true).create(&name).is_ok(),
                "inside a runtime the same call must succeed"
            );
        });
    }

    // ---- a second launch never ends silently ----

    #[test]
    fn a_running_instance_that_answers_in_time_is_never_taken_over() {
        assert!(!should_take_over_hung_instance(true, true, true));
    }

    #[test]
    fn a_silent_running_instance_is_taken_over_after_the_timeout() {
        assert!(should_take_over_hung_instance(false, true, true));
    }

    #[test]
    fn a_different_executable_path_is_never_taken_over() {
        assert!(!should_take_over_hung_instance(false, true, false));
        assert!(!should_take_over_hung_instance(false, false, false));
    }

    #[test]
    fn a_different_user_is_never_taken_over_even_at_the_same_path() {
        assert!(!should_take_over_hung_instance(false, false, true));
    }

    // A directory under the system temp folder, unique per test, removed on
    // drop -- the CLI's own store lives in a real folder (portable/APPDATA
    // resolution is bypassed entirely by pointing db_conn_at/do_backup_at at
    // it directly), so nothing here ever touches the real Prompt Saver data.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "prompt-saver-clitest-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn mk_prompt(id: &str, name: &str, text: &str, tags: &[&str], favorite: bool) -> Prompt {
        Prompt {
            id: id.to_string(),
            name: name.to_string(),
            text: text.to_string(),
            color: String::new(),
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
            tags: tags.iter().map(|s| (*s).to_string()).collect(),
            notes: String::new(),
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
        }
    }

    fn fixture_store() -> Store {
        let mut store = Store {
            prompts: Vec::new(),
            trash: Vec::new(),
            settings: crate::Settings::default(),
            prompts_locked: false,
            written: std::cell::RefCell::new(WrittenRows::new()),
        };
        store.settings.views.push(crate::View {
            id: "v1".to_string(),
            name: "Main".to_string(),
            cols: 5,
            rows: 4,
            layouts: std::collections::HashMap::new(),
            color: String::new(),
            members: Vec::new(),
        });
        store.settings.active_view = "v1".to_string();
        store.prompts.push(mk_prompt("p1", "Alpha", "alpha text", &["work"], true));
        store.prompts.push(mk_prompt("p2", "Beta", "beta text", &[], false));
        store
    }

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    fn run_cmd(store: &mut Store, dir: &Path, items: &[&str]) -> Outcome {
        let parsed = parse(&args(items)).unwrap_or_else(|e| panic!("expected {items:?} to parse: {e}"));
        let cmd = match parsed {
            Parsed::Run(c) => c,
            Parsed::Help(_) => panic!("expected a command, got help for {items:?}"),
        };
        exec(store, dir, &cmd, Path::new("."))
    }

    fn parse_err(items: &[&str]) {
        let argv = args(items);
        assert!(parse(&argv).is_err(), "expected {items:?} to fail parsing");
    }

    // ---- parsing ----

    #[test]
    fn every_top_level_command_parses_with_its_minimum_arguments() {
        assert!(matches!(parse(&args(&["version"])), Ok(Parsed::Run(Cmd::Version))));
        assert!(matches!(parse(&args(&["list"])), Ok(Parsed::Run(Cmd::List { .. }))));
        assert!(matches!(parse(&args(&["get", "p1"])), Ok(Parsed::Run(Cmd::Get { .. }))));
        assert!(matches!(parse(&args(&["copy", "p1"])), Ok(Parsed::Run(Cmd::Copy { .. }))));
        assert!(matches!(parse(&args(&["add", "--name", "n", "--text", "t"])), Ok(Parsed::Run(Cmd::Add { .. }))));
        assert!(matches!(parse(&args(&["edit", "p1", "--name", "n"])), Ok(Parsed::Run(Cmd::Edit { .. }))));
        assert!(matches!(parse(&args(&["delete", "p1"])), Ok(Parsed::Run(Cmd::Delete { .. }))));
        assert!(matches!(parse(&args(&["restore", "p1"])), Ok(Parsed::Run(Cmd::Restore { .. }))));
        assert!(matches!(parse(&args(&["tag", "list"])), Ok(Parsed::Run(Cmd::TagList { .. }))));
        assert!(matches!(parse(&args(&["tag", "add", "p1", "x"])), Ok(Parsed::Run(Cmd::TagAdd { .. }))));
        assert!(matches!(parse(&args(&["tag", "remove", "p1", "x"])), Ok(Parsed::Run(Cmd::TagRemove { .. }))));
        assert!(matches!(parse(&args(&["view", "list"])), Ok(Parsed::Run(Cmd::ViewList { .. }))));
        assert!(matches!(parse(&args(&["view", "add", "n"])), Ok(Parsed::Run(Cmd::ViewAdd { .. }))));
        assert!(matches!(parse(&args(&["view", "remove", "v1"])), Ok(Parsed::Run(Cmd::ViewRemove { .. }))));
        assert!(matches!(parse(&args(&["search", "q"])), Ok(Parsed::Run(Cmd::Search { .. }))));
        assert!(matches!(parse(&args(&["export", "out.json"])), Ok(Parsed::Run(Cmd::Export { .. }))));
        assert!(matches!(parse(&args(&["import", "in.json"])), Ok(Parsed::Run(Cmd::Import { .. }))));
        assert!(matches!(parse(&args(&["backup", "create"])), Ok(Parsed::Run(Cmd::BackupCreate { .. }))));
        assert!(matches!(parse(&args(&["backup", "list"])), Ok(Parsed::Run(Cmd::BackupList { .. }))));
        assert!(matches!(parse(&args(&["backup", "restore", "2024-01-01_00-00-00"])), Ok(Parsed::Run(Cmd::BackupRestore { .. }))));
        assert!(matches!(parse(&args(&["settings", "get", "theme"])), Ok(Parsed::Run(Cmd::SettingsGet { .. }))));
        assert!(matches!(parse(&args(&["settings", "set", "theme", "dark"])), Ok(Parsed::Run(Cmd::SettingsSet { .. }))));
    }

    #[test]
    fn missing_required_arguments_are_a_bad_invocation() {
        parse_err(&["get"]);
        parse_err(&["copy"]);
        parse_err(&["add", "--name", "n"]); // no --text
        parse_err(&["add", "--text", "t"]); // no --name
        parse_err(&["delete"]);
        parse_err(&["restore"]);
        parse_err(&["tag"]);
        parse_err(&["tag", "add", "p1"]); // no tag name
        parse_err(&["view"]);
        parse_err(&["view", "add"]); // no name
        parse_err(&["search"]);
        parse_err(&["export"]);
        parse_err(&["import"]);
        parse_err(&["backup"]);
        parse_err(&["backup", "restore"]);
        parse_err(&["settings"]);
        parse_err(&["settings", "get"]);
        parse_err(&["settings", "set", "theme"]); // no value
        parse_err(&["nonsense"]);
        parse_err(&["get", "p1", "--extra"]); // unexpected argument
        parse_err(&["edit", "p1"]); // nothing to change
    }

    #[test]
    fn edit_rejects_favorite_and_no_favorite_together() {
        parse_err(&["edit", "p1", "--favorite", "--no-favorite"]);
    }

    #[test]
    fn ids_with_an_invalid_shape_are_rejected_before_any_lookup() {
        parse_err(&["get", "../escape"]);
        parse_err(&["get", "id with spaces"]);
        parse_err(&["get", ""]);
    }

    #[test]
    fn bare_help_never_needs_the_rest_of_the_arguments() {
        assert!(matches!(parse(&args(&["--help"])), Ok(Parsed::Help(_))));
        assert!(matches!(parse(&args(&["get", "--help"])), Ok(Parsed::Help(_))));
        assert!(matches!(parse(&args(&["tag", "--help"])), Ok(Parsed::Help(_))));
        assert!(matches!(parse(&args(&["tag", "add", "--help"])), Ok(Parsed::Help(_))));
        assert!(matches!(parse(&args(&["backup", "restore", "--help"])), Ok(Parsed::Help(_))));
    }

    // ---- list / get / copy / search: exit codes and both output forms ----

    #[test]
    fn list_prints_every_prompt_in_text_and_json() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let text = run_cmd(&mut store, &dir, &["list"]);
        assert_eq!(text.code, 0);
        assert!(text.stdout.contains("p1") && text.stdout.contains("p2"));
        let json = run_cmd(&mut store, &dir, &["list", "--json"]);
        assert_eq!(json.code, 0);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(json.stdout.trim()).expect("valid JSON array");
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn list_favorite_filters_down_to_the_starred_prompt() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let out = run_cmd(&mut store, &dir, &["list", "--favorite"]);
        assert_eq!(out.code, 0);
        assert!(out.stdout.contains("p1"));
        assert!(!out.stdout.contains("p2"));
    }

    #[test]
    fn get_returns_not_found_for_an_unknown_id() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let out = run_cmd(&mut store, &dir, &["get", "does-not-exist"]);
        assert_eq!(out.code, 2);
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
    }

    #[test]
    fn get_prints_the_full_prompt_in_text_and_json() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let text = run_cmd(&mut store, &dir, &["get", "p1"]);
        assert_eq!(text.code, 0);
        assert!(text.stdout.contains("alpha text"));
        let json = run_cmd(&mut store, &dir, &["get", "p1", "--json"]);
        assert_eq!(json.code, 0);
        let v: serde_json::Value = serde_json::from_str(json.stdout.trim()).expect("valid JSON object");
        assert_eq!(v["id"], "p1");
        assert_eq!(v["text"], "alpha text");
    }

    #[test]
    fn copy_reports_not_found_for_an_unknown_id() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let out = run_cmd(&mut store, &dir, &["copy", "does-not-exist"]);
        assert_eq!(out.code, 2);
    }

    #[test]
    fn copy_never_changes_the_store() {
        let mut store = fixture_store();
        let before = store.prompts.len();
        let dir = std::env::temp_dir();
        let _ = run_cmd(&mut store, &dir, &["copy", "p1"]);
        assert_eq!(store.prompts.len(), before);
    }

    #[test]
    fn search_matches_name_text_and_tags_case_insensitively() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let by_name = run_cmd(&mut store, &dir, &["search", "ALPHA"]);
        assert!(by_name.stdout.contains("p1") && !by_name.stdout.contains("p2"));
        let by_tag = run_cmd(&mut store, &dir, &["search", "", "--tag", "work"]);
        assert!(by_tag.stdout.contains("p1") && !by_tag.stdout.contains("p2"));
        let none = run_cmd(&mut store, &dir, &["search", "no-such-word-anywhere"]);
        assert!(none.stdout.is_empty());
    }

    // ---- mutating commands: refused without --yes, applied with it ----

    #[test]
    fn add_is_refused_without_yes_and_leaves_the_store_untouched() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let before = store.prompts.len();
        let out = run_cmd(&mut store, &dir, &["add", "--name", "New", "--text", "hi"]);
        assert_eq!(out.code, 4);
        assert_eq!(store.prompts.len(), before);
    }

    #[test]
    fn add_with_yes_appends_a_new_prompt_and_prints_its_id() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let before = store.prompts.len();
        let out = run_cmd(&mut store, &dir, &["add", "--name", "New", "--text", "hi", "--tags", "a,b", "--yes"]);
        assert_eq!(out.code, 0);
        assert_eq!(store.prompts.len(), before + 1);
        let new_id = out.stdout.trim();
        let added = store.prompts.iter().find(|p| p.id == new_id).expect("new prompt present");
        assert_eq!(added.name, "New");
        assert_eq!(added.tags, vec!["a".to_string(), "b".to_string()]);
        // JSON form reports the same id as an object field, not a bare line.
        let out2 = run_cmd(&mut store, &dir, &["add", "--name", "N2", "--text", "t2", "--yes", "--json"]);
        let v: serde_json::Value = serde_json::from_str(out2.stdout.trim()).unwrap();
        assert!(v["id"].as_str().is_some());
    }

    #[test]
    fn add_rejects_an_over_long_tag_and_changes_nothing() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let before = store.prompts.len();
        let long_tag = "x".repeat(61);
        let out = run_cmd(&mut store, &dir, &["add", "--name", "New", "--text", "hi", "--tags", &long_tag, "--yes"]);
        assert_eq!(out.code, 4);
        assert_eq!(store.prompts.len(), before);
    }

    #[test]
    fn edit_is_refused_without_yes() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let out = run_cmd(&mut store, &dir, &["edit", "p1", "--name", "Changed"]);
        assert_eq!(out.code, 4);
        assert_eq!(store.prompts[0].name, "Alpha");
    }

    #[test]
    fn edit_with_yes_changes_only_the_given_fields() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let out = run_cmd(&mut store, &dir, &["edit", "p1", "--favorite" /* already true */, "--notes", "hello", "--yes"]);
        assert_eq!(out.code, 0);
        let p = store.prompts.iter().find(|p| p.id == "p1").unwrap();
        assert_eq!(p.text, "alpha text"); // untouched
        assert_eq!(p.notes, "hello");
    }

    #[test]
    fn edit_on_an_unknown_id_is_not_found() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let out = run_cmd(&mut store, &dir, &["edit", "ghost", "--name", "x", "--yes"]);
        assert_eq!(out.code, 2);
    }

    #[test]
    fn delete_moves_a_prompt_to_the_trash_only_with_yes() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let refused = run_cmd(&mut store, &dir, &["delete", "p1"]);
        assert_eq!(refused.code, 4);
        assert_eq!(store.prompts.len(), 2);
        let ok = run_cmd(&mut store, &dir, &["delete", "p1", "--yes"]);
        assert_eq!(ok.code, 0);
        assert_eq!(store.prompts.len(), 1);
        assert_eq!(store.trash.len(), 1);
        assert_eq!(store.trash[0].id, "p1");
    }

    #[test]
    fn delete_of_an_unknown_id_is_not_found() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let out = run_cmd(&mut store, &dir, &["delete", "ghost", "--yes"]);
        assert_eq!(out.code, 2);
    }

    #[test]
    fn restore_moves_a_trashed_prompt_back_only_with_yes() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let _ = run_cmd(&mut store, &dir, &["delete", "p1", "--yes"]);
        let refused = run_cmd(&mut store, &dir, &["restore", "p1"]);
        assert_eq!(refused.code, 4);
        assert_eq!(store.trash.len(), 1);
        let ok = run_cmd(&mut store, &dir, &["restore", "p1", "--yes"]);
        assert_eq!(ok.code, 0);
        assert_eq!(store.trash.len(), 0);
        assert!(store.prompts.iter().any(|p| p.id == "p1"));
    }

    #[test]
    fn tag_add_and_remove_round_trip_only_with_yes() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let refused = run_cmd(&mut store, &dir, &["tag", "add", "p2", "urgent"]);
        assert_eq!(refused.code, 4);
        assert!(store.prompts[1].tags.is_empty());
        let ok = run_cmd(&mut store, &dir, &["tag", "add", "p2", "urgent", "--yes"]);
        assert_eq!(ok.code, 0);
        assert_eq!(store.prompts[1].tags, vec!["urgent".to_string()]);
        let missing = run_cmd(&mut store, &dir, &["tag", "remove", "p2", "no-such-tag", "--yes"]);
        assert_eq!(missing.code, 2);
        let removed = run_cmd(&mut store, &dir, &["tag", "remove", "p2", "urgent", "--yes"]);
        assert_eq!(removed.code, 0);
        assert!(store.prompts[1].tags.is_empty());
    }

    #[test]
    fn tag_list_counts_every_distinct_tag() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let out = run_cmd(&mut store, &dir, &["tag", "list", "--json"]);
        assert_eq!(out.code, 0);
        let rows: Vec<serde_json::Value> = serde_json::from_str(out.stdout.trim()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], "work");
        assert_eq!(rows[0]["count"], 1);
    }

    #[test]
    fn view_add_and_remove_round_trip_only_with_yes() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let refused = run_cmd(&mut store, &dir, &["view", "add", "Second"]);
        assert_eq!(refused.code, 4);
        assert_eq!(store.settings.views.len(), 1);
        let ok = run_cmd(&mut store, &dir, &["view", "add", "Second", "--yes"]);
        assert_eq!(ok.code, 0);
        assert_eq!(store.settings.views.len(), 2);
        let new_id = ok.stdout.trim().to_string();
        let remove_last = run_cmd(&mut store, &dir, &["view", "remove", "v1", "--yes"]);
        assert_eq!(remove_last.code, 0);
        assert_eq!(store.settings.views.len(), 1);
        // The one remaining view can never be removed.
        let refused_last = run_cmd(&mut store, &dir, &["view", "remove", &new_id, "--yes"]);
        assert_eq!(refused_last.code, 4);
        assert_eq!(store.settings.views.len(), 1);
    }

    #[test]
    fn view_remove_of_an_unknown_id_is_not_found() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let out = run_cmd(&mut store, &dir, &["view", "remove", "ghost", "--yes"]);
        assert_eq!(out.code, 2);
    }

    // ---- settings ----

    #[test]
    fn settings_get_reports_not_found_for_an_unknown_key() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let out = run_cmd(&mut store, &dir, &["settings", "get", "not-a-real-key"]);
        assert_eq!(out.code, 2);
    }

    #[test]
    fn settings_round_trips_a_well_known_and_a_generic_key_only_with_yes() {
        let mut store = fixture_store();
        let dir = std::env::temp_dir();
        let refused = run_cmd(&mut store, &dir, &["settings", "set", "theme", "dark"]);
        assert_eq!(refused.code, 4);
        assert_ne!(store.settings.theme, "dark");
        let ok = run_cmd(&mut store, &dir, &["settings", "set", "theme", "dark", "--yes"]);
        assert_eq!(ok.code, 0);
        assert_eq!(store.settings.theme, "dark");
        let got = run_cmd(&mut store, &dir, &["settings", "get", "theme"]);
        assert_eq!(got.stdout.trim(), "dark");

        let _ = run_cmd(&mut store, &dir, &["settings", "set", "myFlag", "true", "--yes"]);
        assert_eq!(store.settings.ui_flags.get("myFlag"), Some(&true));
        let _ = run_cmd(&mut store, &dir, &["settings", "set", "myNumber", "12.5", "--yes"]);
        assert_eq!(store.settings.ui_values.get("myNumber"), Some(&12.5));
        let _ = run_cmd(&mut store, &dir, &["settings", "set", "myText", "hello", "--yes"]);
        assert_eq!(store.settings.ui_texts.get("myText").map(String::as_str), Some("hello"));
    }

    // ---- export / import: real files, since export writes and import reads one ----

    #[test]
    fn export_writes_a_file_and_refuses_to_overwrite_without_yes() {
        let tmp = TempDir::new("export");
        let out_path = tmp.path().join("out.json");
        let out_str = out_path.to_string_lossy().to_string();
        let mut store = fixture_store();
        let export_cmd = |confirmed: bool| Cmd::Export { path: out_str.clone(), format: "json".to_string(), ids: None, confirmed };

        let first = exec(&mut store, tmp.path(), &export_cmd(false), Path::new("."));
        assert_eq!(first.code, 0, "a new file needs no --yes: {}", first.stderr);
        assert!(out_path.exists());

        let refused = exec(&mut store, tmp.path(), &export_cmd(false), Path::new("."));
        assert_eq!(refused.code, 4, "an existing file must not be silently overwritten");

        let overwritten = exec(&mut store, tmp.path(), &export_cmd(true), Path::new("."));
        assert_eq!(overwritten.code, 0);
    }

    #[test]
    fn export_relative_paths_resolve_against_the_callers_cwd_not_this_process() {
        let tmp = TempDir::new("export-cwd");
        let mut store = fixture_store();
        let out = exec(&mut store, tmp.path(), &Cmd::Export { path: "rel.json".to_string(), format: "json".to_string(), ids: None, confirmed: false }, tmp.path());
        assert_eq!(out.code, 0);
        assert!(tmp.path().join("rel.json").exists());
    }

    #[test]
    fn import_requires_yes_and_appends_exported_prompts() {
        let tmp = TempDir::new("import");
        let mut store = fixture_store();
        let export_path = tmp.path().join("dump.json");
        let e = exec(&mut store, tmp.path(), &Cmd::Export { path: export_path.to_string_lossy().to_string(), format: "json".to_string(), ids: None, confirmed: false }, Path::new("."));
        assert_eq!(e.code, 0);

        let mut fresh = Store { prompts: Vec::new(), trash: Vec::new(), settings: crate::Settings::default(), prompts_locked: false, written: std::cell::RefCell::new(WrittenRows::new()) };
        let refused = run_cmd(&mut fresh, tmp.path(), &["import", export_path.to_str().unwrap()]);
        assert_eq!(refused.code, 4);
        assert_eq!(fresh.prompts.len(), 0);
        let ok = run_cmd(&mut fresh, tmp.path(), &["import", export_path.to_str().unwrap(), "--yes"]);
        assert_eq!(ok.code, 0);
        assert_eq!(fresh.prompts.len(), 2);
    }

    #[test]
    fn import_of_a_missing_file_fails_cleanly() {
        let tmp = TempDir::new("import-missing");
        let mut store = fixture_store();
        let out = run_cmd(&mut store, tmp.path(), &["import", "does-not-exist.json", "--yes"]);
        assert_ne!(out.code, 0);
    }

    // ---- backups: real files under a temp "data dir" ----

    #[test]
    fn backup_create_then_list_finds_the_new_snapshot() {
        let tmp = TempDir::new("backup");
        // do_backup_at requires a real data.db to snapshot.
        let mut conn = db_conn_at(tmp.path()).expect("open store");
        let mut written = WrittenRows::new();
        db_write_prompts(&mut conn, &[], &mut written).ok();
        let store = fixture_store();
        let created = cmd_backup_create(&store, tmp.path(), Some(1), true);
        assert_eq!(created.code, 0, "stderr: {}", created.stderr);
        let listed = cmd_backup_list(tmp.path(), true);
        assert_eq!(listed.code, 0);
        let rows: Vec<serde_json::Value> = serde_json::from_str(listed.stdout.trim()).unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn backup_restore_is_refused_without_yes_at_the_launch_dispatch_level() {
        // The refusal for backup restore happens before either caller (live or
        // standalone) touches a single file -- checked directly on the parsed
        // command here, since exec() itself is never reached for this one.
        let cmd = match parse(&args(&["backup", "restore", "2024-01-01_00-00-00"])).unwrap() {
            Parsed::Run(c) => c,
            Parsed::Help(_) => unreachable!(),
        };
        assert!(!confirmed_of(&cmd));
        assert!(is_mutating(&cmd));
    }

    // ---- standalone round trip: a real store on disk, via run_standalone ----

    // Exercises exactly what run_standalone() does around exec() -- load,
    // run, persist by writing straight to the database -- against an explicit
    // temp directory instead of run_standalone() itself, which resolves its
    // folder from APPDATA/portable detection. Those touch process-global state
    // (env vars, the exe's own folder) that a parallel test run must not poke,
    // so the directory is passed in here rather than faked through them.
    // db_conn_at() only ever runs the schema batch once per PROCESS (a real
    // standalone launch never touches more than one directory, so that cache is
    // exactly right there) -- but this test binary calls it against a fresh
    // TempDir per test, and whichever directory the cache warmed up on leaves
    // every other one without its tables. Creating them here, once per
    // directory before its first real use, sidesteps the cache instead of
    // reaching into it.
    fn ensure_test_schema(dir: &Path) {
        let conn = rusqlite::Connection::open(dir.join("data.db")).expect("open db for test schema");
        assert!(crate::db_conn_init_schema(&conn), "schema init failed for test directory");
    }

    fn standalone_roundtrip(dir: &Path, items: &[&str]) -> (i32, String) {
        ensure_test_schema(dir);
        let store = load_store_headless(dir);
        let parsed = parse(&args(items)).unwrap();
        let cmd = match parsed {
            Parsed::Run(c) => c,
            Parsed::Help(t) => return (0, t),
        };
        let mut store = store;
        let outcome = exec(&mut store, dir, &cmd, Path::new("."));
        if outcome.mutated_prompts || outcome.mutated_trash || outcome.mutated_settings {
            let mut conn = db_conn_at(dir).expect("open store");
            // store.written, not a fresh map -- see the matching comment in
            // run_standalone(). A fresh map here silently turned every delete
            // into a no-op on disk, caught by
            // standalone_delete_then_restore_round_trips_through_the_trash_table.
            if outcome.mutated_trash {
                db_write_prompts_and_trash(&mut conn, &store.prompts, &store.trash, &mut store.written.borrow_mut()).unwrap();
            } else if outcome.mutated_prompts {
                db_write_prompts(&mut conn, &store.prompts, &mut store.written.borrow_mut()).unwrap();
            }
            if outcome.mutated_settings {
                db_write_settings(&conn, &store.settings).unwrap();
            }
        }
        (outcome.code, outcome.stdout)
    }

    #[test]
    fn standalone_add_persists_to_disk_for_the_next_load() {
        let tmp = TempDir::new("standalone-add");
        let (code, stdout) = standalone_roundtrip(tmp.path(), &["add", "--name", "Solo", "--text", "hi", "--yes"]);
        assert_eq!(code, 0);
        let new_id = stdout.trim().to_string();

        // A fresh load_store_headless() call re-opens the same data.db --
        // proving the write really reached disk, not just this run's Store.
        let reloaded = load_store_headless(tmp.path());
        assert!(reloaded.prompts.iter().any(|p| p.id == new_id && p.name == "Solo"));
    }

    #[test]
    fn standalone_delete_then_restore_round_trips_through_the_trash_table() {
        let tmp = TempDir::new("standalone-delete");
        let (add_code, stdout) = standalone_roundtrip(tmp.path(), &["add", "--name", "Solo", "--text", "hi", "--yes"]);
        assert_eq!(add_code, 0);
        let id = stdout.trim().to_string();

        let (del_code, _) = standalone_roundtrip(tmp.path(), &["delete", &id, "--yes"]);
        assert_eq!(del_code, 0);
        let after_delete = load_store_headless(tmp.path());
        assert!(!after_delete.prompts.iter().any(|p| p.id == id));
        assert!(after_delete.trash.iter().any(|p| p.id == id));

        let (restore_code, _) = standalone_roundtrip(tmp.path(), &["restore", &id, "--yes"]);
        assert_eq!(restore_code, 0);
        let after_restore = load_store_headless(tmp.path());
        assert!(after_restore.prompts.iter().any(|p| p.id == id));
        assert!(!after_restore.trash.iter().any(|p| p.id == id));
    }

    #[test]
    fn run_standalone_reports_bad_invocation_as_exit_one() {
        let code = run_standalone(&args(&["not-a-command"]), Path::new("."));
        assert_eq!(code, 1);
    }

    #[test]
    fn run_standalone_prints_help_and_exits_zero() {
        let code = run_standalone(&args(&["--help"]), Path::new("."));
        assert_eq!(code, 0);
    }

    // ---- pipe framing ----

    #[test]
    fn frame_round_trips_through_an_in_memory_duplex_pipe() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_io().enable_time().build().unwrap();
        rt.block_on(async {
            let (mut a, mut b) = tokio::io::duplex(4096);
            let payload = b"{\"argv\":[\"list\"],\"cwd\":\"C:/\"}".to_vec();
            write_frame(&mut a, &payload).await.unwrap();
            let got = read_frame(&mut b, MAX_FRAME_BYTES).await.unwrap();
            assert_eq!(got, payload);
        });
    }

    #[test]
    fn oversized_frame_is_rejected_instead_of_allocated() {
        use tokio::io::AsyncWriteExt;
        let rt = tokio::runtime::Builder::new_current_thread().enable_io().enable_time().build().unwrap();
        rt.block_on(async {
            let (mut a, mut b) = tokio::io::duplex(64);
            // Only the length header is written, never the claimed 100-byte body --
            // proving read_frame rejects on the header alone instead of waiting on
            // (or allocating for) bytes that were never sent.
            a.write_all(&100u32.to_le_bytes()).await.unwrap();
            let err = read_frame(&mut b, 10).await.unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        });
    }

    // ---- remote-caller path confinement ----

    #[test]
    fn export_inside_the_data_folder_is_allowed() {
        let dir = TempDir::new("confine-ok");
        let argv = args(&["export", "out.json", "--yes"]);
        assert!(confine_path_arg(&argv, dir.path()).is_ok());
    }

    #[test]
    fn export_escaping_the_data_folder_via_dotdot_is_refused() {
        let dir = TempDir::new("confine-escape");
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        let argv = args(&["export", "../outside.json", "--yes"]);
        let inner = dir.path().join("sub");
        assert!(confine_path_arg(&argv, &inner).is_err());
    }

    #[test]
    fn export_with_an_absolute_path_outside_the_data_folder_is_refused() {
        let dir = TempDir::new("confine-abs");
        let outside = TempDir::new("confine-abs-outside");
        let abs = outside.path().join("out.json");
        let argv = args(&["export", abs.to_str().unwrap(), "--yes"]);
        assert!(confine_path_arg(&argv, dir.path()).is_err());
    }

    #[test]
    fn import_from_inside_the_data_folder_is_allowed() {
        let dir = TempDir::new("confine-import-ok");
        let file = dir.path().join("in.json");
        std::fs::write(&file, "[]").unwrap();
        let argv = args(&["import", "in.json", "--yes"]);
        assert!(confine_path_arg(&argv, dir.path()).is_ok());
    }

    #[test]
    fn import_escaping_the_data_folder_is_refused() {
        let dir = TempDir::new("confine-import-escape");
        let outside = TempDir::new("confine-import-outside");
        let file = outside.path().join("in.json");
        std::fs::write(&file, "[]").unwrap();
        let argv = args(&["import", file.to_str().unwrap(), "--yes"]);
        assert!(confine_path_arg(&argv, dir.path()).is_err());
    }

    #[test]
    fn a_non_path_command_is_never_confined() {
        let dir = TempDir::new("confine-noop");
        assert!(confine_path_arg(&args(&["list"]), dir.path()).is_ok());
        assert!(confine_path_arg(&args(&["get", "p1"]), dir.path()).is_ok());
    }
}
