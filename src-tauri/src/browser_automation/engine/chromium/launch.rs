//! Finding a Chromium binary and starting it on a CDP pipe.
//!
//! CDP runs only over `--remote-debugging-pipe`: the child reads requests
//! on fd 3 and writes replies on fd 4. No debugging port is ever opened, so
//! no other local process can attach to the browser.

use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// The operator's override: the browser binary to run.
pub const PATH_VAR: &str = "TROUPE_CHROMIUM_PATH";

/// Install paths of browsers that update themselves, checked first.
#[cfg(target_os = "macos")]
const INSTALLED: &[&str] = &[
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
];
#[cfg(not(target_os = "macos"))]
const INSTALLED: &[&str] = &["/opt/google/chrome/chrome", "/opt/microsoft/msedge/msedge"];

/// Names looked up on `PATH`: Chrome and Edge first, then a distribution
/// Chromium.
const ON_PATH: &[&str] = &[
    "google-chrome",
    "google-chrome-stable",
    "microsoft-edge",
    "microsoft-edge-stable",
    "msedge",
    "chromium",
    "chromium-browser",
];

/// The browser binary to run: the override, an installed Chrome or Edge,
/// or a Chromium on `PATH`. `None` when this host has none.
pub fn find() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(PATH_VAR) {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    INSTALLED
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .or_else(|| ON_PATH.iter().find_map(|name| on_path(name)))
}

fn on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

/// One browser process to start.
#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub binary: PathBuf,
    /// The trusted state directory under which profile directories live.
    /// Chain validation starts here and never walks above it.
    pub state_dir: PathBuf,
    /// The persistent user-data dir of this process's profile.
    pub profile_dir: PathBuf,
    /// Run without Chromium's sandbox. Only for an operator who runs the
    /// server as root, and for tests on throwaway profiles.
    pub no_sandbox: bool,
    /// The loopback port of the perimeter proxy every request goes
    /// through, loopback included.
    pub proxy_port: u16,
}

impl LaunchSpec {
    /// The fixed argv. `--disable-gpu` keeps a host with no GPU adapter off
    /// a software-rendering GPU process.
    pub fn args(&self) -> Vec<OsString> {
        let mut profile = OsString::from("--user-data-dir=");
        profile.push(&self.profile_dir);
        let mut args: Vec<OsString> = [
            "--headless=new",
            "--remote-debugging-pipe",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-sync",
            "--disable-extensions",
            "--disable-default-apps",
            "--mute-audio",
            "--password-store=basic",
            "--use-mock-keychain",
            "--disable-gpu",
            "--disable-features=Translate,MediaRouter,OptimizationHints",
            "--deny-permission-prompts",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        args.push(profile);
        args.push(format!("--proxy-server=http://127.0.0.1:{}", self.proxy_port).into());
        // Chromium sends loopback hosts past a proxy unless told not to.
        args.push("--proxy-bypass-list=<-loopback>".into());
        // WebRTC would otherwise open UDP around the proxy.
        args.push("--force-webrtc-ip-handling-policy=disable_non_proxied_udp".into());
        if self.no_sandbox {
            args.push("--no-sandbox".into());
        }
        args
    }
}

/// A started browser and the host's ends of its CDP pipe.
pub struct Launched {
    pub child: Child,
    /// What Chromium writes (its fd 4).
    pub read: File,
    /// What Chromium reads (its fd 3).
    pub write: File,
}

/// Why a browser did not start.
#[derive(Debug)]
pub enum LaunchError {
    /// Chromium cannot keep its sandbox as root, and the operator did not
    /// opt out of it.
    RootNeedsNoSandbox,
    Io(io::Error),
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RootNeedsNoSandbox => f.write_str(
                "Chromium cannot run its sandbox as root; run the server as another user or pass --browser-host-no-sandbox",
            ),
            Self::Io(e) => write!(f, "could not start the browser: {e}"),
        }
    }
}

/// Start the browser with its CDP pipe on fds 3 and 4.
pub fn spawn(spec: &LaunchSpec) -> Result<Launched, LaunchError> {
    // SAFETY: geteuid has no preconditions.
    if !spec.no_sandbox && unsafe { libc::geteuid() } == 0 {
        return Err(LaunchError::RootNeedsNoSandbox);
    }
    prepare_profile(&spec.state_dir, &spec.profile_dir).map_err(LaunchError::Io)?;
    let (child_reads, host_writes) = pipe().map_err(LaunchError::Io)?;
    let (host_reads, child_writes) = pipe().map_err(LaunchError::Io)?;
    let (r, w) = (child_reads.as_raw_fd(), child_writes.as_raw_fd());
    let mut command = Command::new(&spec.binary);
    command
        .args(spec.args())
        .current_dir(spec.profile_dir.parent().unwrap_or(Path::new("/")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: the closure runs in the forked child before exec and calls
    // only async-signal-safe functions (fcntl, dup2, close).
    unsafe {
        command.pre_exec(move || place_pipe_fds(r, w));
    }
    let child = command.spawn().map_err(LaunchError::Io)?;
    drop(child_reads);
    drop(child_writes);
    Ok(Launched {
        child,
        read: File::from(host_reads),
        write: File::from(host_writes),
    })
}

/// Put `read` on fd 3 and `write` on fd 4 in the child. Both are first
/// copied above 4, so one cannot overwrite the other while it is placed.
///
/// SAFETY (each call below): plain descriptor syscalls on descriptors the
/// child inherited.
fn place_pipe_fds(read: RawFd, write: RawFd) -> io::Result<()> {
    let high_read = check(unsafe { libc::fcntl(read, libc::F_DUPFD, 5) })?;
    let high_write = check(unsafe { libc::fcntl(write, libc::F_DUPFD, 5) })?;
    check(unsafe { libc::dup2(high_read, 3) })?;
    check(unsafe { libc::dup2(high_write, 4) })?;
    unsafe {
        libc::close(high_read);
        libc::close(high_write);
    }
    Ok(())
}

fn check(rc: libc::c_int) -> io::Result<libc::c_int> {
    if rc < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(rc)
    }
}

/// A pipe whose ends close on exec: only the copies placed on fds 3 and 4
/// reach the browser.
fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds: [libc::c_int; 2] = [0; 2];
    // SAFETY: `fds` has room for the two descriptors pipe writes.
    check(unsafe { libc::pipe(fds.as_mut_ptr()) })?;
    // SAFETY: pipe returned two fresh descriptors this function now owns.
    let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    for fd in [&read, &write] {
        // SAFETY: a valid descriptor owned above.
        check(unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) })?;
    }
    Ok((read, write))
}

/// Private browser state must never be shared with another local user.
///
/// Validates `base` (the trusted state directory) and every component below it
/// down to `path`, never walking above `base`. Refuses symlinks, foreign
/// ownership, group- or world-writable directories, non-private profile leaves,
/// and paths outside `base`.
fn prepare_profile(base: &Path, path: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    if !base.is_absolute() || !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "browser state and profile directories must be absolute paths",
        ));
    }
    let relative = path.strip_prefix(base).map_err(|_| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "browser profile must live under the state directory",
        )
    })?;
    let components: Vec<_> = relative.components().collect();
    if components.is_empty()
        || components
            .iter()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "invalid profile path",
        ));
    }
    let euid = unsafe { libc::geteuid() };
    let validate = |directory: &Path, forbidden_mode_bits: u32| -> io::Result<()> {
        let metadata = std::fs::symlink_metadata(directory)?;
        if !metadata.is_dir()
            || metadata.uid() != euid
            || metadata.mode() & forbidden_mode_bits != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "browser profile chain must contain only user-owned non-writable directories",
            ));
        }
        Ok(())
    };
    let mut current = base.to_path_buf();
    validate(&current, 0o022)?;
    let last_index = components.len() - 1;
    for (index, component) in components.into_iter().enumerate() {
        current.push(component);
        match std::fs::DirBuilder::new().mode(0o700).create(&current) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        let forbidden = if index == last_index { 0o077 } else { 0o022 };
        validate(&current, forbidden)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "launch_security_tests.rs"]
mod security_tests;
