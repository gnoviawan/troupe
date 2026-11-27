//! `--parent-pid`: stop the server when the process that started it exits.
//!
//! The Rune desktop app starts `troupe-server` and passes its own pid. When
//! the app dies (crash, kill, logout) nothing else stops the server, so it
//! watches the pid and asks for the same graceful shutdown SIGTERM starts:
//! the listener drains, agents are stopped and their sessions closed.

use tracing::{info, warn};

/// How often the parent is checked.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

/// Resolves once `pid` no longer runs. On Unix, when `pid` is this process's
/// parent at the start, the parent is also gone once this process is
/// re-parented, which covers a pid reused by an unrelated process.
pub async fn parent_exited(pid: u32) {
    #[cfg(unix)]
    unix::exited(pid, POLL_INTERVAL).await;
    #[cfg(windows)]
    windows::exited(pid, POLL_INTERVAL).await;
}

/// Watch `pid` and request a graceful shutdown when it exits.
pub fn spawn(pid: u32) {
    info!(
        target: "termul::parent_watch",
        parent_pid = pid,
        "stopping when the parent process exits"
    );
    tokio::spawn(async move {
        parent_exited(pid).await;
        warn!(
            target: "termul::parent_watch",
            parent_pid = pid,
            "parent process exited; shutting down"
        );
        crate::web::request_shutdown();
    });
}

#[cfg(unix)]
mod unix {
    use std::time::Duration;

    fn running(pid: u32) -> bool {
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return false;
        };
        // SAFETY: signal 0 only checks that the pid exists and may be
        // signalled; it sends nothing.
        if unsafe { libc::kill(pid, 0) } == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    fn parent() -> u32 {
        // SAFETY: getppid has no preconditions and cannot fail.
        u32::try_from(unsafe { libc::getppid() }).unwrap_or(0)
    }

    pub(super) async fn exited(pid: u32, interval: Duration) {
        let is_parent = parent() == pid;
        loop {
            if !running(pid) || (is_parent && parent() != pid) {
                return;
            }
            tokio::time::sleep(interval).await;
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::time::Duration;

    use tracing::warn;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::OpenProcess;
    use winapi::um::synchapi::WaitForSingleObject;
    use winapi::um::winbase::WAIT_OBJECT_0;
    use winapi::um::winnt::{HANDLE, SYNCHRONIZE};

    /// An owned process handle. While it is open the process object stays
    /// alive, so the pid cannot be reused by another process.
    struct Process(HANDLE);

    // SAFETY: a process handle is a kernel object reference, valid from any
    // thread.
    unsafe impl Send for Process {}

    impl Drop for Process {
        fn drop(&mut self) {
            // SAFETY: `self.0` is the non-null handle `OpenProcess` returned,
            // closed exactly once here.
            unsafe { CloseHandle(self.0) };
        }
    }

    pub(super) async fn exited(pid: u32, interval: Duration) {
        // SAFETY: OpenProcess returns an owned handle or null.
        let handle = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            warn!(
                target: "termul::parent_watch",
                parent_pid = pid,
                "cannot open the parent process; treating it as exited"
            );
            return;
        }
        let process = Process(handle);
        loop {
            // SAFETY: a zero timeout only polls the open handle.
            if unsafe { WaitForSingleObject(process.0, 0) } == WAIT_OBJECT_0 {
                return;
            }
            tokio::time::sleep(interval).await;
        }
    }
}
