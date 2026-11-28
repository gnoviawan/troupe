//! The SFTP explorer's pure parts (`hooks/use-sftp-browser.ts`, `hooks/use-sftp-transfers.ts`):
//! remote paths, the listing order, and the transfer rows a transfer writes around its run.

use serde::{Deserialize, Serialize};

use super::{State, Transfer};

/// One remote entry (`SFTPEntry`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub path: String,
    /// `file`, `directory` or `symlink`.
    pub entry_type: String,
    pub size: u64,
    pub permissions: u32,
    pub modified_at: String,
    #[serde(default)]
    pub owner: Option<String>,
}

impl Entry {
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.entry_type == "directory"
    }
}

/// `parentPath`: `/` stays `/`.
#[must_use]
pub fn parent_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(index) if index > 0 => trimmed[..index].to_owned(),
        _ => "/".to_owned(),
    }
}

/// `joinPath`.
#[must_use]
pub fn join_path(dir: &str, name: &str) -> String {
    if dir.ends_with('/') { format!("{dir}{name}") } else { format!("{dir}/{name}") }
}

/// `localFileName`: the last segment, Windows or POSIX.
#[must_use]
pub fn local_file_name(path: &str) -> String {
    path.split(['/', '\\']).filter(|s| !s.is_empty()).next_back().unwrap_or(path).to_owned()
}

/// Folders first, then by name.
pub fn sort_entries(entries: &mut [Entry]) {
    entries.sort_by(|a, b| b.is_dir().cmp(&a.is_dir()).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
}

/// The names of `uploads` (local paths) that already exist as files in `listing`.
#[must_use]
pub fn conflicts(uploads: &[String], listing: &[Entry]) -> Vec<String> {
    uploads
        .iter()
        .map(|local| local_file_name(local))
        .filter(|name| listing.iter().any(|e| !e.is_dir() && e.name == *name))
        .collect()
}

impl State {
    /// `runTransfer`'s first row: pending, before the host's first progress.
    pub fn transfer_started(&mut self, connection_id: &str, remote_path: &str, local_path: &str, direction: &str) {
        self.update_transfer(Transfer {
            connection_id: connection_id.to_owned(),
            remote_path: remote_path.to_owned(),
            local_path: local_path.to_owned(),
            bytes_transferred: 0,
            total_bytes: 0,
            direction: direction.to_owned(),
            status: "in-progress".into(),
            error: None,
        });
    }

    /// `runTransfer`'s last row, keeping the byte counts the host's last event carried.
    pub fn transfer_finished(&mut self, connection_id: &str, remote_path: &str, local_path: &str, direction: &str, result: Result<(), String>) {
        let last = self
            .transfers
            .iter()
            .find(|t| t.connection_id == connection_id && t.remote_path == remote_path && t.direction == direction);
        let (bytes, total) = last.map_or((0, 0), |t| (t.bytes_transferred, t.total_bytes));
        let (status, error, bytes) = match result {
            Ok(()) => ("completed", None, if total > 0 { total } else { bytes }),
            Err(e) => ("failed", Some(e), bytes),
        };
        self.update_transfer(Transfer {
            connection_id: connection_id.to_owned(),
            remote_path: remote_path.to_owned(),
            local_path: local_path.to_owned(),
            bytes_transferred: bytes,
            total_bytes: total,
            direction: direction.to_owned(),
            status: status.into(),
            error,
        });
    }
}
