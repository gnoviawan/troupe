//! Host-owned Troupe Comu policy service (app-wide + per-project).

pub mod model;
mod store;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use serde_json::Value;

use crate::web::{fan_out, EventSink};

pub use model::{
    app_entry_matches, app_identity_key, normalize_app_entry, validate_project_id, ComuAppOp,
    ComuAppPolicy, ComuAppPolicyView, ComuCallerScope, ComuPolicyChanged, ComuPolicyRefusal,
    ComuProjectOp, ComuProjectPolicy, ComuProjectPolicyView, ComuWireCode,
};
use model::{parse_app_op, parse_project_op};
use store::ComuPolicyFiles;

/// Event fanned out over `/ws` when app or project Comu policy changes.
pub const EVENT_COMU_POLICY_CHANGED: &str = "acp:comu_policy_changed";

static INSTALLED: OnceLock<Arc<ComuPolicyService>> = OnceLock::new();

pub fn install(service: Arc<ComuPolicyService>) {
    if INSTALLED.set(service).is_err() {
        log::warn!("[comu-policy] a policy service was already installed");
    }
}

#[must_use]
pub fn installed() -> Option<Arc<ComuPolicyService>> {
    INSTALLED.get().cloned()
}

fn audited<T>(operation: &str, result: Result<T, ComuPolicyRefusal>) -> Result<T, ComuPolicyRefusal> {
    if let Err(refusal) = &result {
        match refusal {
            ComuPolicyRefusal::RevisionConflict { .. } => {
                log::debug!("[comu-policy] {operation} lost a revision race")
            }
            _ => log::warn!(
                "[comu-policy] {operation} refused: code={} reason={}",
                refusal.code().as_str(),
                refusal.reason()
            ),
        }
    }
    result
}

pub struct ComuPolicyService {
    files: ComuPolicyFiles,
    sinks: Vec<Arc<dyn EventSink>>,
}

impl ComuPolicyService {
    #[must_use]
    pub fn new(root: PathBuf, sinks: Vec<Arc<dyn EventSink>>) -> Self {
        Self {
            files: ComuPolicyFiles::new(root),
            sinks,
        }
    }

    pub async fn read_app_policy(&self) -> ComuAppPolicy {
        self.files.read_app().await.unwrap_or_default()
    }

    pub async fn read_project_policy(&self, project_id: &str) -> ComuProjectPolicy {
        self.files
            .read_project(project_id)
            .await
            .unwrap_or_default()
    }

    pub async fn app_view(
        &self,
        caller: &ComuCallerScope,
    ) -> Result<ComuAppPolicyView, ComuPolicyRefusal> {
        let result = async {
            caller.require_app()?;
            let policy = self.files.read_app().await?;
            Ok(ComuAppPolicyView::from(&policy))
        }
        .await;
        audited("read app policy", result)
    }

    pub async fn project_view(
        &self,
        caller: &ComuCallerScope,
        project_id: &str,
    ) -> Result<ComuProjectPolicyView, ComuPolicyRefusal> {
        let result = async {
            validate_project_id(project_id)?;
            caller.require_project(project_id)?;
            let policy = self.files.read_project(project_id).await?;
            Ok(ComuProjectPolicyView::from_policy(project_id, &policy))
        }
        .await;
        audited("read project policy", result)
    }

    pub async fn apply_app(
        &self,
        caller: &ComuCallerScope,
        expected_revision: u64,
        op: &Value,
    ) -> Result<ComuAppPolicyView, ComuPolicyRefusal> {
        let result = async {
            caller.require_app()?;
            let parsed = parse_app_op(op)?;
            let (policy, changed) = self
                .files
                .update_app(Some(expected_revision), parsed)
                .await?;
            if changed {
                self.emit_changed(ComuPolicyChanged {
                    scope: "app",
                    project_id: None,
                    revision: policy.revision,
                });
            }
            Ok(ComuAppPolicyView::from(&policy))
        }
        .await;
        audited("edit app policy", result)
    }

    pub async fn apply_project(
        &self,
        caller: &ComuCallerScope,
        project_id: &str,
        expected_revision: u64,
        op: &Value,
    ) -> Result<ComuProjectPolicyView, ComuPolicyRefusal> {
        let result = async {
            validate_project_id(project_id)?;
            caller.require_project(project_id)?;
            let parsed = parse_project_op(op)?;
            let (policy, changed) = self
                .files
                .update_project(project_id, Some(expected_revision), parsed)
                .await?;
            if changed {
                self.emit_changed(ComuPolicyChanged {
                    scope: "project",
                    project_id: Some(project_id.to_string()),
                    revision: policy.revision,
                });
            }
            Ok(ComuProjectPolicyView::from_policy(project_id, &policy))
        }
        .await;
        audited("edit project policy", result)
    }

    /// True if `app_id` or `app_name` is allow-listed globally or in `project_id`.
    pub async fn is_app_allowlisted(
        &self,
        project_id: &str,
        app_id: &str,
        app_name: &str,
    ) -> bool {
        let app_policy = self.read_app_policy().await;
        if app_policy
            .allowed_apps
            .iter()
            .any(|saved| app_entry_matches(saved, app_id, app_name))
        {
            return true;
        }
        if validate_project_id(project_id).is_ok() {
            let proj_policy = self.read_project_policy(project_id).await;
            if proj_policy
                .allowed_apps
                .iter()
                .any(|saved| app_entry_matches(saved, app_id, app_name))
            {
                return true;
            }
        }
        false
    }

    /// Persist an `"allow_project"` consent decision into `project_id`'s allowed apps.
    pub async fn record_project_approval(&self, project_id: &str, app_entry: &str) {
        if validate_project_id(project_id).is_err() {
            return;
        }
        let Ok(clean) = normalize_app_entry(app_entry) else {
            return;
        };
        if let Ok((policy, true)) = self
            .files
            .update_project(project_id, None, ComuProjectOp::AddApp(clean))
            .await
        {
            self.emit_changed(ComuPolicyChanged {
                scope: "project",
                project_id: Some(project_id.to_string()),
                revision: policy.revision,
            });
        }
    }

    fn emit_changed(&self, payload: ComuPolicyChanged) {
        if let Ok(val) = serde_json::to_value(payload) {
            fan_out(&self.sinks, None, EVENT_COMU_POLICY_CHANGED, &val);
        }
    }
}

#[cfg(test)]
mod tests;
