//! The project settings window's state (`ProjectSettingsModal`): the draft of the project it was
//! opened for, Save and Discard, the unsaved-changes confirm on close, the `.env` import and the
//! worktree symlinks read from `.gitignore`. The draft's rules are `model::settings::project`;
//! the pages are `project_settings_view`.

use client::http::{HttpError, HttpRequest, Method};
use model::prefs::EnvVar;
use model::projects::{Event as ProjectEvent, Project};
use model::settings::project::{self, Draft};
use rune::TextEdit;
use serde_json::{Value, json};
use ui::overlay::toast::ToastKind;

use crate::State;
use crate::host::Fetch;

/// The overlay key of the project settings window.
pub const PROJECT: &str = "settings-project";
/// The "Unsaved changes" confirm a close raises over a dirty draft.
pub const LEAVE: &str = "settings-project-leave";

/// One environment variable row: its two fields, and whether its value is a secret.
#[derive(Default)]
pub struct EnvRow {
    pub key: TextEdit,
    pub value: TextEdit,
    pub is_secret: Option<bool>,
}

impl EnvRow {
    fn of(var: &EnvVar) -> Self {
        let value = TextEdit::new(&var.value);
        Self {
            key: TextEdit::new(&var.key),
            value: if var.is_secret == Some(true) { value.secure() } else { value },
            is_secret: var.is_secret,
        }
    }

    fn var(&self) -> EnvVar {
        EnvVar { key: self.key.text.clone(), value: self.value.text.clone(), is_secret: self.is_secret }
    }
}

/// What `.gitignore` is read for: the first fill of an empty list, or the Sync button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitignoreRead {
    AutoFill,
    Sync,
}

#[derive(Default)]
pub struct ProjectSettingsUi {
    /// The page; None is General.
    pub page: Option<&'static str>,
    /// The project the window was opened for; None follows the active project.
    pub target: Option<String>,
    /// The project whose fields the form holds (`symlinkInitProjectRef`).
    pub inited: Option<String>,
    /// The draft as saved: the form is dirty while it differs.
    pub base: Draft,
    pub name: TextEdit,
    pub path: TextEdit,
    pub color: String,
    pub shell: String,
    pub env: Vec<EnvRow>,
    pub dirs: Vec<TextEdit>,
    pub import_error: Option<String>,
    pub import_warning: Option<String>,
    /// Bumped by every import and reset, so a late `.env` read is dropped.
    pub import_request: u64,
    pub syncing: bool,
    /// The project whose empty list `.gitignore` already filled once.
    pub autofilled: Option<String>,
}

impl State {
    /// `openProject(projectId, page)`: the window on `page` (else General) for `project`
    /// (else the active one).
    pub fn open_project_settings(&mut self, project: Option<&str>, page: Option<&str>) {
        let ui = &mut self.project_ui;
        ui.target = project.map(str::to_owned);
        ui.page = page.and_then(|id| ui::settings::pages::PROJECT_PAGES.iter().find(|p| p.id == id)).map(|p| p.id);
        self.settings_ui.search = TextEdit::new("");
        self.overlays.close(crate::DRAWER);
        let Some(p) = self.settings_project().cloned() else {
            log::warn!("[project-settings] no project to open settings for");
            return;
        };
        log::info!("[project-settings] open {} on {}", p.id, self.project_ui.page.unwrap_or("general"));
        if self.project_ui.inited.as_deref() != Some(p.id.as_str()) {
            self.reset_project_form(&p);
        }
        self.fetch_shells();
        self.autofill_symlinks(&p);
        if !self.overlays.is_open(PROJECT) {
            self.overlays.open(PROJECT);
        }
    }

    /// The project the window edits.
    pub(crate) fn settings_project(&self) -> Option<&Project> {
        match &self.project_ui.target {
            Some(id) => self.projects.store.projects.iter().find(|p| p.id == *id),
            None => self.active_project(),
        }
    }

    /// `resetForm`: every field back to the saved project; a pending `.env` read is dropped.
    fn reset_project_form(&mut self, p: &Project) {
        let draft = Draft::of(p);
        let ui = &mut self.project_ui;
        ui.inited = Some(p.id.clone());
        ui.name = TextEdit::new(&draft.name);
        ui.path = TextEdit::new(&draft.path);
        ui.color.clone_from(&draft.color);
        ui.shell.clone_from(&draft.shell);
        ui.env = draft.env_vars.iter().map(EnvRow::of).collect();
        ui.dirs = draft.symlink_dirs.iter().map(TextEdit::new).collect();
        ui.import_request += 1;
        ui.import_error = None;
        ui.import_warning = None;
        ui.base = draft;
    }

    /// The form as a draft.
    pub(crate) fn project_draft(&self) -> Draft {
        let ui = &self.project_ui;
        Draft {
            name: ui.name.text.clone(),
            color: ui.color.clone(),
            path: ui.path.text.clone(),
            shell: ui.shell.clone(),
            env_vars: ui.env.iter().map(EnvRow::var).collect(),
            symlink_dirs: ui.dirs.iter().map(|d| d.text.clone()).collect(),
        }
    }

    pub(crate) fn project_dirty(&self) -> bool {
        self.project_ui.inited.is_some() && self.project_draft() != self.project_ui.base
    }

    /// `handleSave`: one `updateProject` with the normalized draft, which the form then shows.
    pub(crate) fn save_project_settings(&mut self) {
        let Some(id) = self.settings_project().map(|p| p.id.clone()) else { return };
        let updates = self.project_draft().updates();
        log::info!("[project-settings] save {id}");
        let env = updates.client.env_vars.clone().unwrap_or_default();
        let dirs = updates.client.symlink_dirs.clone().unwrap_or_default();
        self.project_ui.env = env.iter().map(EnvRow::of).collect();
        self.project_ui.dirs = dirs.iter().map(TextEdit::new).collect();
        self.project_ui.base = self.project_draft();
        self.projects_step(ProjectEvent::UpdateProject(id, updates));
    }

    /// `handleDiscard`.
    pub(crate) fn discard_project_settings(&mut self) {
        if let Some(p) = self.settings_project().cloned() {
            self.reset_project_form(&p);
        }
    }

    /// `handleClose`: a dirty draft asks first.
    pub fn close_project_settings(&mut self) {
        if self.project_dirty() {
            self.overlays.open(LEAVE);
        } else {
            self.select_close();
            self.close(PROJECT);
        }
    }

    /// The confirm's Leave: the edits go, then the window.
    pub(crate) fn leave_project_settings(&mut self) {
        self.close(LEAVE);
        self.discard_project_settings();
        self.select_close();
        self.close(PROJECT);
    }

    fn select_close(&mut self) {
        if self.settings_ui.select.is_some() {
            self.close_select();
        }
    }

    /// The project the window was opened for is gone or archived: drop the edits and close.
    pub(crate) fn project_settings_check_target(&mut self) {
        if !self.overlays.is_open(PROJECT) || self.project_ui.target.is_none() {
            return;
        }
        if self.settings_project().is_some_and(|p| p.is_archived != Some(true)) {
            return;
        }
        log::info!("[project-settings] the project was removed; closing");
        self.project_ui.inited = None;
        self.project_ui.import_request += 1;
        self.close(LEAVE);
        self.select_close();
        self.close(PROJECT);
        self.toasts.push(self.now, ToastKind::Info, "Project was removed, settings closed.", None);
    }

    pub(crate) fn add_env_var(&mut self) {
        self.project_ui.env.push(EnvRow::default());
    }

    pub(crate) fn remove_env_var(&mut self, index: usize) {
        if index < self.project_ui.env.len() {
            self.project_ui.env.remove(index);
        }
    }

    pub(crate) fn add_symlink_dir(&mut self) {
        self.project_ui.dirs.push(TextEdit::new(""));
    }

    pub(crate) fn remove_symlink_dir(&mut self, index: usize) {
        if index < self.project_ui.dirs.len() {
            self.project_ui.dirs.remove(index);
        }
    }

    /// `handleImportEnvFile`: reads `.env` under the root the form shows.
    pub(crate) fn import_env(&mut self) {
        let ui = &mut self.project_ui;
        ui.import_request += 1;
        ui.import_error = None;
        ui.import_warning = None;
        let root = ui.path.text.trim().to_owned();
        if root.is_empty() {
            ui.import_error = Some("Project root is required to import .env.".into());
            return;
        }
        let (request, project) = (ui.import_request, ui.inited.clone().unwrap_or_default());
        let path = project::env_path(&root);
        log::info!("[project-settings] import {path}");
        if let Some(host) = &mut self.host {
            host.fetch(
                format!("/fs/read?path={}", ui::composer::encode_query(&path)),
                Fetch::ProjectEnv { request, project, root },
            );
        }
    }

    /// The `.env` read: dropped when another import, a reset, another project or a new root
    /// came since.
    pub(crate) fn env_read(&mut self, (request, project, root): (u64, String, String), result: Result<Value, HttpError>) {
        let ui = &self.project_ui;
        if request != ui.import_request || ui.inited.as_deref() != Some(project.as_str()) || ui.path.text.trim() != root {
            log::info!("[project-settings] a stale .env read was dropped");
            return;
        }
        let content = match result {
            Ok(v) => v.get("content").and_then(Value::as_str).unwrap_or_default().to_owned(),
            Err(e) => {
                let message = match e {
                    HttpError::Refused { message, .. } => message,
                    other => other.to_string(),
                };
                self.project_ui.import_error = Some(format!("Failed to read .env: {message}"));
                return;
            }
        };
        let existing: Vec<EnvVar> = self.project_ui.env.iter().map(EnvRow::var).collect();
        let done = project::import(&existing, &content);
        log::info!("[project-settings] .env import: {} rows, warning {}", done.vars.as_ref().map_or(0, Vec::len), done.warning.is_some());
        let ui = &mut self.project_ui;
        if let Some(vars) = done.vars {
            ui.env = vars.iter().map(EnvRow::of).collect();
        }
        ui.import_error = done.error;
        ui.import_warning = done.warning;
    }

    /// D5: a git project that never listed directories gets them from `.gitignore`, once.
    fn autofill_symlinks(&mut self, p: &Project) {
        let empty = p.client.symlink_dirs.as_ref().is_none_or(Vec::is_empty);
        if p.client.is_git_repo != Some(true) || !empty || self.project_ui.autofilled.as_deref() == Some(p.id.as_str()) {
            return;
        }
        self.read_gitignore(p, GitignoreRead::AutoFill);
    }

    /// The Sync from .gitignore button.
    pub(crate) fn sync_gitignore(&mut self) {
        if let Some(p) = self.settings_project().cloned() {
            self.project_ui.syncing = true;
            self.read_gitignore(&p, GitignoreRead::Sync);
        }
    }

    fn read_gitignore(&mut self, p: &Project, why: GitignoreRead) {
        let Some(path) = p.path.clone() else { return };
        let request = HttpRequest {
            method: Method::Post,
            path: "/worktree/parse-gitignore".into(),
            body: Some(json!({ "projectPath": path })),
            command_id: None,
        };
        if let Some(host) = &mut self.host {
            host.request(request, Fetch::Gitignore { project: p.id.clone(), why });
        }
    }

    /// `.gitignore`'s directories that exist: the first fill sets the list, Sync adds the new ones.
    pub(crate) fn gitignore_parsed(&mut self, project: &str, why: GitignoreRead, result: Result<Value, HttpError>) {
        if why == GitignoreRead::Sync {
            self.project_ui.syncing = false;
        }
        if self.project_ui.inited.as_deref() != Some(project) {
            return;
        }
        let found: Vec<String> = match result {
            Ok(Value::Array(dirs)) => dirs
                .iter()
                .filter(|d| d["exists"] == Value::Bool(true))
                .filter_map(|d| d["dirName"].as_str().map(str::to_owned))
                .collect(),
            Ok(_) => Vec::new(),
            Err(e) => {
                log::warn!("[project-settings] reading .gitignore failed: {e}");
                return;
            }
        };
        let ui = &mut self.project_ui;
        match why {
            GitignoreRead::AutoFill => {
                if !found.is_empty() {
                    ui.autofilled = Some(project.to_owned());
                    ui.dirs = found.iter().map(TextEdit::new).collect();
                }
            }
            GitignoreRead::Sync => {
                let mut kept: Vec<String> = ui.dirs.iter().map(|d| d.text.clone()).filter(|d| !d.is_empty()).collect();
                let new: Vec<String> = found.into_iter().filter(|d| !kept.contains(d)).collect();
                if !new.is_empty() {
                    kept.extend(new);
                    ui.dirs = kept.iter().map(TextEdit::new).collect();
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "project_settings_tests.rs"]
mod tests;
