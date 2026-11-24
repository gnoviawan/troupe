//! The device's own project preferences: what the host registry does not
//! carry (shell, env vars, worktrees, git state, icon) plus the sidebar
//! layout. Port of the web path of `src/renderer/lib/project-prefs.ts`.
//!
//! The app reads the `project-prefs` store key into [`State::load`] once,
//! and writes the [`Record`] a [`State::persist`] returns, stamping
//! `updatedAt`. The web keeps no env vars; the desktop's secure-storage path
//! is not ported.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::projects::{Group, Project};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    pub key: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_secret: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub id: String,
    pub name: String,
    pub branch: String,
    pub path: String,
    pub created_at: String,
    /// Derived on the web by the worktree reconciler; never persisted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outside_project_root: Option<bool>,
}

/// The fields of a project the host registry does not carry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_shell: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_vars: Option<Vec<EnvVar>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktrees: Option<Vec<Worktree>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symlink_dirs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_git_repo: Option<bool>,
    /// The resolved icon (`data:` URI and fetch stamp), opaque here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Value>,
}

impl ClientFields {
    #[must_use]
    pub fn of(project: &Project) -> Self {
        project.client.clone()
    }

    /// The record's form: worktrees lose their derived flag, env vars stay
    /// out (the web has no secure storage).
    fn persisted(&self) -> Self {
        Self {
            env_vars: None,
            worktrees: self.worktrees.as_ref().map(|worktrees| {
                worktrees
                    .iter()
                    .map(|worktree| Worktree {
                        outside_project_root: None,
                        ..worktree.clone()
                    })
                    .collect()
            }),
            ..self.clone()
        }
    }
}

/// The sidebar layout: groups and the order projects are listed in.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub groups: Vec<Group>,
    pub order: Vec<String>,
}

/// The `project-prefs` store record, without `updatedAt`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    #[serde(default)]
    pub projects: std::collections::BTreeMap<String, ClientFields>,
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub order: Vec<String>,
}

/// What the device remembers, and what it last wrote.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub layout: Layout,
    /// Client fields by project id, in the order they were first seen.
    #[serde(skip)]
    pub known: Vec<(String, ClientFields)>,
    /// What the last write covered; a persist that matches it writes nothing.
    #[serde(skip)]
    last_signature: Option<String>,
}

impl State {
    /// Replace memory with a read record (`None`: no record yet).
    pub fn load(&mut self, record: Option<Record>) {
        *self = Self::default();
        let Some(record) = record else {
            return;
        };
        self.known = record
            .projects
            .into_iter()
            .map(|(id, fields)| (id, fields.persisted()))
            .collect();
        self.layout = Layout {
            groups: record.groups,
            order: record.order,
        };
    }

    #[must_use]
    pub fn get(&self, project_id: &str) -> Option<&ClientFields> {
        self.known
            .iter()
            .find(|(id, _)| id == project_id)
            .map(|(_, fields)| fields)
    }

    /// Remember a project's fields, keeping its place when already known.
    pub fn remember(&mut self, project_id: &str, fields: ClientFields) {
        match self.known.iter_mut().find(|(id, _)| id == project_id) {
            Some((_, known)) => *known = fields,
            None => self.known.push((project_id.to_owned(), fields)),
        }
    }

    /// The record for the store's projects and groups and the signature it
    /// claims, or `None` when nothing it covers changed since the last write.
    pub fn persist(&mut self, projects: &[Project], groups: &[Group]) -> Option<(Record, String)> {
        for project in projects {
            self.remember(&project.id, ClientFields::of(project));
        }
        let listed: Vec<&str> = projects.iter().map(|project| project.id.as_str()).collect();
        let order: Vec<String> = listed
            .iter()
            .map(|id| (*id).to_owned())
            .chain(
                self.layout
                    .order
                    .iter()
                    .filter(|id| !listed.contains(&id.as_str()))
                    .cloned(),
            )
            .collect();
        let signature =
            serde_json::to_string(&(&self.known, groups, &order, &listed)).unwrap_or_default();
        if self.last_signature.as_ref() == Some(&signature) {
            return None;
        }
        self.last_signature = Some(signature.clone());
        self.layout = Layout {
            groups: groups.to_vec(),
            order: order.clone(),
        };
        let record = Record {
            projects: self
                .known
                .iter()
                .map(|(id, fields)| (id.clone(), fields.persisted()))
                .collect(),
            groups: groups.to_vec(),
            order,
        };
        Some((record, signature))
    }

    /// The write that claimed `signature` failed: release the claim so the
    /// next persist writes again, unless a newer persist claimed since.
    pub fn write_failed(&mut self, signature: &str) {
        if self.last_signature.as_deref() == Some(signature) {
            self.last_signature = None;
        }
    }

    /// Drop everything kept for a project this device deleted.
    pub fn forget(&mut self, project_id: &str) {
        self.known.retain(|(id, _)| id != project_id);
        for group in &mut self.layout.groups {
            group.project_ids.retain(|id| id != project_id);
        }
        self.layout.order.retain(|id| id != project_id);
        self.last_signature = None;
    }
}

#[cfg(test)]
#[path = "prefs_tests.rs"]
mod tests;
