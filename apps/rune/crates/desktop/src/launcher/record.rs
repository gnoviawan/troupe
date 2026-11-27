//! The launcher's files: `server.json` names the server this data dir runs (so a later launch
//! can reuse it), `server-token` holds its bearer token, `last-port` the port to try first
//! (which keeps the device's web storage origin), and `device-id` this install's device. The
//! record and the token are owner-only on Unix.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerRecord {
    pub pid: u32,
    pub port: u16,
    pub binary: PathBuf,
    pub data_dir: PathBuf,
    /// Started to outlive the app (`TROUPE_PREVIEW_KEEP_SERVER=1`): no watchdog, left running
    /// at quit.
    pub keep_alive: bool,
}

pub fn read_record(path: &Path) -> Option<ServerRecord> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text)
        .inspect_err(|e| log::warn!("[launcher] ignoring an unreadable {}: {e}", path.display()))
        .ok()
}

pub fn write_record(path: &Path, record: &ServerRecord) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(record).map_err(std::io::Error::other)?;
    write_private(path, json.as_bytes())
}

pub fn read_token(path: &Path) -> Option<String> {
    let token = fs::read_to_string(path).ok()?;
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_owned())
}

pub fn read_port(path: &Path) -> Option<u16> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// This install's device id, made once: the server keys a device's settings by it.
pub fn device_id(path: &Path) -> String {
    if let Some(id) = fs::read_to_string(path).ok().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()) {
        return id;
    }
    let id = crate::launcher::process::new_token()
        .map(|t| format!("troupe-preview-{}", &t[..16]))
        .unwrap_or_else(|_| format!("troupe-preview-{}", std::process::id()));
    if let Err(e) = write_private(path, id.as_bytes()) {
        log::warn!("[launcher] cannot save the device id: {e}");
    }
    id
}

/// Writes through a temp file and a rename, owner-only on Unix.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&tmp, path)
}
