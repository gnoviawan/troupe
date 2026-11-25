//! The git driver: runs `model::git`'s calls on the server's `/git/*` routes, keeps the
//! active project's working copy fresh (on project change and every 30 s, as
//! `git-status-sources.ts` polls), copies its branch into the project (`useProjectGitBranch`),
//! and holds the panel's own state: the commit fields, the busy flags, the dialogs' input
//! and what each action does when its task settles (`use-git-actions.ts`).

use std::collections::{HashMap, HashSet};

use client::http::{HttpError, HttpRequest, Method};
use model::git::{self, Effect, Event, Failure, Outcome, TaskId};
use model::prefs::ClientFields;
use model::projects::{Event as ProjectEvent, ProjectUpdates};
use model::workspace::Tab;
use rune::TextEdit;
use rune_kit::diff::DiffState;
use ui::overlay::ToastKind;

use crate::State;

/// `GIT_STATUS_POLL_INTERVAL_MS`, in seconds.
const POLL_SECS: f64 = 30.0;

pub const BRANCH_MENU: &str = "git-branch-menu";
pub const CREATE_BRANCH: &str = "git-create-branch";
pub const SWITCH_BRANCH: &str = "git-switch-branch";
pub const STASH_MENU: &str = "git-stash-menu";
pub const STASH_DIALOG: &str = "git-stash-dialog";
pub const DISCARD_CONFIRM: &str = "git-discard-confirm";
pub const AMEND_CONFIRM: &str = "git-amend-confirm";
/// The phone's Git changes sheet (`MobileWorkspaceShell`'s git sheet).
pub const GIT_SHEET: &str = "git-sheet";

/// What to do when an action's task settles.
#[derive(Clone, Debug)]
enum Then {
    /// A success toast (none for `None`) or `"{fail}: {error}"`.
    Toast { ok: Option<String>, fail: &'static str },
    Commit { amend: bool },
    Push,
    Discard { paths: Vec<String> },
    /// The diff read for this key ended.
    Diff(String),
    /// `handleExecuteSwitchBranch('stash')`: stash, then switch, then pop.
    StashThenSwitch { cwd: String, name: String },
    SwitchThenPop { cwd: String, name: String },
    Pop { name: String },
}

#[derive(Default)]
pub struct GitUi {
    pub cache: git::State,
    then: HashMap<TaskId, Then>,
    /// Calls made with no server link (a headless run or a test), oldest first.
    pub(crate) unsent: Vec<(TaskId, git::Route)>,
    /// Trailing diff-stat reads, by working copy, due at (ms).
    due: Vec<(String, f64)>,
    /// The diff keys being read.
    reading: HashSet<String>,
    pub summary: TextEdit,
    pub description: TextEdit,
    pub amend: bool,
    pub filter: TextEdit,
    pub committing: bool,
    pub pushing: bool,
    pub mutating: bool,
    /// The paths the discard confirm names.
    pub discard: Vec<String>,
    /// The branch the uncommitted-changes dialog switches to.
    pub pending_branch: Option<String>,
    pub new_branch: TextEdit,
    pub stash_message: TextEdit,
    pub stash_untracked: bool,
    /// Whether the selected file's staged side is the one shown.
    pub selected_staged: bool,
    /// The working copy the poll last refreshed, and when (seconds).
    read: Option<(String, f64)>,
    /// Per diff tab: the diff text its view was built from, and the view.
    pub diff_views: HashMap<String, (String, DiffState)>,
}

impl GitUi {
    /// Whether a write is running, so the panel's write controls are off.
    pub fn busy(&self) -> bool {
        self.mutating || self.committing || self.pushing
    }
}

/// The ledger verdicts the web client turns into `AcpTransportError` codes.
fn ledger_code(code: &str) -> Option<&'static str> {
    match code {
        "PRECONDITION_FAILED" => Some(git::PRECONDITION_FAILED),
        "OUTCOME_UNKNOWN" => Some("outcome_unknown"),
        "COMMAND_CONFLICT" => Some("command_conflict"),
        "INVALID_COMMAND_ID" => Some("invalid_command_id"),
        _ => None,
    }
}

fn failure(error: HttpError) -> Failure {
    match error {
        HttpError::Refused { code, message, .. } => Failure {
            message,
            code: ledger_code(&code).map(str::to_owned),
        },
        HttpError::Network(detail) => Failure::new(detail),
        HttpError::InProgress => Failure::new("the first run of this command is still going"),
    }
}

/// The routes the host runs through its command ledger (`LEDGERED_ROUTES` in
/// `lib/ipc/http.ts`): each send carries a fresh `X-Command-Id`.
const LEDGERED: [&str; 9] = [
    "/git/stage-hunk",
    "/git/unstage-hunk",
    "/git/discard",
    "/git/commit",
    "/git/create-branch",
    "/git/stash-save",
    "/git/stash-apply",
    "/git/stash-pop",
    "/git/stash-drop",
];

fn request(route: git::Route) -> HttpRequest {
    let command_id = LEDGERED.contains(&route.path.as_str()).then(crate::ids::uuid);
    HttpRequest {
        method: if route.post { Method::Post } else { Method::Get },
        path: route.path,
        body: route.body,
        command_id,
    }
}

#[cfg(test)]
pub(crate) fn tests_request(path: &str) -> HttpRequest {
    request(git::Route { post: true, path: path.to_owned(), body: None })
}

impl State {
    /// The active project's folder: the working copy the panel shows.
    pub fn git_cwd(&self) -> Option<String> {
        self.active_project()?.path.clone().filter(|p| !p.is_empty())
    }

    /// Applies one git event and carries out its effects.
    pub(crate) fn git_step(&mut self, event: Event) -> Option<TaskId> {
        let step = git::step(&mut self.git.cache, event, self.now * 1000.0);
        self.run_git(step.effects);
        step.task
    }

    /// Starts an action whose settle runs `then`.
    fn git_action(&mut self, event: Event, then: Then) {
        let step = git::step(&mut self.git.cache, event, self.now * 1000.0);
        if let Some(task) = step.task {
            self.git.then.insert(task, then);
        }
        self.run_git(step.effects);
    }

    fn run_git(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Call { task, call } => {
                    let route = call.route();
                    match &mut self.host {
                        Some(host) => host.request(request(route), crate::host::Fetch::Git(task)),
                        None => self.git.unsent.push((task, route)),
                    }
                }
                Effect::Log { message, .. } => log::warn!("{message}"),
                Effect::Toast { message } => self.toast_error(&message, ""),
                Effect::ReconcileWorktree { cwd } => {
                    log::info!("[git-status] branch switched in {cwd}; worktree owners re-list on the next projects frame");
                }
                Effect::ScheduleDiffStat { cwd, at } => self.git.due.push((cwd, at)),
                Effect::Settled { task, outcome } => self.git_settled(task, outcome),
            }
        }
    }

    /// The server answered a git call.
    pub(crate) fn git_replied(&mut self, task: TaskId, result: Result<serde_json::Value, HttpError>) {
        if let Err(e) = &result {
            log::warn!("[git] call failed: task={task} {e}");
        }
        self.git_step(Event::Reply(task, result.map_err(failure)));
    }

    fn git_settled(&mut self, task: TaskId, outcome: Outcome) {
        let Some(then) = self.git.then.remove(&task) else {
            return;
        };
        let failed = outcome.as_ref().err().map(Failure::shown);
        match then {
            Then::Toast { ok, fail } => {
                self.git.mutating = false;
                match failed {
                    Some(error) => self.toast_error(&format!("{fail}: {error}"), ""),
                    None => {
                        if let Some(ok) = ok {
                            self.git_toast(ToastKind::Success, &ok);
                        }
                    }
                }
            }
            Then::Commit { amend } => {
                self.git.committing = false;
                self.overlays.close(AMEND_CONFIRM);
                match failed {
                    Some(error) => self.toast_error(&format!("Failed to commit: {error}"), ""),
                    None => {
                        self.git.summary = TextEdit::default();
                        self.git.description = TextEdit::default();
                        self.git.amend = false;
                        let done = if amend { "Commit amended" } else { "Changes committed" };
                        self.git_toast(ToastKind::Success, done);
                    }
                }
            }
            Then::Push => {
                self.git.pushing = false;
                match failed {
                    Some(error) => self.toast_error(&format!("Failed to push: {error}"), ""),
                    None => self.git_toast(ToastKind::Success, "Pushed to remote"),
                }
            }
            Then::Discard { paths } => {
                self.git.mutating = false;
                self.overlays.close(DISCARD_CONFIRM);
                self.git.discard.clear();
                match failed {
                    Some(error) => self.toast_error(&format!("Failed to discard changes: {error}"), ""),
                    None => {
                        if self.git.cache.selected_file.as_ref().is_some_and(|f| paths.contains(f)) {
                            self.git_step(Event::SetSelectedFile(None));
                        }
                    }
                }
            }
            Then::Diff(key) => {
                self.git.reading.remove(&key);
            }
            Then::StashThenSwitch { cwd, name } => match failed {
                Some(error) => self.switch_failed(&error),
                None => self.git_action(Event::BranchSwitch(cwd.clone(), name.clone()), Then::SwitchThenPop { cwd, name }),
            },
            Then::SwitchThenPop { cwd, name } => match failed {
                Some(error) => self.switch_failed(&error),
                None => self.git_action(Event::StashPop(cwd, 0), Then::Pop { name }),
            },
            Then::Pop { name } => {
                self.git.mutating = false;
                match failed {
                    Some(_) => self.git_toast(ToastKind::Warning, &format!("Switched to branch {name}, but changes were left in stash@{{0}} due to conflicts")),
                    None => self.git_toast(ToastKind::Success, &format!("Switched to branch {name} and reapplied changes")),
                }
            }
        }
    }

    fn git_toast(&mut self, kind: ToastKind, title: &str) {
        self.toasts.push(self.now, kind, title, None);
    }

    fn switch_failed(&mut self, error: &str) {
        self.git.mutating = false;
        self.toast_error(&format!("Failed to switch branch: {error}"), "");
    }

    /// Each frame: the poll, trailing diff-stat reads, diffs the open diff tabs need, and the
    /// project's branch. Returns the next time it wants to run.
    pub(crate) fn git_tick(&mut self) -> Option<f64> {
        if self.status != client::link::Status::Live {
            return None;
        }
        let now_ms = self.now * 1000.0;
        let due: Vec<String> = {
            let (ready, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.git.due).into_iter().partition(|(_, at)| *at <= now_ms);
            self.git.due = later;
            ready.into_iter().map(|(cwd, _)| cwd).collect()
        };
        for cwd in due {
            self.git_step(Event::DiffStatDue(cwd));
        }
        let cwd = self.git_cwd()?;
        let fresh = matches!(&self.git.read, Some((read, at)) if *read == cwd && self.now - at < POLL_SECS);
        if !fresh {
            let first = self.git.read.as_ref().is_none_or(|(read, _)| *read != cwd);
            self.git.read = Some((cwd.clone(), self.now));
            if first {
                self.git.filter = TextEdit::default();
                self.git_step(Event::WatchDiffStat(cwd.clone()));
                self.git_step(Event::FetchStashes(cwd.clone()));
                self.git_step(Event::FetchBranches(cwd.clone()));
            }
            self.git_step(Event::RefreshWorkingCopy(cwd.clone()));
        }
        self.read_open_diffs();
        self.copy_branch(&cwd);
        let next = self.git.read.as_ref().map(|(_, at)| at + POLL_SECS);
        let trailing = self.git.due.iter().map(|(_, at)| at / 1000.0).reduce(f64::min);
        [next, trailing].into_iter().flatten().reduce(f64::min)
    }

    /// `useProjectGitBranch`: the active project takes its working copy's branch.
    fn copy_branch(&mut self, cwd: &str) {
        let Some(context) = self.git.cache.commit_context(cwd) else {
            return;
        };
        let branch = context.branch.clone();
        let has_head = context.has_head;
        let Some(project) = self.active_project() else {
            return;
        };
        let stamp_repo = has_head && project.client.is_git_repo != Some(true);
        if project.client.git_branch == branch && !stamp_repo {
            return;
        }
        let id = project.id.clone();
        let mut client = ClientFields { git_branch: branch, ..ClientFields::default() };
        if stamp_repo {
            client.is_git_repo = Some(true);
        }
        self.projects_step(ProjectEvent::UpdateProject(id, ProjectUpdates { client, ..ProjectUpdates::default() }));
    }

    /// The diff tabs of the visible project: read each one's diff once, and rebuild a view
    /// whose text changed.
    fn read_open_diffs(&mut self) {
        let Some(root) = self.workspace.record().map(|_| self.workspace.root().clone()) else {
            return;
        };
        let tabs: Vec<(String, String, String, bool)> = model::workspace::leaves(&root)
            .into_iter()
            .flat_map(|leaf| leaf.tabs.iter())
            .filter_map(|tab| match tab {
                Tab::Diff { id, cwd, path, staged } => Some((id.clone(), cwd.clone(), path.clone(), *staged)),
                _ => None,
            })
            .chain(self.sheet_diff())
            .collect();
        for (id, cwd, path, staged) in tabs {
            match self.git.cache.diff(&cwd, &path, staged).map(str::to_owned) {
                Some(text) => {
                    let current = self.git.diff_views.get(&id).is_some_and(|(built, _)| *built == text);
                    if !current {
                        let files = rune_kit::diff::parse(&text);
                        self.git.diff_views.insert(id, (text, DiffState::new(files)));
                    }
                }
                None if self.git.cache.diff_error(&cwd, &path, staged).is_none() => self.read_diff(cwd, path, staged),
                None => {}
            }
        }
    }

    /// The phone sheet's open diff, under the sheet's own view key.
    fn sheet_diff(&self) -> Option<(String, String, String, bool)> {
        if !self.overlays.is_open(GIT_SHEET) {
            return None;
        }
        let file = self.git.cache.selected_file.clone()?;
        Some((GIT_SHEET.to_owned(), self.git_cwd()?, file, self.git.selected_staged))
    }

    /// `DiffTab`'s `load`: one quiet read per key at a time.
    pub(crate) fn read_diff(&mut self, cwd: String, path: String, staged: bool) {
        let key = git::diff_key(&cwd, &path, staged);
        if self.git.reading.insert(key.clone()) {
            let quiet = Some(git::FetchDiffOptions { notify: Some(false) });
            self.git_action(Event::FetchDiff(cwd, path, staged, quiet), Then::Diff(key));
        }
    }

    pub(crate) fn git_stage(&mut self, paths: Vec<String>) {
        let Some(cwd) = self.git_cwd() else { return };
        if paths.is_empty() || self.git.busy() {
            return;
        }
        self.git.mutating = true;
        self.git_action(Event::StageFiles(cwd, paths), Then::Toast { ok: None, fail: "Failed to stage" });
    }

    pub(crate) fn git_unstage(&mut self, paths: Vec<String>) {
        let Some(cwd) = self.git_cwd() else { return };
        if paths.is_empty() || self.git.busy() {
            return;
        }
        self.git.mutating = true;
        self.git_action(Event::UnstageFiles(cwd, paths), Then::Toast { ok: None, fail: "Failed to unstage" });
    }

    /// `requestDiscard`: the confirm names the paths.
    pub(crate) fn git_request_discard(&mut self, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        self.git.discard = paths;
        self.overlays.open(DISCARD_CONFIRM);
    }

    pub(crate) fn git_confirm_discard(&mut self) {
        let Some(cwd) = self.git_cwd() else { return };
        let paths = self.git.discard.clone();
        if paths.is_empty() || self.git.mutating {
            return;
        }
        self.git.mutating = true;
        self.git_action(Event::DiscardFiles(cwd, paths.clone()), Then::Discard { paths });
    }

    /// `handleToggleAmend`: amending fills empty fields with the last commit's message, and
    /// turning it off clears fields that still hold it.
    pub(crate) fn git_toggle_amend(&mut self) {
        let context = self.git_cwd().and_then(|cwd| self.git.cache.commit_context(&cwd).cloned());
        let next = !self.git.amend;
        self.git.amend = next;
        let (subject, body) = context.as_ref().map_or((String::new(), String::new()), |c| (c.last_subject.clone(), c.last_body.clone()));
        if next && context.as_ref().is_some_and(|c| c.has_head) {
            if self.git.summary.text.trim().is_empty() && self.git.description.text.trim().is_empty() {
                self.git.summary = TextEdit::new(&subject);
                self.git.description = TextEdit::new(&body);
            }
        } else if !next && self.git.summary.text == subject && self.git.description.text == body {
            self.git.summary = TextEdit::default();
            self.git.description = TextEdit::default();
        }
    }

    /// `canCommit`.
    pub fn git_can_commit(&self) -> bool {
        let context = self.git_cwd().and_then(|cwd| self.git.cache.commit_context(&cwd).cloned());
        let staged = context.as_ref().map_or(0, |c| c.staged_count);
        !self.git.summary.text.trim().is_empty()
            && !self.git.committing
            && !self.git.pushing
            && if self.git.amend { context.is_some_and(|c| c.has_head) } else { staged > 0 }
    }

    /// `handleCommit`: amending a commit already pushed asks first.
    pub(crate) fn git_commit(&mut self, confirmed: bool) {
        let Some(cwd) = self.git_cwd() else { return };
        if !self.git_can_commit() {
            return;
        }
        let pushed = self.git.cache.commit_context(&cwd).is_some_and(|c| c.has_upstream && c.ahead == 0);
        if self.git.amend && pushed && !confirmed {
            self.overlays.open(AMEND_CONFIRM);
            return;
        }
        self.git.committing = true;
        let amend = self.git.amend;
        let event = Event::Commit(cwd, self.git.summary.text.clone(), self.git.description.text.clone(), amend);
        self.git_action(event, Then::Commit { amend });
    }

    pub(crate) fn git_push(&mut self) {
        let Some(cwd) = self.git_cwd() else { return };
        if self.git.pushing || self.git.committing {
            return;
        }
        self.git.pushing = true;
        self.git_action(Event::Push(cwd), Then::Push);
    }

    /// `handleSwitchBranch`: with uncommitted changes, ask how to carry them first.
    pub(crate) fn git_switch_branch(&mut self, name: String) {
        let Some(cwd) = self.git_cwd() else { return };
        self.overlays.close(BRANCH_MENU);
        if !self.git.cache.statuses(&cwd).is_empty() {
            self.git.pending_branch = Some(name);
            self.overlays.open(SWITCH_BRANCH);
            return;
        }
        self.git.mutating = true;
        let ok = Some(format!("Switched to branch {name}"));
        self.git_action(Event::BranchSwitch(cwd, name), Then::Toast { ok, fail: "Failed to switch branch" });
    }

    /// `handleExecuteSwitchBranch`: bring the changes along, or stash, switch and pop.
    pub(crate) fn git_execute_switch(&mut self, stash: bool) {
        let (Some(cwd), Some(name)) = (self.git_cwd(), self.git.pending_branch.take()) else {
            return;
        };
        self.overlays.close(SWITCH_BRANCH);
        self.git.mutating = true;
        if stash {
            let message = format!("Auto-stash before checkout to {name}");
            self.git_action(Event::StashSave(cwd.clone(), Some(message), Some(true)), Then::StashThenSwitch { cwd, name });
        } else {
            let ok = Some(format!("Switched to branch {name} (changes carried over)"));
            self.git_action(Event::BranchSwitch(cwd, name), Then::Toast { ok, fail: "Failed to switch branch" });
        }
    }

    pub(crate) fn git_create_branch(&mut self) {
        let Some(cwd) = self.git_cwd() else { return };
        let name = self.git.new_branch.text.trim().to_owned();
        if name.is_empty() {
            return;
        }
        self.overlays.close(CREATE_BRANCH);
        self.git.new_branch = TextEdit::default();
        self.git.mutating = true;
        let ok = Some(format!("Created and switched to branch {name}"));
        self.git_action(Event::BranchCreate(cwd, name), Then::Toast { ok, fail: "Failed to create branch" });
    }

    pub(crate) fn git_stash_save(&mut self) {
        let Some(cwd) = self.git_cwd() else { return };
        let message = Some(self.git.stash_message.text.trim().to_owned()).filter(|m| !m.is_empty());
        let untracked = self.git.stash_untracked;
        self.overlays.close(STASH_DIALOG);
        self.git.stash_message = TextEdit::default();
        self.git.mutating = true;
        let event = Event::StashSave(cwd, message, Some(untracked));
        self.git_action(event, Then::Toast { ok: Some("Changes stashed successfully".into()), fail: "Failed to stash changes" });
    }

    /// Apply, pop or drop stash `index`.
    pub(crate) fn git_stash(&mut self, op: StashOp, index: u32) {
        let Some(cwd) = self.git_cwd() else { return };
        self.overlays.close(STASH_MENU);
        self.git.mutating = true;
        let (event, done, fail) = match op {
            StashOp::Apply => (Event::StashApply(cwd, index), "applied", "Failed to apply stash"),
            StashOp::Pop => (Event::StashPop(cwd, index), "popped", "Failed to pop stash"),
            StashOp::Drop => (Event::StashDrop(cwd, index), "dropped", "Failed to drop stash"),
        };
        self.git_action(event, Then::Toast { ok: Some(format!("Stash@{{{index}}} {done}")), fail });
    }

    /// A file row's plain click: select it and open its diff tab. In the phone sheet the
    /// selection is the stack pointer: the sheet shows that file's diff in place.
    pub(crate) fn git_open_diff(&mut self, path: String, staged: bool) {
        let Some(cwd) = self.git_cwd() else { return };
        self.git_step(Event::SetSelectedFile(Some(path.clone())));
        self.git.selected_staged = staged;
        if self.overlays.is_open(GIT_SHEET) {
            self.read_diff(cwd, path, staged);
            return;
        }
        self.workspace.step(model::workspace::Event::AddDiffTab(cwd, path, staged, None));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StashOp {
    Apply,
    Pop,
    Drop,
}
