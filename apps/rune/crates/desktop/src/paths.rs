//! Where the desktop app keeps things. The server opens the stores the Tauri app keeps in its
//! `app_data_dir` (identifier `com.termul-manager.app`, `src-tauri/tauri.conf.json`), so a user
//! who quits the Tauri app and opens this one sees the same projects and chats. Logs go to the
//! Tauri app's `app_log_dir`. The launcher's own files live in `<data dir>/troupe-preview`.
//!
//! `TROUPE_PREVIEW_DATA_DIR` moves all three under one folder, for tests.

use std::path::PathBuf;

pub const IDENTIFIER: &str = "com.termul-manager.app";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Linux,
    Mac,
    Windows,
}

impl Os {
    pub const CURRENT: Os = if cfg!(target_os = "macos") {
        Os::Mac
    } else if cfg!(target_os = "windows") {
        Os::Windows
    } else {
        Os::Linux
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    /// The Tauri app's `app_data_dir`: the server's state dir and stores.
    pub data_dir: PathBuf,
    /// The Tauri app's `app_log_dir`.
    pub log_dir: PathBuf,
    /// The launcher's record, token, port, server log and window frame.
    pub launcher_dir: PathBuf,
}

impl Paths {
    pub fn from_env() -> Result<Paths, String> {
        Paths::resolve(Os::CURRENT, |name| {
            std::env::var(name).ok().filter(|v| !v.trim().is_empty())
        })
    }

    /// The paths on `os` with `env` as its environment: Tauri's `app_data_dir` and
    /// `app_log_dir` (dirs' `data_dir`, `data_local_dir` and `~/Library/Logs`).
    pub fn resolve(os: Os, env: impl Fn(&str) -> Option<String>) -> Result<Paths, String> {
        if let Some(dir) = env("TROUPE_PREVIEW_DATA_DIR") {
            let data_dir = PathBuf::from(dir);
            return Ok(Paths {
                log_dir: data_dir.join("logs"),
                launcher_dir: data_dir.join("troupe-preview"),
                data_dir,
            });
        }
        let var = |name: &str| env(name).map(PathBuf::from).ok_or_else(|| format!("{name} is not set"));
        let (data_dir, log_dir) = match os {
            Os::Linux => {
                let share = match env("XDG_DATA_HOME") {
                    Some(dir) => PathBuf::from(dir),
                    None => var("HOME")?.join(".local/share"),
                };
                let data = share.join(IDENTIFIER);
                (data.clone(), data.join("logs"))
            }
            Os::Mac => {
                let home = var("HOME")?;
                (
                    home.join("Library/Application Support").join(IDENTIFIER),
                    home.join("Library/Logs").join(IDENTIFIER),
                )
            }
            Os::Windows => (
                var("APPDATA")?.join(IDENTIFIER),
                var("LOCALAPPDATA")?.join(IDENTIFIER).join("logs"),
            ),
        };
        Ok(Paths {
            launcher_dir: data_dir.join("troupe-preview"),
            data_dir,
            log_dir,
        })
    }

    pub fn record_file(&self) -> PathBuf {
        self.launcher_dir.join("server.json")
    }

    pub fn token_file(&self) -> PathBuf {
        self.launcher_dir.join("server-token")
    }

    pub fn last_port_file(&self) -> PathBuf {
        self.launcher_dir.join("last-port")
    }

    pub fn server_log(&self) -> PathBuf {
        self.launcher_dir.join("server.log")
    }

    pub fn device_id_file(&self) -> PathBuf {
        self.launcher_dir.join("device-id")
    }

    pub fn window_file(&self) -> PathBuf {
        self.launcher_dir.join("window.json")
    }

    /// The single-instance id: one running app per data dir, so a test run under its own
    /// data dir never hands its launch to the user's app. Aliases of one folder (a symlink,
    /// `a/../a`) share an id once the folder exists.
    pub fn instance_id(&self) -> String {
        let dir = std::fs::canonicalize(&self.data_dir).unwrap_or_else(|_| self.data_dir.clone());
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in dir.to_string_lossy().bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
        format!("{IDENTIFIER}.preview-{hash:016x}")
    }
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
