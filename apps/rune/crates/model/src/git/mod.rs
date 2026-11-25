//! The git cache of `stores/git-status-store.ts`: per working copy, the status list, the
//! commit context, the diff stat, the stash and branch lists, the diffs the panel read, and
//! the last failure of each read. Every map is keyed by `normalize_path(cwd)`.
//!
//! The store's actions are async chains of `gitApi` calls. Here each action is a task: `step`
//! starts it and returns [`Effect::Call`]s, the app runs each call against the host
//! (`Call::route`), and feeds the answer back as [`Event::Reply`]. A task that ends returns
//! [`Effect::Settled`] with what the TypeScript promise resolved or rejected with.
//!
//! Not ported: `refreshTracked` (the all-repo refresh pool; the Rune client refreshes the
//! working copy it shows), the write block for an unresolved managed worktree
//! (`isGitWriteBlocked`; the Rune client has no worktree resolution yet), and
//! `updateStoresWithBranch` (the project and terminal stores read the branch from the
//! commit context here).

mod selectors;
mod tasks;

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::Level;
use crate::workspace::normalize_path;

pub use selectors::{Counts, changed_file_count, count_statuses, deepest_tracked_key};
use tasks::Task;

/// `DIFF_STAT_MIN_INTERVAL_MS`: a watched diff stat is read at most this often.
pub const DIFF_STAT_MIN_INTERVAL_MS: f64 = 30_000.0;
/// The host's ledger verdict for a commit whose `expectedHead` moved.
pub const PRECONDITION_FAILED: &str = "precondition_failed";
/// The `source` of the store's log lines.
pub const LOG_SOURCE: &str = "git-status-store";

/// One changed file (`GitStatusDetail`). A file both staged and unstaged has two entries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusDetail {
    pub path: String,
    /// `modified`, `added`, `deleted`, `renamed`, `untracked`, `conflicted`.
    pub status: String,
    pub staged: bool,
}

/// `GitCommitContext`: what the commit footer needs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitContext {
    pub branch: Option<String>,
    pub has_upstream: bool,
    pub ahead: u32,
    pub behind: u32,
    pub staged_count: u32,
    pub has_head: bool,
    pub last_subject: String,
    pub last_body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiffStat {
    pub path: String,
    pub added: u32,
    pub removed: u32,
}

/// `GitDiffStat`: the working tree's line totals.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffStat {
    pub added: u32,
    pub removed: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<FileDiffStat>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StashInfo {
    pub index: u32,
    pub name: String,
    pub message: String,
}

/// `GitStatusErrors`: the last failure of each read, cleared by its next success.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Errors {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_stat: Option<String>,
}

impl Errors {
    fn slot(&mut self, field: ErrorField) -> &mut Option<String> {
        match field {
            ErrorField::Status => &mut self.status,
            ErrorField::CommitContext => &mut self.commit_context,
            ErrorField::DiffStat => &mut self.diff_stat,
        }
    }

    fn is_empty(&self) -> bool {
        self.status.is_none() && self.commit_context.is_none() && self.diff_stat.is_none()
    }
}

#[derive(Clone, Copy)]
enum ErrorField {
    Status,
    CommitContext,
    DiffStat,
}

/// The store's data, as the recorder snapshots it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// The first display path seen per key, usable as a cwd.
    pub paths: BTreeMap<String, String>,
    pub statuses: BTreeMap<String, Vec<StatusDetail>>,
    /// By [`diff_key`]. `""` is the sentinel a failed fetch leaves.
    pub diffs: BTreeMap<String, String>,
    pub diff_errors: BTreeMap<String, String>,
    pub commit_contexts: BTreeMap<String, CommitContext>,
    pub diff_stats: BTreeMap<String, DiffStat>,
    pub errors: BTreeMap<String, Errors>,
    pub stashes: BTreeMap<String, Vec<StashInfo>>,
    pub branches: BTreeMap<String, Vec<String>>,
    pub selected_file: Option<String>,
    pub is_fetching_status: bool,
    pub status_fetch_count: u32,
    #[serde(skip)]
    book: Book,
}

/// What the store keeps outside its state: request tokens, the HEAD each commit expects,
/// the diff-stat watchers and throttle, and the running tasks.
#[derive(Clone, Debug, Default)]
struct Book {
    status_requests: HashMap<String, u64>,
    context_requests: HashMap<String, u64>,
    diff_versions: HashMap<String, u64>,
    /// `Some("")`: unborn branch. `None`: HEAD could not be read.
    known_heads: HashMap<String, Option<String>>,
    watchers: HashMap<String, u32>,
    last_fetch: HashMap<String, f64>,
    trailing: HashSet<String>,
    tasks: BTreeMap<TaskId, Task>,
    next_task: TaskId,
}

pub type TaskId = u64;

/// `fetchDiff`'s options: `notify: false` keeps the cache empty and skips the toast.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct FetchDiffOptions {
    #[serde(default)]
    pub notify: Option<bool>,
}

/// A failed host call: the message, and the ledger code when the host gave one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl Failure {
    pub fn new(message: impl Into<String>) -> Failure {
        Failure { message: message.into(), code: None }
    }

    /// `String(error)`: the error's name, then its message.
    #[must_use]
    pub fn shown(&self) -> String {
        let name = if self.code.is_some() { "AcpTransportError" } else { "Error" };
        format!("{name}: {}", self.message)
    }

    fn is_precondition_failed(&self) -> bool {
        self.code.as_deref() == Some(PRECONDITION_FAILED)
    }
}

/// What a task's promise settled with: `Ok(None)` resolved with nothing.
pub type Outcome = Result<Option<Value>, Failure>;

/// The store's actions, and the host's answers to the calls they made.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Event {
    SetSelectedFile(Option<String>),
    RefreshStatus(String),
    RefreshDiffStat(String),
    /// A ref-counted opt-in to diff-stat reads for the working copy.
    WatchDiffStat(String),
    /// Releases one [`Event::WatchDiffStat`].
    UnwatchDiffStat(String),
    RefreshWorkingCopy(String),
    FetchDiff(String, String, bool, #[serde(default)] Option<FetchDiffOptions>),
    FetchCommitContext(String),
    FetchStashes(String),
    FetchBranches(String),
    StageFile(String, String),
    UnstageFile(String, String),
    DiscardFile(String, String),
    StageFiles(String, Vec<String>),
    UnstageFiles(String, Vec<String>),
    DiscardFiles(String, Vec<String>),
    StageHunk(String, String, String),
    UnstageHunk(String, String, String),
    Commit(String, String, String, bool),
    Push(String),
    StashSave(String, #[serde(default)] Option<String>, #[serde(default)] Option<bool>),
    StashApply(String, u32),
    StashPop(String, u32),
    StashDrop(String, u32),
    BranchSwitch(String, String),
    BranchCreate(String, String),
    /// The host answered a task's [`Call`].
    #[serde(skip)]
    Reply(TaskId, Result<Value, Failure>),
    /// The trailing diff-stat read [`Effect::ScheduleDiffStat`] asked for came due.
    #[serde(skip)]
    DiffStatDue(String),
}

/// One `gitApi` call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    Status { cwd: String },
    DiffStat { cwd: String },
    Diff { cwd: String, path: String, staged: bool },
    CommitContext { cwd: String },
    /// `getLog(cwd, 1)`: the HEAD sha a commit expects.
    Head { cwd: String },
    Stage { cwd: String, path: String },
    Unstage { cwd: String, path: String },
    Discard { cwd: String, path: String },
    StageHunk { cwd: String, path: String, hunk_patch: String },
    UnstageHunk { cwd: String, path: String, hunk_patch: String },
    Commit { cwd: String, summary: String, description: String, amend: bool, expected_head: String },
    Push { cwd: String },
    StashList { cwd: String },
    BranchList { cwd: String },
    StashSave { cwd: String, message: Option<String>, include_untracked: Option<bool> },
    StashApply { cwd: String, index: u32 },
    StashPop { cwd: String, index: u32 },
    StashDrop { cwd: String, index: u32 },
    BranchSwitch { cwd: String, name: String },
    CreateBranch { cwd: String, name: String },
}

/// An HTTP route of `webServerGit`.
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub post: bool,
    pub path: String,
    pub body: Option<Value>,
}

impl Call {
    /// The `gitApi` method and its arguments, as the recorder's mock saw them.
    #[must_use]
    pub fn api(&self) -> (&'static str, Vec<Value>) {
        match self {
            Call::Status { cwd } => ("getStatus", vec![json!(cwd)]),
            Call::DiffStat { cwd } => ("getDiffStat", vec![json!(cwd)]),
            Call::Diff { cwd, path, staged } => ("getDiff", vec![json!(cwd), json!(path), json!(staged)]),
            Call::CommitContext { cwd } => ("getCommitContext", vec![json!(cwd)]),
            Call::Head { cwd } => ("getLog", vec![json!(cwd), json!(1)]),
            Call::Stage { cwd, path } => ("stage", vec![json!(cwd), json!(path)]),
            Call::Unstage { cwd, path } => ("unstage", vec![json!(cwd), json!(path)]),
            Call::Discard { cwd, path } => ("discard", vec![json!(cwd), json!(path)]),
            Call::StageHunk { cwd, path, hunk_patch } => ("stageHunk", vec![json!(cwd), json!(path), json!(hunk_patch)]),
            Call::UnstageHunk { cwd, path, hunk_patch } => ("unstageHunk", vec![json!(cwd), json!(path), json!(hunk_patch)]),
            Call::Commit { cwd, summary, description, amend, expected_head } => (
                "commit",
                vec![json!(cwd), json!(summary), json!(description), json!(amend), json!(expected_head)],
            ),
            Call::Push { cwd } => ("push", vec![json!(cwd)]),
            Call::StashList { cwd } => ("stashList", vec![json!(cwd)]),
            Call::BranchList { cwd } => ("branchList", vec![json!(cwd)]),
            Call::StashSave { cwd, message, include_untracked } => {
                ("stashSave", vec![json!(cwd), json!(message), json!(include_untracked)])
            }
            Call::StashApply { cwd, index } => ("stashApply", vec![json!(cwd), json!(index)]),
            Call::StashPop { cwd, index } => ("stashPop", vec![json!(cwd), json!(index)]),
            Call::StashDrop { cwd, index } => ("stashDrop", vec![json!(cwd), json!(index)]),
            Call::BranchSwitch { cwd, name } => ("branchSwitch", vec![json!(cwd), json!(name)]),
            Call::CreateBranch { cwd, name } => ("createBranch", vec![json!(cwd), json!(name)]),
        }
    }

    /// The server route `webServerGit` sends this call to.
    #[must_use]
    pub fn route(&self) -> Route {
        let post = |path: &str, body: Value| Route { post: true, path: path.to_owned(), body: Some(body) };
        let get = |path: &str, cwd: &str| Route {
            post: false,
            path: format!("{path}?cwd={}", encode_component(cwd)),
            body: None,
        };
        match self {
            Call::Status { cwd } => post("/git/status", json!({ "cwd": cwd })),
            Call::DiffStat { cwd } => post("/git/diff-stat", json!({ "cwd": cwd })),
            Call::Diff { cwd, path, staged } => post("/git/diff", json!({ "cwd": cwd, "path": path, "staged": staged })),
            Call::CommitContext { cwd } => post("/git/commit-context", json!({ "cwd": cwd })),
            Call::Head { cwd } => post("/git/log", json!({ "cwd": cwd, "limit": 1 })),
            Call::Stage { cwd, path } => post("/git/stage", json!({ "cwd": cwd, "path": path })),
            Call::Unstage { cwd, path } => post("/git/unstage", json!({ "cwd": cwd, "path": path })),
            Call::Discard { cwd, path } => post("/git/discard", json!({ "cwd": cwd, "path": path })),
            Call::StageHunk { cwd, path, hunk_patch } => {
                post("/git/stage-hunk", json!({ "cwd": cwd, "path": path, "hunkPatch": hunk_patch }))
            }
            Call::UnstageHunk { cwd, path, hunk_patch } => {
                post("/git/unstage-hunk", json!({ "cwd": cwd, "path": path, "hunkPatch": hunk_patch }))
            }
            Call::Commit { cwd, summary, description, amend, expected_head } => post(
                "/git/commit",
                json!({ "cwd": cwd, "summary": summary, "description": description, "amend": amend, "expectedHead": expected_head }),
            ),
            Call::Push { cwd } => post("/git/push", json!({ "cwd": cwd })),
            Call::StashList { cwd } => get("/git/stash-list", cwd),
            Call::BranchList { cwd } => get("/git/branch-list", cwd),
            Call::StashSave { cwd, message, include_untracked } => {
                let mut body = json!({ "cwd": cwd });
                if let Some(message) = message {
                    body["message"] = json!(message);
                }
                if let Some(include) = include_untracked {
                    body["includeUntracked"] = json!(include);
                }
                post("/git/stash-save", body)
            }
            Call::StashApply { cwd, index } => post("/git/stash-apply", json!({ "cwd": cwd, "index": index })),
            Call::StashPop { cwd, index } => post("/git/stash-pop", json!({ "cwd": cwd, "index": index })),
            Call::StashDrop { cwd, index } => post("/git/stash-drop", json!({ "cwd": cwd, "index": index })),
            Call::BranchSwitch { cwd, name } => post("/git/branch-switch", json!({ "cwd": cwd, "name": name })),
            Call::CreateBranch { cwd, name } => post("/git/create-branch", json!({ "cwd": cwd, "branch": name })),
        }
    }
}

/// `encodeURIComponent`.
#[must_use]
pub fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// What a step asks the app to do, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Run `call` and answer with [`Event::Reply`] for `task`.
    Call { task: TaskId, call: Call },
    Log { level: Level, message: String },
    /// `toast.error`.
    Toast { message: String },
    /// `reconcileWorktreeOwnerOf`: re-list the worktrees that own `cwd`.
    ReconcileWorktree { cwd: String },
    /// Send [`Event::DiffStatDue`] at `at` (milliseconds on the step clock).
    ScheduleDiffStat { cwd: String, at: f64 },
    /// An action's task ended.
    Settled { task: TaskId, outcome: Outcome },
}

/// What one step did: the task it started, if any, and the effects.
#[derive(Debug, Default)]
pub struct Step {
    pub task: Option<TaskId>,
    pub effects: Vec<Effect>,
}

/// `diffKey`: the staged and unstaged rows of one file get their own entry.
#[must_use]
pub fn diff_key(cwd: &str, path: &str, staged: bool) -> String {
    format!("{}:{path}:{staged}", normalize_path(cwd))
}

/// Applies one event. `now` is the clock in milliseconds (`Date.now()`).
pub fn step(state: &mut State, event: Event, now: f64) -> Step {
    let mut cx = tasks::Cx { state, effects: Vec::new(), now };
    let task = match event {
        Event::SetSelectedFile(path) => {
            cx.state.selected_file = path;
            None
        }
        Event::WatchDiffStat(cwd) => {
            let key = cx.track(&cwd);
            *cx.state.book.watchers.entry(key).or_default() += 1;
            None
        }
        Event::UnwatchDiffStat(cwd) => {
            let key = normalize_path(&cwd);
            let book = &mut cx.state.book;
            match book.watchers.get(&key).copied().unwrap_or(1) {
                remaining if remaining > 1 => {
                    book.watchers.insert(key, remaining - 1);
                }
                _ => {
                    book.watchers.remove(&key);
                    book.trailing.remove(&key);
                }
            }
            None
        }
        Event::Reply(task, result) => {
            cx.reply(task, result);
            None
        }
        Event::DiffStatDue(cwd) => {
            let key = normalize_path(&cwd);
            if cx.state.book.trailing.remove(&key) {
                cx.start(Task::diff_stat(&cwd), None);
            }
            None
        }
        action => Some(cx.start(Task::of(action), None)),
    };
    Step { task, effects: cx.effects }
}

impl State {
    /// The cached commit context's branch: `None` on a detached HEAD or with nothing read.
    #[must_use]
    pub fn branch(&self, cwd: &str) -> Option<&str> {
        self.commit_contexts.get(&normalize_path(cwd))?.branch.as_deref()
    }

    #[must_use]
    pub fn commit_context(&self, cwd: &str) -> Option<&CommitContext> {
        self.commit_contexts.get(&normalize_path(cwd))
    }

    #[must_use]
    pub fn statuses(&self, cwd: &str) -> &[StatusDetail] {
        self.statuses.get(&normalize_path(cwd)).map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn diff_stat(&self, cwd: &str) -> Option<&DiffStat> {
        self.diff_stats.get(&normalize_path(cwd))
    }

    #[must_use]
    pub fn errors(&self, cwd: &str) -> Errors {
        self.errors.get(&normalize_path(cwd)).cloned().unwrap_or_default()
    }

    #[must_use]
    pub fn stashes(&self, cwd: &str) -> &[StashInfo] {
        self.stashes.get(&normalize_path(cwd)).map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn branches(&self, cwd: &str) -> &[String] {
        self.branches.get(&normalize_path(cwd)).map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn diff(&self, cwd: &str, path: &str, staged: bool) -> Option<&str> {
        self.diffs.get(&diff_key(cwd, path, staged)).map(String::as_str)
    }

    #[must_use]
    pub fn diff_error(&self, cwd: &str, path: &str, staged: bool) -> Option<&str> {
        self.diff_errors.get(&diff_key(cwd, path, staged)).map(String::as_str)
    }

    /// Whether a task is still waiting on the host.
    #[must_use]
    pub fn running(&self, task: TaskId) -> bool {
        self.book.tasks.contains_key(&task)
    }

    fn patch_error(&mut self, key: &str, field: ErrorField, message: Option<String>) {
        let mut errors = self.errors.remove(key).unwrap_or_default();
        *errors.slot(field) = message;
        if !errors.is_empty() {
            self.errors.insert(key.to_owned(), errors);
        }
    }

    fn invalidate_file_diffs(&mut self, cwd: &str, path: &str) {
        for staged in [true, false] {
            let key = diff_key(cwd, path, staged);
            *self.book.diff_versions.entry(key.clone()).or_default() += 1;
            self.diffs.remove(&key);
            self.diff_errors.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests;
