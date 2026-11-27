//! Atomic schema-versioned file store for Troupe Comu policy documents.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as AsyncMutex;

use crate::acp::atomic_file;

use super::model::{
    apply_app_mutation, apply_project_mutation, normalize_app_entry, validate_project_id,
    ComuAppOp, ComuAppPolicy, ComuPolicyRefusal, ComuProjectOp, ComuProjectPolicy,
};

pub const SCHEMA_VERSION: u32 = 1;

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppFile {
    version: u32,
    #[serde(default)]
    revision: u64,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_true")]
    cursor_enabled: bool,
    #[serde(default = "default_true")]
    ask_before_driving_apps: bool,
    #[serde(default = "default_true")]
    ask_before_foreground_desktop: bool,
    #[serde(default = "default_true")]
    ask_before_clipboard_read: bool,
    #[serde(default)]
    allowed_apps: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFile {
    version: u32,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    allowed_apps: Vec<String>,
}

#[derive(Deserialize)]
struct VersionProbe {
    version: u32,
}

/// Bijective lowercase-only storage key so distinct `project_id`s (e.g.
/// `"proj-alpha"` vs `"PROJ-ALPHA"`) never collide on case-insensitive
/// filesystems (Windows, macOS APFS default).
///
/// `[a-z0-9-]` stay literal; every other byte (including `'_'` and `'A'..='Z'`)
/// encodes as `_{byte:02x}`.
#[must_use]
pub fn encode_project_storage_key(project_id: &str) -> String {
    let mut out = String::with_capacity(project_id.len() + 8);
    for &b in project_id.as_bytes() {
        if b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' {
            out.push(char::from(b));
        } else {
            let _ = write!(&mut out, "_{b:02x}");
        }
    }
    out
}

pub struct ComuPolicyFiles {
    root: PathBuf,
    app_lock: AsyncMutex<()>,
    project_locks: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl ComuPolicyFiles {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            app_lock: AsyncMutex::new(()),
            project_locks: Mutex::new(HashMap::new()),
        }
    }

    fn app_path(&self) -> PathBuf {
        self.root.join("app.json")
    }

    fn project_path_for_key(&self, storage_key: &str) -> PathBuf {
        self.root
            .join("projects")
            .join(format!("{storage_key}.json"))
    }

    /// Fallback read path for lowercase-only legacy files written before
    /// `encode_project_storage_key` (e.g. `__unassigned__.json` vs
    /// `_5f_5funassigned_5f_5f.json`), restricted to IDs with no ASCII
    /// uppercase so uppercase IDs never alias lowercase files on Windows/macOS.
    fn legacy_project_path(&self, project_id: &str, storage_key: &str) -> Option<PathBuf> {
        if project_id == storage_key || project_id.bytes().any(|b| b.is_ascii_uppercase()) {
            return None;
        }
        Some(
            self.root
                .join("projects")
                .join(format!("{project_id}.json")),
        )
    }

    fn project_lock(&self, storage_key: &str) -> Arc<AsyncMutex<()>> {
        let mut guard = self.project_locks.lock();
        Arc::clone(
            guard
                .entry(storage_key.to_string())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        )
    }

    fn read_project_by_key(&self, project_id: &str, storage_key: &str) -> ComuProjectPolicy {
        let primary = self.project_path_for_key(storage_key);
        if primary.exists() {
            return read_project_unlocked(&primary);
        }
        if let Some(legacy) = self.legacy_project_path(project_id, storage_key) {
            if legacy.exists() {
                return read_project_unlocked(&legacy);
            }
        }
        ComuProjectPolicy::default()
    }

    pub async fn read_app(&self) -> Result<ComuAppPolicy, ComuPolicyRefusal> {
        let _guard = self.app_lock.lock().await;
        Ok(read_app_unlocked(&self.app_path()))
    }

    pub async fn read_project(&self, project_id: &str) -> Result<ComuProjectPolicy, ComuPolicyRefusal> {
        validate_project_id(project_id)?;
        let key = encode_project_storage_key(project_id);
        let lock = self.project_lock(&key);
        let _guard = lock.lock().await;
        Ok(self.read_project_by_key(project_id, &key))
    }

    pub async fn update_app(
        &self,
        expected_revision: Option<u64>,
        op: ComuAppOp,
    ) -> Result<(ComuAppPolicy, bool), ComuPolicyRefusal> {
        let _guard = self.app_lock.lock().await;
        let path = self.app_path();
        let mut policy = read_app_unlocked(&path);
        if let Some(expected) = expected_revision {
            if policy.revision != expected {
                return Err(ComuPolicyRefusal::RevisionConflict {
                    current: policy.revision,
                });
            }
        }
        let changed = apply_app_mutation(&mut policy, op)?;
        if changed {
            write_app_unlocked(&path, &policy)?;
        }
        Ok((policy, changed))
    }

    pub async fn update_project(
        &self,
        project_id: &str,
        expected_revision: Option<u64>,
        op: ComuProjectOp,
    ) -> Result<(ComuProjectPolicy, bool), ComuPolicyRefusal> {
        validate_project_id(project_id)?;
        let key = encode_project_storage_key(project_id);
        let lock = self.project_lock(&key);
        let _guard = lock.lock().await;
        let path = self.project_path_for_key(&key);
        let mut policy = self.read_project_by_key(project_id, &key);
        if let Some(expected) = expected_revision {
            if policy.revision != expected {
                return Err(ComuPolicyRefusal::RevisionConflict {
                    current: policy.revision,
                });
            }
        }
        let changed = apply_project_mutation(&mut policy, op)?;
        if changed {
            write_project_unlocked(&path, &policy)?;
        }
        Ok((policy, changed))
    }
}

fn sanitize_apps(entries: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for entry in entries {
        if let Ok(clean) = normalize_app_entry(&entry) {
            if !out.iter().any(|existing: &String| existing.eq_ignore_ascii_case(&clean)) {
                out.push(clean);
            }
        }
    }
    out
}

fn read_app_unlocked(path: &Path) -> ComuAppPolicy {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ComuAppPolicy::default(),
        Err(e) => {
            log::warn!("[comu-policy] failed reading app policy (fail-closed): {e}");
            return ComuAppPolicy {
                read_only: true,
                enabled: false,
                ..ComuAppPolicy::default()
            };
        }
    };
    if let Ok(probe) = serde_json::from_slice::<VersionProbe>(&bytes) {
        if probe.version != SCHEMA_VERSION {
            log::warn!(
                "[comu-policy] unsupported app policy schema version {} (expected {}); failing closed",
                probe.version,
                SCHEMA_VERSION
            );
            return ComuAppPolicy {
                read_only: true,
                enabled: false,
                ..ComuAppPolicy::default()
            };
        }
    }
    match serde_json::from_slice::<AppFile>(&bytes) {
        Ok(file) => ComuAppPolicy {
            revision: file.revision,
            read_only: false,
            enabled: file.enabled,
            cursor_enabled: file.cursor_enabled,
            ask_before_driving_apps: file.ask_before_driving_apps,
            ask_before_foreground_desktop: file.ask_before_foreground_desktop,
            ask_before_clipboard_read: file.ask_before_clipboard_read,
            allowed_apps: sanitize_apps(file.allowed_apps),
        },
        Err(_) => {
            log::warn!("[comu-policy] corrupt app policy JSON (fail-closed read-only)");
            ComuAppPolicy {
                read_only: true,
                enabled: false,
                ..ComuAppPolicy::default()
            }
        }
    }
}

fn read_project_unlocked(path: &Path) -> ComuProjectPolicy {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ComuProjectPolicy::default(),
        Err(e) => {
            log::warn!("[comu-policy] failed reading project policy (fail-closed): {e}");
            return ComuProjectPolicy {
                read_only: true,
                ..ComuProjectPolicy::default()
            };
        }
    };
    if let Ok(probe) = serde_json::from_slice::<VersionProbe>(&bytes) {
        if probe.version != SCHEMA_VERSION {
            return ComuProjectPolicy {
                read_only: true,
                ..ComuProjectPolicy::default()
            };
        }
    }
    match serde_json::from_slice::<ProjectFile>(&bytes) {
        Ok(file) => ComuProjectPolicy {
            revision: file.revision,
            read_only: false,
            allowed_apps: sanitize_apps(file.allowed_apps),
        },
        Err(_) => {
            log::warn!("[comu-policy] corrupt project policy JSON (fail-closed read-only)");
            ComuProjectPolicy {
                read_only: true,
                ..ComuProjectPolicy::default()
            }
        }
    }
}

fn write_app_unlocked(path: &Path, policy: &ComuAppPolicy) -> Result<(), ComuPolicyRefusal> {
    let file = AppFile {
        version: SCHEMA_VERSION,
        revision: policy.revision,
        enabled: policy.enabled,
        cursor_enabled: policy.cursor_enabled,
        ask_before_driving_apps: policy.ask_before_driving_apps,
        ask_before_foreground_desktop: policy.ask_before_foreground_desktop,
        ask_before_clipboard_read: policy.ask_before_clipboard_read,
        allowed_apps: policy.allowed_apps.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&file)
        .map_err(|e| ComuPolicyRefusal::StorageFailed(e.to_string()))?;
    atomic_file::replace(path, &bytes)
        .map_err(|e| ComuPolicyRefusal::StorageFailed(e.to_string()))
}

fn write_project_unlocked(
    path: &Path,
    policy: &ComuProjectPolicy,
) -> Result<(), ComuPolicyRefusal> {
    let file = ProjectFile {
        version: SCHEMA_VERSION,
        revision: policy.revision,
        allowed_apps: policy.allowed_apps.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&file)
        .map_err(|e| ComuPolicyRefusal::StorageFailed(e.to_string()))?;
    atomic_file::replace(path, &bytes)
        .map_err(|e| ComuPolicyRefusal::StorageFailed(e.to_string()))
}
