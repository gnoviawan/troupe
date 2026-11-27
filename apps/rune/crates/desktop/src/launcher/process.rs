//! The server process: finding the binary, starting it on loopback with its token in the
//! environment, probing `/health`, stopping it, and making sure it dies with the app.
//!
//! A started server runs in its own process group, so a Ctrl+C in the app's terminal reaches
//! only the app. On Linux and macOS a detached `sh` watchdog polls the app's pid every second
//! and stops the server (SIGTERM, then SIGKILL after 10 s) once the app is gone without
//! stopping it (SIGKILL, a crash). On Windows the server joins a Job Object that kills it when
//! the app's last handle to the job closes, which the OS does when the app dies.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::paths::Paths;

const BIN_NAMES: [&str; 2] = ["troupe-server", "termul-server"];
/// How long a start may take to answer `/health`.
pub const READY_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a stop waits after SIGTERM before it kills.
const STOP_GRACE: Duration = Duration::from_secs(5);

/// The binary: `TROUPE_SERVER_BIN`, else `troupe-server` (or the old `termul-server`) beside
/// this executable, else the first on `PATH`.
pub fn server_binary() -> Option<PathBuf> {
    if let Some(bin) = std::env::var_os("TROUPE_SERVER_BIN").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(bin));
    }
    let exe = |name: &str| format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let beside = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    let path = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
    BIN_NAMES
        .iter()
        .flat_map(|name| beside.iter().chain(path.iter()).map(move |dir| dir.join(exe(name))))
        .find(|candidate| candidate.is_file())
}

/// 32 random bytes as hex.
pub fn new_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("no randomness for the server token: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// `preferred` when it is free on loopback, else a free port the OS picks. Free means nothing
/// answers there and a bind works: std binds with `SO_REUSEADDR`, which on Linux lets a bind
/// succeed beside another listener.
pub fn pick_port(preferred: Option<u16>) -> std::io::Result<u16> {
    let answers = |port: u16| {
        TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_millis(200)).is_ok()
    };
    if let Some(port) = preferred.filter(|p| *p != 0)
        && !answers(port)
        && TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok()
    {
        return Ok(port);
    }
    Ok(TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?.local_addr()?.port())
}

/// `GET /health` answers 200 with `"status":"ok"`.
pub fn healthy(port: u16) -> bool {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let request = format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut reply = String::new();
    let _ = stream.take(64 * 1024).read_to_string(&mut reply);
    reply.starts_with("HTTP/1.1 200") && reply.contains("\"status\":\"ok\"")
}

/// The server's flags for the desktop data dir: the Tauri app's stores (`lock_desktop_data`).
pub fn server_args(paths: &Paths, port: u16) -> Vec<String> {
    let data = &paths.data_dir;
    let at = |p: PathBuf| p.to_string_lossy().into_owned();
    // TODO(R10b): add `--desktop` once troupe-server has the desktop launch mode (legacy
    // imports, MigrationManager, the SSH manager, the keychain self-test), and `--parent-pid`
    // so the server can watch the app itself.
    vec![
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "--state-dir".into(),
        at(data.clone()),
        "--sessions-dir".into(),
        at(data.join("acp-sessions")),
        "--projects-file".into(),
        at(data.join("projects.json")),
        "--store-file".into(),
        at(data.join("store.json")),
    ]
}

/// A server this app started.
pub struct Server {
    pub child: Child,
    #[cfg(windows)]
    _job: Option<windows::Job>,
}

impl Server {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// The exit status, once it has exited.
    pub fn exited(&mut self) -> Option<String> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(describe(status)),
            Ok(None) => None,
            Err(e) => Some(format!("lost track of the server: {e}")),
        }
    }

    /// SIGTERM, up to 5 s for it to go, then a kill. Windows kills at once.
    pub fn stop(&mut self) {
        if self.exited().is_some() {
            return;
        }
        #[cfg(unix)]
        {
            let pid = self.child.id().to_string();
            let _ = Command::new("kill").args(["-TERM", &pid]).status();
            let until = Instant::now() + STOP_GRACE;
            while Instant::now() < until {
                if self.exited().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            log::warn!("[launcher] server {pid} ignored SIGTERM for 5 s; killing it");
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn describe(status: std::process::ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("signal {signal}");
        }
    }
    match status.code() {
        Some(code) => format!("exit code {code}"),
        None => status.to_string(),
    }
}

/// Starts the server with `token` in its environment, its output in `server.log`, and the
/// watchdog or job that stops it with the app unless `keep_alive`.
pub fn spawn(bin: &Path, paths: &Paths, port: u16, token: &str, keep_alive: bool) -> Result<Server, String> {
    let log = server_log(&paths.server_log()).map_err(|e| format!("cannot open the server log: {e}"))?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let mut command = Command::new(bin);
    command
        .args(server_args(paths, port))
        .env("TROUPE_WEB_AUTH_TOKEN", token)
        .env_remove("TERMUL_WEB_AUTH_TOKEN")
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(err);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    let child = command.spawn().map_err(|e| format!("cannot start {}: {e}", bin.display()))?;
    let pid = child.id();
    log::info!("[launcher] spawned {} pid {pid} on 127.0.0.1:{port}", bin.display());
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut server = Server {
        child,
        #[cfg(windows)]
        _job: None,
    };
    if !keep_alive {
        #[cfg(unix)]
        watchdog(std::process::id(), pid);
        #[cfg(windows)]
        {
            server._job = windows::Job::kill_on_close(&server.child)
                .inspect_err(|e| log::warn!("[launcher] no job object for the server: {e}"))
                .ok();
        }
    }
    Ok(server)
}

/// Waits for `/health` on `port` while `server` is alive.
pub fn wait_ready(server: &mut Server, port: u16) -> Result<Duration, String> {
    let started = Instant::now();
    while started.elapsed() < READY_TIMEOUT {
        if let Some(status) = server.exited() {
            return Err(format!("the server exited before it was ready ({status}); see server.log"));
        }
        if healthy(port) {
            return Ok(started.elapsed());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    server.stop();
    Err(format!("the server did not answer /health within {} s", READY_TIMEOUT.as_secs()))
}

/// Opens `server.log` fresh, keeping the last run's as `server.log.prev`.
fn server_log(path: &Path) -> std::io::Result<File> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    if path.exists() {
        let _ = fs::rename(path, path.with_extension("log.prev"));
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// The detached watchdog: an `sh` loop reparented to init, so it outlives a killed app.
#[cfg(unix)]
pub(crate) fn watchdog(app: u32, server: u32) {
    const SCRIPT: &str = r#"(
  while kill -0 "$1" 2>/dev/null && kill -0 "$2" 2>/dev/null; do sleep 1; done
  kill -0 "$2" 2>/dev/null || exit 0
  kill -TERM "$2" 2>/dev/null
  i=0
  while [ "$i" -lt 10 ] && kill -0 "$2" 2>/dev/null; do sleep 1; i=$((i + 1)); done
  kill -KILL "$2" 2>/dev/null
) </dev/null >/dev/null 2>&1 &"#;
    let started = Command::new("/bin/sh")
        .args(["-c", SCRIPT, "troupe-watchdog", &app.to_string(), &server.to_string()])
        .stdin(Stdio::null())
        .status();
    match started {
        Ok(status) if status.success() => log::info!("[launcher] watchdog armed for server {server}"),
        Ok(status) => log::warn!("[launcher] watchdog did not start ({status})"),
        Err(e) => log::warn!("[launcher] watchdog did not start: {e}"),
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows {
    //! A Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`: the app holds its only handle,
    //! so the OS kills the server when the app exits for any reason.
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
    };

    pub struct Job(HANDLE);

    // The handle is an owned kernel object; the job is only closed in `Drop`.
    unsafe impl Send for Job {}

    impl Job {
        pub fn kill_on_close(child: &Child) -> Result<Job, String> {
            // SAFETY: null attributes and name create an anonymous job this process owns.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let job = Job(handle);
            // SAFETY: a zeroed limit struct is valid; only the flag below is set.
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // SAFETY: `info` lives across the call and the size matches its type.
            let set = unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    std::ptr::from_ref(&info).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if set == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            // SAFETY: the child's process handle is valid while `child` lives.
            let assigned = unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle() as HANDLE) };
            if assigned == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            Ok(job)
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: the handle came from CreateJobObjectW and is closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}
