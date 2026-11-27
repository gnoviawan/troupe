//! Who holds the data dir. Every Troupe host takes an exclusive lock on
//! `<data dir>/.termul.lock` before it opens a store (`src-tauri/src/data_lock.rs`, AD-14) and
//! writes its pid on the first line (not on Windows). The Tauri app holds it while it runs, and
//! so does a `troupe-server` this launcher started. The launcher probes the lock before it
//! spawns a server, so a running Tauri app gets a message that names it instead of a server
//! that dies on start.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::Read;
use std::path::{Path, PathBuf};

/// The sidecar every host locks in a directory store (`DIR_LOCK_FILE_NAME`).
pub const LOCK_FILE: &str = ".termul.lock";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holder {
    pub lock_file: PathBuf,
    /// The pid the holder recorded, when it did.
    pub pid: Option<u32>,
}

impl Holder {
    /// The refusal the window shows, naming the Tauri app as the usual holder.
    pub fn message(&self, data_dir: &Path) -> String {
        let who = match self.pid {
            Some(pid) => format!("Troupe (the desktop app, pid {pid})"),
            None => "Troupe (the desktop app)".to_owned(),
        };
        format!(
            "{who} is using {}. Quit it, then open Troupe Preview again. \
             The lock is {}; a standalone troupe-server on the same folder holds it too.",
            data_dir.display(),
            self.lock_file.display()
        )
    }
}

/// The current holder of `data_dir`'s lock, or None when no host holds it. A probe that wins
/// the lock lets go of it at once.
pub fn holder(data_dir: &Path) -> std::io::Result<Option<Holder>> {
    let lock_file = data_dir.join(LOCK_FILE);
    let file = match OpenOptions::new().read(true).write(true).open(&lock_file) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    match file.try_lock() {
        Ok(()) => {
            file.unlock()?;
            Ok(None)
        }
        Err(TryLockError::WouldBlock) => Ok(Some(Holder { pid: recorded_pid(&file), lock_file })),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

fn recorded_pid(file: &File) -> Option<u32> {
    let mut first = String::new();
    file.take(256).read_to_string(&mut first).ok()?;
    first.lines().next()?.trim().parse().ok()
}

#[cfg(test)]
#[path = "data_lock_tests.rs"]
mod tests;
